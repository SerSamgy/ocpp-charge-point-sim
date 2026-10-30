//! OCPP charge point simulator: connects to a CSMS over WebSocket, boots
//! and keeps the connection alive. Speaks OCPP 1.6 today.
//!
//! Usage: `cp-sim [ws://host:port/<charge_point_id>]`, default
//! `ws://localhost:9000/CP001`. The map of the modules and their
//! invariants is `docs/architecture.md`.

mod backoff;
mod boot;
mod client;
mod frame;
mod ocpp16;
mod pending;
mod session;
#[cfg(test)]
mod test_support;

use std::time::Duration;

use tokio_tungstenite::tungstenite::client::IntoClientRequest;

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
};

#[tokio::main]
async fn main() -> Result<(), client::BoxError> {
    let url = std::env::args()
        .nth(1)
        .unwrap_or_else(|| DEFAULT_CSMS_URL.to_string());

    // A malformed URL does not get better by retrying: fail at once.
    let handshake = url.as_str().into_client_request()?;
    if !matches!(handshake.uri().scheme_str(), Some("ws" | "wss")) {
        return Err(format!("not a WebSocket URL: {url}").into());
    }

    let request = BootNotificationRequest {
        charge_point_vendor: "Semyon".into(),
        charge_point_model: "cp-sim".into(),
        charge_point_serial_number: None,
        firmware_version: None,
    };
    // `run` never returns: its result type has no values, so this match
    // needs no arms and the compiler knows `main` cannot get past it.
    match session::run(&url, SETTINGS, &request).await {}
}
