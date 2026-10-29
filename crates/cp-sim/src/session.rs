//! Getting registered at the CSMS: connect, BootNotification, retry until Accepted.

use std::{sync::Arc, time::Duration};

use tokio::{
    task::JoinHandle,
    time::{sleep, timeout},
};

use crate::{
    boot::{self, BootStep},
    client::{BoxError, Client, Timeouts},
    ocpp16::BootNotificationRequest,
};

/// A connection on which the server has accepted our BootNotification.
pub struct Registered {
    pub client: Arc<Client>,
    /// Finishes when the connection closes.
    pub reader: JoinHandle<()>,
    /// Heartbeat interval from the server; never zero.
    pub heartbeat: Duration,
}

/// Connects and sends BootNotification until the server accepts it.
/// - Pending: wait on the same connection, then boot again; fails at once if
///   the connection drops meanwhile.
/// - Rejected: close the connection, wait, connect again and boot again.
///
/// Connection and CALL failures are returned as errors: reconnecting with
/// back-off is a later step.
pub async fn register(
    url: &str,
    timeouts: Timeouts,
    request: &BootNotificationRequest,
) -> Result<Registered, BoxError> {
    loop {
        let (client, mut reader) = Client::connect(url, timeouts).await?;
        println!("connected to {url}");
        match boot(&client, &mut reader, request).await? {
            Booted::Accepted { heartbeat } => {
                return Ok(Registered {
                    client,
                    reader,
                    heartbeat,
                });
            }
            Booted::Rejected { after } => {
                println!("boot rejected, reconnecting in {after:?}");
                disconnect(&client, reader, timeouts.call).await;
                sleep(after).await;
            }
        }
    }
}

/// How a boot on one connection ended.
enum Booted {
    /// Registered: send Heartbeat every `heartbeat`.
    Accepted { heartbeat: Duration },
    /// Rejected: close the connection and stay away for `after`.
    Rejected { after: Duration },
}

/// Sends BootNotification on an open connection until the server accepts or
/// rejects it. Pending: wait on the same connection, then boot again; fails at
/// once if the connection drops meanwhile.
async fn boot(
    client: &Client,
    reader: &mut JoinHandle<()>,
    request: &BootNotificationRequest,
) -> Result<Booted, BoxError> {
    loop {
        let response = client.call(request).await?;
        match boot::next_step(&response) {
            BootStep::Accepted { heartbeat } => return Ok(Booted::Accepted { heartbeat }),
            BootStep::RetryOnSameConnection { after } => {
                println!("boot pending, next attempt in {after:?}");
                tokio::select! {
                    _ = sleep(after) => {}
                    _ = &mut *reader => {
                        return Err("connection closed while the boot was pending".into());
                    }
                }
            }
            BootStep::Reconnect { after } => return Ok(Booted::Rejected { after }),
        }
    }
}

/// Closes the connection and waits up to `wait` for the reader task to finish;
/// aborts it if the server does not complete the closing handshake.
/// Returns true if the reader finished on its own.
async fn disconnect(client: &Client, mut reader: JoinHandle<()>, wait: Duration) -> bool {
    if let Err(err) = client.close().await {
        eprintln!("failed to send Close: {err}");
    }
    if timeout(wait, &mut reader).await.is_ok() {
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
        client::CallFailure,
        frame::Frame,
        ocpp16::HeartbeatRequest,
        test_support::{Event, FakeCsms},
    };
    use serde_json::json;
    use tokio::time::Instant;

    const LONG: Duration = Duration::from_secs(5);
    const TIMEOUTS: Timeouts = Timeouts {
        connect: LONG,
        call: LONG,
    };

    fn boot_request() -> BootNotificationRequest {
        BootNotificationRequest {
            charge_point_vendor: "Semyon".into(),
            charge_point_model: "cp-sim".into(),
            charge_point_serial_number: None,
            firmware_version: None,
        }
    }

    fn spawn_register(url: &str) -> JoinHandle<Result<Registered, BoxError>> {
        let url = url.to_owned();
        tokio::spawn(async move { register(&url, TIMEOUTS, &boot_request()).await })
    }

    /// Asserts that the event is a BootNotification CALL and returns its ID.
    fn boot_id(event: Event) -> String {
        match event {
            Event::Frame(Frame::Call { id, action, .. }) if action == "BootNotification" => id,
            other => panic!("expected a BootNotification CALL, got {other:?}"),
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

    #[tokio::test]
    async fn accepted_returns_heartbeat_interval() {
        let mut csms = FakeCsms::start().await;
        let registering = spawn_register(&csms.url);

        assert!(matches!(csms.recv_event().await, Event::Connected));
        let id = boot_id(csms.recv_event().await);
        csms.send(boot_result(&id, "Accepted", 7));

        let registered = registering.await.unwrap().unwrap();
        assert_eq!(registered.heartbeat, Duration::from_secs(7));
    }

    #[tokio::test]
    async fn pending_boots_again_on_same_connection_after_interval() {
        let mut csms = FakeCsms::start().await;
        let registering = spawn_register(&csms.url);

        assert!(matches!(csms.recv_event().await, Event::Connected));
        let id = boot_id(csms.recv_event().await);
        csms.send(boot_result(&id, "Pending", 1));
        let answered = Instant::now();

        // The very next event must be the second boot: no reconnect, no other CALL.
        let id = boot_id(csms.recv_event_within(Duration::from_secs(3)).await);
        assert!(
            answered.elapsed() >= Duration::from_secs(1),
            "booted again before the interval: {:?}",
            answered.elapsed()
        );
        csms.send(boot_result(&id, "Accepted", 7));

        let registered = registering.await.unwrap().unwrap();
        assert_eq!(registered.heartbeat, Duration::from_secs(7));
    }

    #[tokio::test]
    async fn pending_fails_at_once_when_connection_drops() {
        let mut csms = FakeCsms::start().await;
        let registering = spawn_register(&csms.url);

        assert!(matches!(csms.recv_event().await, Event::Connected));
        let id = boot_id(csms.recv_event().await);
        csms.send(boot_result(&id, "Pending", 30));
        csms.close_connection();

        let result = timeout(Duration::from_secs(2), registering)
            .await
            .expect("register waited for the whole Pending interval")
            .unwrap();
        assert!(result.is_err(), "register succeeded without a connection");
    }

    #[tokio::test]
    async fn rejected_reconnects_after_interval_and_boots_again() {
        let mut csms = FakeCsms::start().await;
        let registering = spawn_register(&csms.url);

        assert!(matches!(csms.recv_event().await, Event::Connected));
        let id = boot_id(csms.recv_event().await);
        csms.send(boot_result(&id, "Rejected", 1));
        let answered = Instant::now();

        // The client closes the connection instead of staying silent on it.
        assert!(matches!(csms.recv_event().await, Event::Closed));
        assert!(matches!(
            csms.recv_event_within(Duration::from_secs(3)).await,
            Event::Connected
        ));
        assert!(
            answered.elapsed() >= Duration::from_secs(1),
            "reconnected before the interval: {:?}",
            answered.elapsed()
        );
        let id = boot_id(csms.recv_event().await);
        csms.send(boot_result(&id, "Accepted", 7));

        let registered = registering.await.unwrap().unwrap();
        assert_eq!(registered.heartbeat, Duration::from_secs(7));
    }

    #[tokio::test]
    async fn disconnect_finishes_reader_and_fails_calls_on_old_client() {
        let mut csms = FakeCsms::start().await;
        let (client, reader) = Client::connect(&csms.url, TIMEOUTS).await.unwrap();
        let waiting = {
            let client = Arc::clone(&client);
            tokio::spawn(async move { client.call(&HeartbeatRequest {}).await })
        };
        csms.recv().await; // the Heartbeat is in flight, never answered

        assert!(
            disconnect(&client, reader, LONG).await,
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
