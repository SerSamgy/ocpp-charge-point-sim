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
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "the session starts using the meter with transactions"
    )
)]
mod meter;
mod ocpp16;
mod pending;
mod session;
mod status16;
#[cfg(test)]
mod test_support;
mod timestamp;

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
