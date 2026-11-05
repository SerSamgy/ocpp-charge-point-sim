//! Staying connected to the CSMS: connect, BootNotification until Accepted,
//! report the status of the charge point and its connector, Heartbeat, and
//! reconnect with back-off whenever the connection is lost. After every
//! Accepted and after every reconnect that skips the boot, the status is
//! reported again (OCPP 1.6 errata 3.22).

use std::{num::NonZeroU32, time::Duration};

use rand::rngs::StdRng;
use time::UtcDateTime;
use tokio::{
    task::JoinHandle,
    time::{MissedTickBehavior, interval, sleep, timeout},
};
use tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode;

use crate::{
    backoff::{Backoff, BackoffConfig},
    boot::{self, BootStep},
    client::{CallFailure, Client, Timeouts},
    connector::Connector,
    ocpp16::{BootNotificationRequest, HeartbeatRequest},
    status16,
};

/// The one connector of the simulated charge point.
// The `unwrap` runs at compile time, and 1 is not zero, so it cannot panic.
const CONNECTOR_ID: NonZeroU32 = NonZeroU32::new(1).unwrap();

#[derive(Debug, Clone, Copy)]
pub struct Settings {
    pub timeouts: Timeouts,
    pub backoff: BackoffConfig,
    /// Wait before the next BootNotification after one that failed
    /// (timeout, CALLERROR, malformed response) on a live connection.
    pub boot_retry: Duration,
    /// How long to wait for the server to answer our Close frame before
    /// dropping the connection.
    pub close_timeout: Duration,
}

/// Runs the charge point until `shutdown` completes.
/// - Connection failures and lost connections: reconnect after a back-off
///   wait (see `backoff`).
/// - BootNotification until Accepted; after that, reconnects skip it, as
///   OCPP-J section 5.4 recommends (1.6 and 2.0.1 alike): our boot data
///   never changes while the program runs.
/// - After Accepted, and after every reconnect that skips the boot, report
///   connector 0 and the connector with their current status (errata 3.22).
///   `started` is the time of the program start: the instant every status
///   was reached so far. It is fixed for the whole run, so a Pi that starts
///   before NTP has synchronised reports that wrong time until a restart
///   (roadmap backlog, "Trust in `timestamp`").
/// - A failed CALL that leaves the connection up is logged and retried.
/// - On `shutdown`: close an open connection with 1001 Going Away (RFC 6455
///   section 7.4.1) and return within `close_timeout`; a CALL in flight is
///   not waited for. While connecting or waiting to reconnect, return at once.
pub async fn run(
    url: &str,
    settings: Settings,
    request: &BootNotificationRequest,
    started: UtcDateTime,
    shutdown: impl Future<Output = ()>,
) {
    // `select!` polls `shutdown` by `&mut` in several places, and a future
    // polled through a reference must not move: pin it once, here.
    tokio::pin!(shutdown);
    let mut station = Station {
        settings,
        request,
        heartbeat: None,
        started,
        connectors: vec![Connector::new(CONNECTOR_ID, started)],
        backoff: Backoff::new(settings.backoff),
        // `StdRng` is `Send`, unlike `rand::rng()`, so the future of `run`
        // can move between Tokio worker threads.
        rng: rand::make_rng(),
    };
    loop {
        let connected = tokio::select! {
            connected = Client::connect(url, settings.timeouts) => connected,
            () = &mut shutdown => return,
        };
        let wait = match connected {
            Ok((client, mut reader)) => {
                println!("connected to {url}");
                // On shutdown the `serve` future is dropped wherever it waits,
                // which cancels it. That is safe: a CALL in flight leaves an
                // entry in `pending`, which `close` fails; a frame whose send
                // had not reached tungstenite yet is never sent, one that had
                // goes out before our Close.
                let ended = tokio::select! {
                    ended = station.serve(&client, &mut reader) => ended,
                    () = &mut shutdown => {
                        println!("closing the connection");
                        disconnect(&client, &mut reader, settings.close_timeout, CloseCode::Away)
                            .await;
                        return;
                    }
                };
                match ended {
                    Ended::Lost => {
                        // Usually finished already; if the socket broke only on
                        // the sending side, stop the reader so it does not linger.
                        reader.abort();
                        station.backoff.next_delay(&mut station.rng)
                    }
                    Ended::Rejected { after } => {
                        println!("boot rejected, reconnecting in {after:?}");
                        // Not under `select!` with `shutdown`: a signal that
                        // cancelled this close half way would leave a second
                        // full `close_timeout` for the shutdown close.
                        disconnect(
                            &client,
                            &mut reader,
                            settings.close_timeout,
                            CloseCode::Normal,
                        )
                        .await;
                        after
                    }
                }
            }
            Err(err) => {
                eprintln!("cannot connect to {url}: {err}");
                station.backoff.next_delay(&mut station.rng)
            }
        };
        println!("reconnecting in {wait:?}");
        tokio::select! {
            () = sleep(wait) => {}
            () = &mut shutdown => return,
        }
    }
}

/// What survives between connections.
struct Station<'a> {
    settings: Settings,
    request: &'a BootNotificationRequest,
    /// Heartbeat interval from the last Accepted boot; `None` until then.
    heartbeat: Option<Duration>,
    /// When the program started; the time of the status of connector 0.
    started: UtcDateTime,
    connectors: Vec<Connector>,
    backoff: Backoff,
    rng: StdRng,
}

/// Why a connection ended.
enum Ended {
    /// It broke or the server closed it.
    Lost,
    /// Boot rejected: stay away for `after`, then connect and boot again.
    Rejected { after: Duration },
}

impl Station<'_> {
    /// Uses one connection until it ends and says why it ended; closing
    /// the connection and waiting before the next one is up to `run`.
    async fn serve(&mut self, client: &Client, reader: &mut JoinHandle<()>) -> Ended {
        match self.heartbeat {
            Some(every) => self.keep_alive(client, reader, every).await,
            None => match self.boot(client, reader).await {
                Ok(every) => {
                    println!("accepted, heartbeat every {every:?}");
                    self.heartbeat = Some(every);
                    self.keep_alive(client, reader, every).await
                }
                Err(ended) => ended,
            },
        }
    }

    /// Sends BootNotification until Accepted and returns the heartbeat interval.
    /// - Pending, or a failed CALL: wait on the same connection and boot again.
    /// - Rejected or lost connection: `Err`.
    async fn boot(
        &mut self,
        client: &Client,
        reader: &mut JoinHandle<()>,
    ) -> Result<Duration, Ended> {
        loop {
            let wait = match client.call(self.request).await {
                Ok(response) => {
                    self.backoff.reset();
                    match boot::next_step(&response) {
                        BootStep::Accepted { heartbeat } => return Ok(heartbeat),
                        BootStep::RetryOnSameConnection { after } => {
                            println!("boot pending, next attempt in {after:?}");
                            after
                        }
                        BootStep::Reconnect { after } => return Err(Ended::Rejected { after }),
                    }
                }
                Err(CallFailure::Disconnected) => return Err(Ended::Lost),
                Err(err) => {
                    let after = self.settings.boot_retry;
                    eprintln!("BootNotification failed: {err}; next attempt in {after:?}");
                    after
                }
            };
            tokio::select! {
                _ = sleep(wait) => {}
                _ = &mut *reader => return Err(Ended::Lost),
            }
        }
    }

    /// Reports the status, then sends Heartbeat every `every` until the
    /// connection is lost. A failed Heartbeat is logged; the next one goes
    /// out on schedule. `every` is never zero (`boot::next_step` replaces 0
    /// with a default), so `interval` does not panic.
    ///
    /// The ticker starts before the reports, so the Heartbeat cadence counts
    /// from the moment the connection became usable, and slow reports
    /// (each may wait up to the call timeout) do not push it further out.
    async fn keep_alive(
        &mut self,
        client: &Client,
        reader: &mut JoinHandle<()>,
        every: Duration,
    ) -> Ended {
        let mut ticker = interval(every);
        // After a call that hung until its timeout, do not fire the missed
        // ticks in a burst; just start counting again.
        ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);
        ticker.tick().await; // first tick completes immediately — skip it
        if let Err(ended) = self.report_all(client).await {
            return ended;
        }
        loop {
            tokio::select! {
                _ = ticker.tick() => match client.call(&HeartbeatRequest {}).await {
                    Ok(_) => self.backoff.reset(),
                    Err(CallFailure::Disconnected) => return Ended::Lost,
                    Err(err) => eprintln!("Heartbeat failed: {err}"),
                },
                // The reader task ends when the connection is closed.
                _ = &mut *reader => return Ended::Lost,
            }
        }
    }

    /// Sends the status of connector 0 and of every connector, in the order
    /// `status16::report` gives (OCPP 1.6 errata 3.22: after Accepted; and
    /// after a reconnect, section 4.9). A success resets the back-off: after
    /// a reconnect without boot this is the first exchange. A failed report
    /// is logged and the next one still goes out; a lost connection is `Err`,
    /// and the next connection reports everything again.
    async fn report_all(&mut self, client: &Client) -> Result<(), Ended> {
        for request in status16::report(self.started, &self.connectors) {
            match client.call(&request).await {
                Ok(_) => self.backoff.reset(),
                Err(CallFailure::Disconnected) => return Err(Ended::Lost),
                Err(err) => eprintln!(
                    "StatusNotification for connector {} failed: {err}",
                    request.connector_id
                ),
            }
        }
        Ok(())
    }
}

/// Closes the connection with `code` and waits for the reader task to finish;
/// aborts it if the server does not complete the closing handshake.
/// Everything, sending the Close included, takes at most `wait`: the send
/// can hang too, on the `sink` lock held by the reader task or on a socket
/// whose peer stopped reading.
/// Returns true if the reader finished on its own.
async fn disconnect(
    client: &Client,
    reader: &mut JoinHandle<()>,
    wait: Duration,
    code: CloseCode,
) -> bool {
    let closing = async {
        if let Err(err) = client.close(code).await {
            eprintln!("failed to send Close: {err}");
        }
        // `boot` and `keep_alive` may have awaited the reader to the end
        // already, and polling a finished `JoinHandle` again panics.
        if !reader.is_finished() {
            let _ = (&mut *reader).await;
        }
    };
    if timeout(wait, closing).await.is_ok() {
        return true;
    }
    eprintln!("server did not finish the closing handshake, dropping the connection");
    reader.abort();
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        frame::Frame,
        test_support::{Event, FakeCsms, echo_ocpp16},
    };
    use futures_util::StreamExt;
    use serde_json::json;
    use std::sync::Arc;
    use tokio::{
        net::TcpListener,
        sync::{mpsc, oneshot},
        time::Instant,
    };
    use tokio_tungstenite::accept_hdr_async;

    const LONG: Duration = Duration::from_secs(5);
    const SHORT: Duration = Duration::from_millis(300);
    /// `wait_minimum` in tests; `repeat_times = 1` caps waits at 2 · MIN.
    const MIN: Duration = Duration::from_millis(200);
    /// Room for a heartbeat (1 s, the smallest whole-second interval) plus scheduling.
    const HEARTBEAT_WAIT: Duration = Duration::from_secs(3);

    fn settings(call_timeout: Duration) -> Settings {
        Settings {
            timeouts: Timeouts {
                connect: LONG,
                call: call_timeout,
            },
            backoff: BackoffConfig {
                wait_minimum: MIN,
                random_range: Duration::ZERO,
                repeat_times: 1,
            },
            boot_retry: MIN,
            // Long, so a quick shutdown proves that the server answered our
            // Close instead of `disconnect` giving up on it.
            close_timeout: LONG,
        }
    }

    fn boot_request() -> BootNotificationRequest {
        BootNotificationRequest {
            charge_point_vendor: "Semyon".into(),
            charge_point_model: "cp-sim".into(),
            charge_point_serial_number: None,
            firmware_version: None,
        }
    }

    /// Runs the charge point in the background; the runtime stops it when
    /// the test ends.
    fn spawn_run(url: &str, settings: Settings) -> JoinHandle<()> {
        spawn_run_until(url, settings, std::future::pending())
    }

    fn spawn_run_until(
        url: &str,
        settings: Settings,
        shutdown: impl Future<Output = ()> + Send + 'static,
    ) -> JoinHandle<()> {
        let url = url.to_owned();
        tokio::spawn(async move {
            run(&url, settings, &boot_request(), STARTED, shutdown).await;
        })
    }

    /// A shutdown signal for `run`: send on the sender to stop. Dropping
    /// the sender stops `run` too, so keep it until the signal is due.
    fn shutdown_signal() -> (
        oneshot::Sender<()>,
        impl Future<Output = ()> + Send + 'static,
    ) {
        let (stop, rx) = oneshot::channel();
        (stop, async move {
            let _ = rx.await;
        })
    }

    /// Waits for `run` to return; returns how long that took.
    async fn time_to_stop(cp: JoinHandle<()>) -> Duration {
        let started = Instant::now();
        timeout(LONG + Duration::from_secs(1), cp)
            .await
            .expect("run did not stop")
            .expect("run panicked");
        started.elapsed()
    }

    /// Asserts that the next event is a Close from the client with `code`.
    async fn assert_closed_with(csms: &mut FakeCsms, code: u16) {
        let closed = csms.recv_event().await;
        assert!(
            matches!(closed, Event::Closed { code: Some(c) } if c == code),
            "got {closed:?}"
        );
    }

    /// Asserts that the event is a CALL of `action` and returns its ID.
    fn call_id(event: Event, action: &str) -> String {
        match event {
            Event::Frame(Frame::Call { id, action: a, .. }) if a == action => id,
            other => panic!("expected a {action} CALL, got {other:?}"),
        }
    }

    fn boot_result(id: &str, status: &str, interval: u64) -> Frame {
        Frame::CallResult {
            id: id.into(),
            payload: json!({
                "status": status,
                "currentTime": "2026-10-29T12:00:00Z",
                "interval": interval,
            }),
        }
    }

    fn heartbeat_result(id: &str) -> Frame {
        Frame::CallResult {
            id: id.into(),
            payload: json!({"currentTime": "2026-10-29T12:00:00Z"}),
        }
    }

    fn call_error(id: &str) -> Frame {
        Frame::CallError {
            id: id.into(),
            code: "InternalError".into(),
            description: "try later".into(),
            details: json!({}),
        }
    }

    /// The fixed program start of the tests: the time of every report.
    const STARTED: UtcDateTime = UtcDateTime::UNIX_EPOCH;
    const STARTED_TEXT: &str = "1970-01-01T00:00:00Z";

    /// Takes the client through a first connection and an Accepted boot
    /// with a 1 s heartbeat; leaves the reports that follow unanswered.
    /// Returns the instant the Accepted answer was sent.
    async fn accept_boot(csms: &mut FakeCsms) -> Instant {
        assert!(matches!(csms.recv_event().await, Event::Connected));
        let id = call_id(csms.recv_event().await, "BootNotification");
        csms.send(boot_result(&id, "Accepted", 1));
        Instant::now()
    }

    /// `accept_boot`, then answers the two reports (see `answer_reports`).
    async fn connect_and_accept(csms: &mut FakeCsms) -> Instant {
        let accepted = accept_boot(csms).await;
        answer_reports(csms).await;
        accepted
    }

    /// The next event must be a StatusNotification CALL; returns its ID and
    /// payload.
    async fn recv_report(csms: &mut FakeCsms) -> (String, serde_json::Value) {
        match csms.recv_event().await {
            Event::Frame(Frame::Call {
                id,
                action,
                payload,
            }) if action == "StatusNotification" => (id, payload),
            other => panic!("expected a StatusNotification CALL, got {other:?}"),
        }
    }

    fn report_payload(connector_id: u32) -> serde_json::Value {
        json!({
            "connectorId": connector_id,
            "errorCode": "NoError",
            "status": "Available",
            "timestamp": STARTED_TEXT,
        })
    }

    fn empty_result(id: &str) -> Frame {
        Frame::CallResult {
            id: id.into(),
            payload: json!({}),
        }
    }

    /// Receives the two reports that follow Accepted (connector 0, then
    /// connector 1), checks them and answers each with an empty result.
    async fn answer_reports(csms: &mut FakeCsms) {
        for connector_id in [0, 1] {
            let (id, payload) = recv_report(csms).await;
            assert_eq!(payload, report_payload(connector_id));
            csms.send(empty_result(&id));
        }
    }

    /// Waits for the client to come back after `Event::Closed`; returns how
    /// long that took.
    async fn reconnect_gap(csms: &mut FakeCsms) -> Duration {
        assert!(matches!(csms.recv_event().await, Event::Closed { .. }));
        let closed = Instant::now();
        assert!(matches!(
            csms.recv_event_within(LONG).await,
            Event::Connected
        ));
        closed.elapsed()
    }

    // --- Boot ---

    #[tokio::test]
    async fn accepted_starts_heartbeat_at_interval() {
        let mut csms = FakeCsms::start().await;
        let _cp = spawn_run(&csms.url, settings(LONG));

        let accepted = connect_and_accept(&mut csms).await;

        call_id(csms.recv_event_within(HEARTBEAT_WAIT).await, "Heartbeat");
        assert!(
            accepted.elapsed() >= Duration::from_secs(1),
            "heartbeat before the interval: {:?}",
            accepted.elapsed()
        );
    }

    #[tokio::test]
    async fn pending_boots_again_on_same_connection_after_interval() {
        let mut csms = FakeCsms::start().await;
        let _cp = spawn_run(&csms.url, settings(LONG));

        assert!(matches!(csms.recv_event().await, Event::Connected));
        let id = call_id(csms.recv_event().await, "BootNotification");
        csms.send(boot_result(&id, "Pending", 1));
        let answered = Instant::now();

        // The very next event must be the second boot: no reconnect, no other CALL.
        call_id(
            csms.recv_event_within(HEARTBEAT_WAIT).await,
            "BootNotification",
        );
        assert!(
            answered.elapsed() >= Duration::from_secs(1),
            "booted again before the interval: {:?}",
            answered.elapsed()
        );
    }

    #[tokio::test]
    async fn rejected_reconnects_after_interval_and_boots_again() {
        let mut csms = FakeCsms::start().await;
        let _cp = spawn_run(&csms.url, settings(LONG));

        assert!(matches!(csms.recv_event().await, Event::Connected));
        let id = call_id(csms.recv_event().await, "BootNotification");
        csms.send(boot_result(&id, "Rejected", 1));
        let answered = Instant::now();

        // The client closes the connection instead of staying silent on it.
        let closed = csms.recv_event().await;
        assert!(
            matches!(closed, Event::Closed { code: Some(1000) }),
            "got {closed:?}"
        );
        assert!(matches!(
            csms.recv_event_within(HEARTBEAT_WAIT).await,
            Event::Connected
        ));
        assert!(
            answered.elapsed() >= Duration::from_secs(1),
            "reconnected before the interval: {:?}",
            answered.elapsed()
        );
        call_id(csms.recv_event().await, "BootNotification");
    }

    #[tokio::test]
    async fn failed_boot_is_retried_on_same_connection() {
        let mut csms = FakeCsms::start().await;
        let _cp = spawn_run(&csms.url, settings(LONG));

        assert!(matches!(csms.recv_event().await, Event::Connected));
        let id = call_id(csms.recv_event().await, "BootNotification");
        csms.send(call_error(&id));
        let answered = Instant::now();

        call_id(csms.recv_event().await, "BootNotification");
        assert!(
            answered.elapsed() >= MIN,
            "booted again before boot_retry: {:?}",
            answered.elapsed()
        );
    }

    // --- Lost connections ---

    #[tokio::test]
    async fn after_accepted_reconnects_without_boot() {
        let mut csms = FakeCsms::start().await;
        let _cp = spawn_run(&csms.url, settings(LONG));
        connect_and_accept(&mut csms).await;

        csms.close_connection();
        let gap = reconnect_gap(&mut csms).await;
        assert!(gap >= MIN, "reconnected before the back-off: {gap:?}");

        // Section 5.4: no BootNotification, just report and carry on with
        // Heartbeat.
        answer_reports(&mut csms).await;
        call_id(csms.recv_event_within(HEARTBEAT_WAIT).await, "Heartbeat");
    }

    #[tokio::test]
    async fn connection_lost_while_pending_boots_again() {
        let mut csms = FakeCsms::start().await;
        let _cp = spawn_run(&csms.url, settings(LONG));

        assert!(matches!(csms.recv_event().await, Event::Connected));
        let id = call_id(csms.recv_event().await, "BootNotification");
        csms.send(boot_result(&id, "Pending", 30));
        csms.close_connection();

        reconnect_gap(&mut csms).await;
        call_id(csms.recv_event().await, "BootNotification");
    }

    #[tokio::test]
    async fn backoff_grows_while_connect_fails() {
        // Accepts TCP and drops it at once, so every handshake fails.
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("ws://{}/CP001", listener.local_addr().unwrap());
        let (attempts_tx, mut attempts) = mpsc::unbounded_channel();
        tokio::spawn(async move {
            while let Ok((tcp, _)) = listener.accept().await {
                drop(tcp);
                let _ = attempts_tx.send(Instant::now());
            }
        });
        let _cp = spawn_run(&url, settings(LONG));

        let mut times = Vec::new();
        for _ in 0..4 {
            times.push(timeout(LONG, attempts.recv()).await.unwrap().unwrap());
        }
        let gaps: Vec<Duration> = times.windows(2).map(|w| w[1] - w[0]).collect();
        // MIN, then doubled once (repeat_times = 1), then capped.
        assert!(gaps[0] >= MIN, "gaps: {gaps:?}");
        assert!(gaps[1] >= 2 * MIN, "gaps: {gaps:?}");
        assert!(gaps[2] >= 2 * MIN, "gaps: {gaps:?}");
    }

    #[tokio::test]
    async fn handshake_alone_does_not_reset_backoff() {
        // A CSMS that completes the handshake and closes at once.
        let mut csms = FakeCsms::start().await;
        let _cp = spawn_run(&csms.url, settings(LONG));
        assert!(matches!(csms.recv_event().await, Event::Connected));

        let mut gaps = Vec::new();
        for _ in 0..2 {
            csms.close_connection();
            gaps.push(reconnect_gap(&mut csms).await);
        }
        assert!(gaps[0] >= MIN, "gaps: {gaps:?}");
        assert!(gaps[1] >= 2 * MIN, "back-off did not grow: {gaps:?}");
    }

    #[tokio::test]
    async fn successful_exchange_resets_backoff() {
        let mut csms = FakeCsms::start().await;
        let _cp = spawn_run(&csms.url, settings(LONG));

        // One failure first, so the next wait would be 2 · MIN without a reset.
        assert!(matches!(csms.recv_event().await, Event::Connected));
        csms.close_connection();
        reconnect_gap(&mut csms).await;

        let id = call_id(csms.recv_event().await, "BootNotification");
        csms.send(boot_result(&id, "Accepted", 1));
        // The first report proves that the boot response arrived. It is left
        // unanswered on purpose: an answer to a report resets the back-off
        // too, and this test is about the boot response.
        recv_report(&mut csms).await;
        csms.close_connection();

        let gap = reconnect_gap(&mut csms).await;
        assert!(gap < 2 * MIN, "back-off was not reset: {gap:?}");
    }

    // --- Status reports ---

    #[tokio::test]
    async fn accepted_reports_connector_0_then_1() {
        let mut csms = FakeCsms::start().await;
        let _cp = spawn_run(&csms.url, settings(LONG));
        accept_boot(&mut csms).await;

        // Connector 0 first, then connector 1, both with the fixed start time,
        // and both before the first Heartbeat.
        answer_reports(&mut csms).await;
        call_id(csms.recv_event_within(HEARTBEAT_WAIT).await, "Heartbeat");
    }

    #[tokio::test]
    async fn reconnect_without_boot_reports_both_again() {
        let mut csms = FakeCsms::start().await;
        let _cp = spawn_run(&csms.url, settings(LONG));
        connect_and_accept(&mut csms).await;

        csms.close_connection();
        reconnect_gap(&mut csms).await;

        // The same payloads, with the same time: a report that took the
        // current time instead of the time the status was reached would differ.
        answer_reports(&mut csms).await;
    }

    #[tokio::test]
    async fn pending_boot_sends_no_report() {
        let mut csms = FakeCsms::start().await;
        let _cp = spawn_run(&csms.url, settings(LONG));

        assert!(matches!(csms.recv_event().await, Event::Connected));
        let id = call_id(csms.recv_event().await, "BootNotification");
        csms.send(boot_result(&id, "Pending", 1));

        // Only BootNotification until the CSMS accepts (section 4.2).
        let id = call_id(
            csms.recv_event_within(HEARTBEAT_WAIT).await,
            "BootNotification",
        );
        csms.send(boot_result(&id, "Accepted", 1));
        answer_reports(&mut csms).await;
    }

    #[tokio::test]
    async fn successful_report_resets_backoff() {
        let mut csms = FakeCsms::start().await;
        let _cp = spawn_run(&csms.url, settings(LONG));
        connect_and_accept(&mut csms).await;

        // One failure, so the next wait would be 2 · MIN without a reset.
        csms.close_connection();
        reconnect_gap(&mut csms).await;
        // This reconnect skips the boot: the answers to the reports are the
        // first exchange on this connection.
        answer_reports(&mut csms).await;
        csms.close_connection();

        let gap = reconnect_gap(&mut csms).await;
        assert!(gap < 2 * MIN, "back-off was not reset: {gap:?}");
    }

    #[tokio::test]
    async fn report_call_error_keeps_connection() {
        let mut csms = FakeCsms::start().await;
        let _cp = spawn_run(&csms.url, settings(LONG));
        accept_boot(&mut csms).await;

        let (id, _) = recv_report(&mut csms).await;
        csms.send(call_error(&id));
        // The next event is the report of connector 1 on the same connection.
        let (id, payload) = recv_report(&mut csms).await;
        assert_eq!(payload, report_payload(1));
        csms.send(empty_result(&id));
        call_id(csms.recv_event_within(HEARTBEAT_WAIT).await, "Heartbeat");
    }

    #[tokio::test]
    async fn report_timeout_keeps_connection() {
        let mut csms = FakeCsms::start().await;
        let _cp = spawn_run(&csms.url, settings(SHORT));
        accept_boot(&mut csms).await;

        // Connector 0 is never answered: the call times out after SHORT.
        recv_report(&mut csms).await;
        let (id, payload) = recv_report(&mut csms).await;
        assert_eq!(payload, report_payload(1));
        csms.send(empty_result(&id));
        call_id(csms.recv_event_within(HEARTBEAT_WAIT).await, "Heartbeat");
    }

    #[tokio::test]
    async fn connection_lost_during_report_reports_again() {
        let mut csms = FakeCsms::start().await;
        let _cp = spawn_run(&csms.url, settings(LONG));
        accept_boot(&mut csms).await;

        recv_report(&mut csms).await; // connector 0, never answered
        csms.close_connection();
        reconnect_gap(&mut csms).await;

        // Nothing is lost: the new connection reports everything.
        answer_reports(&mut csms).await;
    }

    // --- Failed Heartbeat ---

    #[tokio::test]
    async fn heartbeat_call_error_keeps_connection() {
        let mut csms = FakeCsms::start().await;
        let _cp = spawn_run(&csms.url, settings(LONG));
        connect_and_accept(&mut csms).await;

        let id = call_id(csms.recv_event_within(HEARTBEAT_WAIT).await, "Heartbeat");
        csms.send(call_error(&id));

        // The next event is the next Heartbeat: no Closed, no Connected.
        call_id(csms.recv_event_within(HEARTBEAT_WAIT).await, "Heartbeat");
    }

    #[tokio::test]
    async fn heartbeat_timeout_keeps_connection() {
        let mut csms = FakeCsms::start().await;
        let _cp = spawn_run(&csms.url, settings(SHORT));
        connect_and_accept(&mut csms).await;

        // Never answered: the call times out after SHORT.
        call_id(csms.recv_event_within(HEARTBEAT_WAIT).await, "Heartbeat");

        let id = call_id(csms.recv_event_within(HEARTBEAT_WAIT).await, "Heartbeat");
        csms.send(heartbeat_result(&id));
    }

    // --- Shutdown ---

    #[tokio::test]
    async fn shutdown_closes_connection_and_returns() {
        let mut csms = FakeCsms::start().await;
        let (stop, signal) = shutdown_signal();
        let cp = spawn_run_until(&csms.url, settings(LONG), signal);
        connect_and_accept(&mut csms).await;
        let id = call_id(csms.recv_event_within(HEARTBEAT_WAIT).await, "Heartbeat");
        csms.send(heartbeat_result(&id));

        stop.send(()).unwrap();
        let took = time_to_stop(cp).await;
        assert!(took < Duration::from_secs(1), "took {took:?}");
        assert_closed_with(&mut csms, 1001).await;
    }

    #[tokio::test]
    async fn shutdown_during_boot_call_closes_connection() {
        let mut csms = FakeCsms::start().await;
        let (stop, signal) = shutdown_signal();
        let cp = spawn_run_until(&csms.url, settings(LONG), signal);
        assert!(matches!(csms.recv_event().await, Event::Connected));
        call_id(csms.recv_event().await, "BootNotification"); // never answered

        stop.send(()).unwrap();
        let took = time_to_stop(cp).await;
        assert!(
            took < Duration::from_secs(1),
            "waited for the call: {took:?}"
        );
        assert_closed_with(&mut csms, 1001).await;
    }

    #[tokio::test]
    async fn shutdown_during_heartbeat_call_closes_connection() {
        let mut csms = FakeCsms::start().await;
        let (stop, signal) = shutdown_signal();
        let cp = spawn_run_until(&csms.url, settings(LONG), signal);
        connect_and_accept(&mut csms).await;
        call_id(csms.recv_event_within(HEARTBEAT_WAIT).await, "Heartbeat"); // never answered

        stop.send(()).unwrap();
        let took = time_to_stop(cp).await;
        assert!(
            took < Duration::from_secs(1),
            "waited for the call: {took:?}"
        );
        assert_closed_with(&mut csms, 1001).await;
    }

    #[tokio::test]
    async fn shutdown_during_report_closes_connection() {
        let mut csms = FakeCsms::start().await;
        let (stop, signal) = shutdown_signal();
        let cp = spawn_run_until(&csms.url, settings(LONG), signal);
        accept_boot(&mut csms).await;
        recv_report(&mut csms).await; // never answered

        stop.send(()).unwrap();
        let took = time_to_stop(cp).await;
        assert!(
            took < Duration::from_secs(1),
            "waited for the call: {took:?}"
        );
        assert_closed_with(&mut csms, 1001).await;
    }

    #[tokio::test]
    async fn shutdown_during_backoff_returns_at_once() {
        // Accepts TCP and drops it at once, so every handshake fails.
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("ws://{}/CP001", listener.local_addr().unwrap());
        let (attempts_tx, mut attempts) = mpsc::unbounded_channel();
        tokio::spawn(async move {
            while let Ok((tcp, _)) = listener.accept().await {
                drop(tcp);
                let _ = attempts_tx.send(());
            }
        });
        let mut settings = settings(LONG);
        settings.backoff.wait_minimum = LONG;
        let (stop, signal) = shutdown_signal();
        let cp = spawn_run_until(&url, settings, signal);

        timeout(LONG, attempts.recv()).await.unwrap().unwrap();
        // Give the client time to see the dropped connection and start the
        // back-off wait, so the signal arrives there and not in `connect`.
        sleep(Duration::from_millis(200)).await;
        stop.send(()).unwrap();
        let took = time_to_stop(cp).await;
        assert!(
            took < Duration::from_secs(1),
            "waited for the back-off: {took:?}"
        );
        assert!(attempts.try_recv().is_err(), "connected again");
    }

    #[tokio::test]
    async fn shutdown_during_rejected_wait_returns_at_once() {
        let mut csms = FakeCsms::start().await;
        let (stop, signal) = shutdown_signal();
        let cp = spawn_run_until(&csms.url, settings(LONG), signal);
        assert!(matches!(csms.recv_event().await, Event::Connected));
        let id = call_id(csms.recv_event().await, "BootNotification");
        // Longer than the 1 s bound below, shorter than `time_to_stop` waits.
        csms.send(boot_result(&id, "Rejected", 5));
        // Closed, so the client is in the wait after Rejected now.
        assert_closed_with(&mut csms, 1000).await;

        stop.send(()).unwrap();
        let took = time_to_stop(cp).await;
        assert!(
            took < Duration::from_secs(1),
            "waited after Rejected: {took:?}"
        );
    }

    #[tokio::test]
    async fn shutdown_during_connect_returns_at_once() {
        // Accepts TCP but never answers the WebSocket handshake.
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("ws://{}/CP001", listener.local_addr().unwrap());
        let (accepted_tx, accepted) = oneshot::channel();
        tokio::spawn(async move {
            let (_tcp, _) = listener.accept().await.unwrap();
            let _ = accepted_tx.send(());
            std::future::pending::<()>().await;
        });
        let (stop, signal) = shutdown_signal();
        let cp = spawn_run_until(&url, settings(LONG), signal);

        timeout(LONG, accepted).await.unwrap().unwrap();
        stop.send(()).unwrap();
        let took = time_to_stop(cp).await;
        assert!(
            took < Duration::from_secs(1),
            "waited for the handshake: {took:?}"
        );
    }

    #[tokio::test]
    async fn shutdown_gives_up_after_close_timeout() {
        // Completes the handshake, reads the BootNotification and then never
        // reads again, so our Close is never answered.
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("ws://{}/CP001", listener.local_addr().unwrap());
        let (booted_tx, booted) = oneshot::channel();
        tokio::spawn(async move {
            let (tcp, _) = listener.accept().await.unwrap();
            let mut ws = accept_hdr_async(tcp, echo_ocpp16).await.unwrap();
            ws.next().await;
            let _ = booted_tx.send(());
            std::future::pending::<()>().await;
            drop(ws); // keeps the socket open until here
        });
        let mut settings = settings(LONG);
        settings.close_timeout = SHORT;
        let (stop, signal) = shutdown_signal();
        let cp = spawn_run_until(&url, settings, signal);

        timeout(LONG, booted).await.unwrap().unwrap();
        stop.send(()).unwrap();
        let took = time_to_stop(cp).await;
        assert!(took >= SHORT, "did not wait for the server: {took:?}");
        assert!(took < LONG, "ignored close_timeout: {took:?}");
    }

    #[tokio::test]
    async fn disconnect_after_reader_was_awaited_does_not_panic() {
        let mut csms = FakeCsms::start().await;
        let (client, mut reader) = Client::connect(&csms.url, settings(LONG).timeouts)
            .await
            .unwrap();
        assert!(matches!(csms.recv_event().await, Event::Connected));
        csms.close_connection();
        // What `boot` and `keep_alive` do: await the reader to the end
        // through `&mut`, so the handle is already consumed.
        timeout(Duration::from_secs(1), &mut reader)
            .await
            .expect("reader task did not stop")
            .expect("reader task panicked");

        // Awaiting it again would panic: "JoinHandle polled after completion".
        assert!(disconnect(&client, &mut reader, LONG, CloseCode::Away).await);
    }

    #[tokio::test]
    async fn disconnect_finishes_reader_and_fails_calls_on_old_client() {
        let mut csms = FakeCsms::start().await;
        let (client, mut reader) = Client::connect(&csms.url, settings(LONG).timeouts)
            .await
            .unwrap();
        let waiting = {
            let client = Arc::clone(&client);
            tokio::spawn(async move { client.call(&HeartbeatRequest {}).await })
        };
        csms.recv().await; // the Heartbeat is in flight, never answered

        assert!(
            disconnect(&client, &mut reader, LONG, CloseCode::Normal).await,
            "reader task did not finish on its own"
        );

        let err = timeout(Duration::from_secs(1), waiting)
            .await
            .expect("in-flight call hung after disconnect")
            .unwrap()
            .unwrap_err();
        assert!(matches!(err, CallFailure::Disconnected), "got {err:?}");
        let err = timeout(Duration::from_secs(1), client.call(&HeartbeatRequest {}))
            .await
            .expect("new call on the old client hung")
            .unwrap_err();
        assert!(matches!(err, CallFailure::Disconnected), "got {err:?}");
    }
}
