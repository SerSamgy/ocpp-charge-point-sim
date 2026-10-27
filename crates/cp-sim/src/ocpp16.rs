//! OCPP 1.6 message payloads: the inner JSON object of CALL / CALLRESULT.

use serde::{Deserialize, Serialize, de::DeserializeOwned};

/// Ties a request payload to its action name and response type.
pub trait Request: Serialize {
    /// Action name as it appears in the CALL frame.
    const ACTION: &'static str;
    /// Payload type the server answers with.
    type Response: DeserializeOwned;
}

// --- BootNotification ---

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BootNotificationRequest {
    pub charge_point_vendor: String,
    pub charge_point_model: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub charge_point_serial_number: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub firmware_version: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BootNotificationResponse {
    pub status: RegistrationStatus,
    pub current_time: String,
    pub interval: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum RegistrationStatus {
    Accepted,
    Pending,
    Rejected,
}

impl Request for BootNotificationRequest {
    const ACTION: &'static str = "BootNotification";
    type Response = BootNotificationResponse;
}

// --- Heartbeat ---

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HeartbeatRequest {}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HeartbeatResponse {
    pub current_time: String,
}

impl Request for HeartbeatRequest {
    const ACTION: &'static str = "Heartbeat";
    type Response = HeartbeatResponse;
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn boot_request_omits_absent_optionals() {
        let req = BootNotificationRequest {
            charge_point_vendor: "Semyon".into(),
            charge_point_model: "cp-sim".into(),
            charge_point_serial_number: None,
            firmware_version: None,
        };
        assert_eq!(
            serde_json::to_value(&req).unwrap(),
            json!({"chargePointVendor": "Semyon", "chargePointModel": "cp-sim"})
        );
    }

    #[test]
    fn boot_response_parses_cpms_response() {
        let raw = r#"{"currentTime":"2026-10-09T18:10:28.302408+00:00","interval":10,"status":"Accepted"}"#;
        let resp = serde_json::from_str::<BootNotificationResponse>(raw).unwrap();
        assert_eq!(
            resp,
            BootNotificationResponse {
                status: RegistrationStatus::Accepted,
                current_time: "2026-10-09T18:10:28.302408+00:00".into(),
                interval: 10,
            }
        );
    }

    #[test]
    fn boot_response_with_wrong_status_fails() {
        let raw = r#"{"status":"Maybe","interval":10,"currentTime":"2026-10-12T18:00:00Z"}"#;
        let err = serde_json::from_str::<BootNotificationResponse>(raw).unwrap_err();
        assert!(err.is_data(), "unexpected error kind: {err}");
    }

    #[test]
    fn boot_response_without_interval_fails() {
        let raw = r#"{"status":"Accepted","currentTime":"2026-10-12T18:00:00Z"}"#;
        let err = serde_json::from_str::<BootNotificationResponse>(raw).unwrap_err();
        assert!(err.is_data(), "unexpected error kind: {err}");
    }

    #[test]
    fn heartbeat_request_serializes_to_exactly_empty_struct() {
        let req = HeartbeatRequest {};

        assert_eq!(serde_json::to_value(&req).unwrap(), json!({}));
    }
}
