mod client;
mod frame;
mod ocpp16;
mod pending;

use std::time::Duration;

use crate::{
    client::Client,
    ocpp16::{BootNotificationRequest, HeartbeatRequest, RegistrationStatus},
};

const DEFAULT_CSMS_URL: &str = "ws://localhost:9000/CP001";
const CALL_TIMEOUT: Duration = Duration::from_secs(30);

#[tokio::main]
async fn main() -> Result<(), client::BoxError> {
    let url = std::env::args()
        .nth(1)
        .unwrap_or_else(|| DEFAULT_CSMS_URL.to_string());

    // --- 1. Connect; the reader task is started inside connect() ---
    let (client, mut reader) = Client::connect(&url, CALL_TIMEOUT).await?;
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
    if boot.status != RegistrationStatus::Accepted {
        return Err(format!("boot not accepted: {:?}", boot.status).into());
    }
    println!("accepted, heartbeat every {}s", boot.interval);

    // --- 3. Heartbeat until the connection closes ---
    let mut ticker = tokio::time::interval(Duration::from_secs(boot.interval));
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
