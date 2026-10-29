mod boot;
mod client;
mod frame;
mod ocpp16;
mod pending;
#[cfg(test)]
mod test_support;

use std::time::Duration;

use crate::{
    boot::BootStep,
    client::{Client, Timeouts},
    ocpp16::{BootNotificationRequest, HeartbeatRequest},
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

    // --- 1. Connect; the reader task is started inside connect() ---
    let (client, mut reader) = Client::connect(&url, TIMEOUTS).await?;
    println!("connected to {url}");

    // --- 2. BootNotification ---
    let boot = client
        .call(&BootNotificationRequest {
            charge_point_vendor: "Semyon".into(),
            charge_point_model: "cp-sim".into(),
            charge_point_serial_number: None,
            firmware_version: None,
        })
        .await?;
    let heartbeat = match boot::next_step(&boot) {
        BootStep::Accepted { heartbeat } => heartbeat,
        BootStep::RetryOnSameConnection { after } | BootStep::Reconnect { after } => {
            return Err(format!("boot not accepted: {:?}, retry in {after:?}", boot.status).into());
        }
    };
    println!("accepted, heartbeat every {heartbeat:?}");

    // --- 3. Heartbeat until the connection closes ---
    // `heartbeat` is never zero: next_step replaces 0 with a default.
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
