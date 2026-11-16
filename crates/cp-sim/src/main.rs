//! OCPP charge point simulator: connects to a CSMS over WebSocket, boots
//! and keeps the connection alive. Speaks OCPP 1.6 today. Ctrl+C or
//! SIGTERM closes the connection and exits.
//!
//! Usage: `cp-sim [ws://host:port/<charge_point_id>]`, default
//! `ws://localhost:9000/CP001`. The map of the modules and their
//! invariants is `docs/architecture.md`.

// Output goes through `tracing`; see `logging`.
#![deny(clippy::print_stdout, clippy::print_stderr)]

mod backoff;
mod boot;
mod client;
mod connector;
mod frame;
mod keyboard;
mod logging;
mod meter;
mod ocpp16;
mod pending;
mod sample_grid;
mod session;
mod status16;
#[cfg(test)]
mod test_support;
mod timestamp;
mod transaction16;
mod tx_queue;

use std::{io, time::Duration};

use time::UtcDateTime;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tracing::{info, warn};

use crate::{
    backoff::BackoffConfig, client::Timeouts, ocpp16::BootNotificationRequest, session::Settings,
};

const DEFAULT_CSMS_URL: &str = "ws://localhost:9000/CP001";
const SETTINGS: Settings = Settings {
    timeouts: Timeouts {
        connect: Duration::from_secs(10),
        call: Duration::from_secs(30),
    },
    // OCPP-J 2.0.1 gives no defaults: up to 3 · 2^5 = 96 s, plus up to 2 s.
    backoff: BackoffConfig {
        wait_minimum: Duration::from_secs(3),
        random_range: Duration::from_secs(2),
        repeat_times: 5,
    },
    boot_retry: boot::DEFAULT_BOOT_RETRY,
    close_timeout: Duration::from_secs(5),
    // MeterValueSampleInterval. Real stations use 1 to 5 minutes (1.6
    // section 3.16.1); 10 s shows the progress quickly when trying it out.
    meter_sample_interval: Duration::from_secs(10),
    // TransactionMessageAttempts as in the example of 1.6 section 3.7.1;
    // TransactionMessageRetryInterval 10 s instead of its 60 s, so retries
    // show quickly when trying them out (wait 10 s, then 20 s).
    transaction_attempts: 3,
    transaction_retry_interval: Duration::from_secs(10),
    // 4 hours of samples at 10 s (section 3.7: queue what an offline station
    // would have sent), about 0.15 MB. Past it the oldest are dropped. The
    // limit also bounds how long a StopTransaction waits behind the samples
    // of a CSMS that keeps failing them: about 30 s per sample for CALLERRORs
    // (3 attempts, waits of 10 s and 20 s), about 120 s when it does not
    // answer at all (3 timeouts of 30 s as well), so up to about 12 h or
    // 48 h. The roadmap backlog has a rule that would cut this.
    sample_queue_limit: 1440,
};

#[tokio::main]
async fn main() -> Result<(), client::BoxError> {
    logging::init()?;
    let url = std::env::args()
        .nth(1)
        .unwrap_or_else(|| DEFAULT_CSMS_URL.to_string());

    // A malformed URL does not get better by retrying: fail at once.
    let handshake = url.as_str().into_client_request()?;
    if !matches!(handshake.uri().scheme_str(), Some("ws" | "wss")) {
        return Err(format!("not a WebSocket URL: {url}").into());
    }

    let shutdown = shutdown_signal()?;
    let request = BootNotificationRequest {
        charge_point_vendor: "Semyon".into(),
        charge_point_model: "cp-sim".into(),
        charge_point_serial_number: None,
        firmware_version: None,
    };
    // Room for a few commands typed while the station is offline.
    let (inputs_tx, inputs) = tokio::sync::mpsc::channel(32);
    keyboard::spawn(inputs_tx)?;
    session::run(
        &url,
        SETTINGS,
        &request,
        UtcDateTime::now(),
        inputs,
        shutdown,
    )
    .await;
    Ok(())
}

/// Completes on Ctrl+C (SIGINT) or SIGTERM, which is how systemd stops a
/// service. The SIGTERM handler is installed here, before the program
/// connects; failing to install it is a startup error.
#[cfg(unix)]
fn shutdown_signal() -> io::Result<impl Future<Output = ()>> {
    use tokio::signal::unix::{SignalKind, signal};

    let mut terminate = signal(SignalKind::terminate())?;
    Ok(async move {
        tokio::select! {
            result = tokio::signal::ctrl_c() => {
                if let Err(err) = result {
                    warn!(%err, "cannot listen for Ctrl+C; only SIGTERM stops the program");
                    terminate.recv().await;
                }
            }
            _ = terminate.recv() => {}
        }
        info!("shutting down");
    })
}

/// Completes on Ctrl+C; other platforms have no SIGTERM.
#[cfg(not(unix))]
fn shutdown_signal() -> io::Result<impl Future<Output = ()>> {
    Ok(async {
        if let Err(err) = tokio::signal::ctrl_c().await {
            warn!(%err, "cannot listen for Ctrl+C; the program runs until killed");
            std::future::pending::<()>().await;
        }
        info!("shutting down");
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_sample_queue_holds_hours_of_samples() {
        // Section 3.7: an offline station queues what it would have sent.
        // 30 samples were 5 minutes, which lost the samples of any ordinary
        // outage; the queue must hold at least 4 hours.
        let held =
            SETTINGS.meter_sample_interval * u32::try_from(SETTINGS.sample_queue_limit).unwrap();
        assert!(held >= Duration::from_secs(4 * 3600), "only {held:?}");
    }
}
