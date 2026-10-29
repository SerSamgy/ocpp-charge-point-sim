mod boot;
mod client;
mod frame;
mod ocpp16;
mod pending;
mod session;
#[cfg(test)]
mod test_support;

use std::time::Duration;

use crate::{
    client::Timeouts,
    ocpp16::{BootNotificationRequest, HeartbeatRequest},
    session::Registered,
};

const DEFAULT_CSMS_URL: &str = "ws://localhost:9000/CP001";
const TIMEOUTS: Timeouts = Timeouts {
    connect: Duration::from_secs(10),
    call: Duration::from_secs(30),
};

#[tokio::main]
async fn main() -> Result<(), client::BoxError> {
    let url = std::env::args()
        .nth(1)
        .unwrap_or_else(|| DEFAULT_CSMS_URL.to_string());

    // --- 1. Connect and boot until the server accepts us ---
    let Registered {
        client,
        mut reader,
        heartbeat,
    } = session::register(
        &url,
        TIMEOUTS,
        &BootNotificationRequest {
            charge_point_vendor: "Semyon".into(),
            charge_point_model: "cp-sim".into(),
            charge_point_serial_number: None,
            firmware_version: None,
        },
    )
    .await?;
    println!("accepted, heartbeat every {heartbeat:?}");

    // --- 2. Heartbeat until the connection closes ---
    // `heartbeat` is never zero: boot::next_step replaces 0 with a default.
    let mut ticker = tokio::time::interval(heartbeat);
    ticker.tick().await; // first tick completes immediately — skip it

    loop {
        tokio::select! {
            _ = ticker.tick() => {
                client.call(&HeartbeatRequest {}).await?;
            }
            // The reader task ends when the connection is closed.
            _ = &mut reader => break,
        }
    }
    Ok(())
}
