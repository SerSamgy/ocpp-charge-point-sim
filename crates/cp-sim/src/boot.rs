//! What to do after a BootNotification response (OCPP 1.6, section 4.2).

use std::time::Duration;

use crate::ocpp16::{BootNotificationResponse, RegistrationStatus};

/// Heartbeat interval when the server accepts us with `interval = 0`.
/// OCPP 1.6 does not define this case; 0 must not reach `tokio::time::interval`,
/// which panics on a zero period.
pub const DEFAULT_HEARTBEAT_INTERVAL: Duration = Duration::from_secs(300);

/// Wait before the next BootNotification when the server says Pending or
/// Rejected with `interval = 0`: the spec lets the charge point choose,
/// "in a way that avoids flooding the Central System".
pub const DEFAULT_BOOT_RETRY: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BootStep {
    /// Registered: send Heartbeat every `heartbeat`.
    Accepted { heartbeat: Duration },
    /// Pending: keep the connection (the server may send requests meanwhile)
    /// and send the next BootNotification after `after`.
    RetryOnSameConnection { after: Duration },
    /// Rejected: send nothing, not even answers, for `after`. We close the
    /// connection for that time and boot again on a new one.
    Reconnect { after: Duration },
}

/// Maps a BootNotification response to the next step, replacing a zero
/// interval with our own default.
pub fn next_step(resp: &BootNotificationResponse) -> BootStep {
    let interval = Duration::from_secs(resp.interval);
    let or_default = |default: Duration| {
        if interval.is_zero() {
            eprintln!(
                "{:?} with interval 0, using {default:?} instead",
                resp.status
            );
            default
        } else {
            interval
        }
    };
    match resp.status {
        RegistrationStatus::Accepted => BootStep::Accepted {
            heartbeat: or_default(DEFAULT_HEARTBEAT_INTERVAL),
        },
        RegistrationStatus::Pending => BootStep::RetryOnSameConnection {
            after: or_default(DEFAULT_BOOT_RETRY),
        },
        RegistrationStatus::Rejected => BootStep::Reconnect {
            after: or_default(DEFAULT_BOOT_RETRY),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn response(status: RegistrationStatus, interval: u64) -> BootNotificationResponse {
        BootNotificationResponse {
            status,
            current_time: "2026-10-29T12:00:00Z".into(),
            interval,
        }
    }

    const TEN: Duration = Duration::from_secs(10);

    #[test]
    fn accepted_uses_interval_as_heartbeat() {
        assert_eq!(
            next_step(&response(RegistrationStatus::Accepted, 10)),
            BootStep::Accepted { heartbeat: TEN }
        );
    }

    #[test]
    fn accepted_with_zero_interval_uses_default_heartbeat() {
        assert_eq!(
            next_step(&response(RegistrationStatus::Accepted, 0)),
            BootStep::Accepted {
                heartbeat: DEFAULT_HEARTBEAT_INTERVAL
            }
        );
    }

    #[test]
    fn pending_retries_on_same_connection_after_interval() {
        assert_eq!(
            next_step(&response(RegistrationStatus::Pending, 10)),
            BootStep::RetryOnSameConnection { after: TEN }
        );
    }

    #[test]
    fn pending_with_zero_interval_uses_default_retry() {
        assert_eq!(
            next_step(&response(RegistrationStatus::Pending, 0)),
            BootStep::RetryOnSameConnection {
                after: DEFAULT_BOOT_RETRY
            }
        );
    }

    #[test]
    fn rejected_reconnects_after_interval() {
        assert_eq!(
            next_step(&response(RegistrationStatus::Rejected, 10)),
            BootStep::Reconnect { after: TEN }
        );
    }

    #[test]
    fn rejected_with_zero_interval_uses_default_retry() {
        assert_eq!(
            next_step(&response(RegistrationStatus::Rejected, 0)),
            BootStep::Reconnect {
                after: DEFAULT_BOOT_RETRY
            }
        );
    }
}
