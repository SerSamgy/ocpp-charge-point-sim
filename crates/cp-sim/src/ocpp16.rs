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

// --- StatusNotification ---

/// Only `connector_id`, `error_code` and `status` are required by the
/// schema. `timestamp` is the time the status was reached, in UTC and whole
/// seconds (see `timestamp`); when it is absent the CSMS assumes the time of
/// receipt.
#[cfg_attr(not(test), expect(dead_code, reason = "wired into session in step 4b"))]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StatusNotificationRequest {
    pub connector_id: u32,
    pub error_code: ChargePointErrorCode,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub info: Option<String>,
    pub status: ChargePointStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vendor_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vendor_error_code: Option<String>,
}

#[cfg_attr(not(test), expect(dead_code, reason = "wired into session in step 4b"))]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StatusNotificationResponse {}

/// Every status of the schema, although the connector state machine uses
/// only some of them so far.
#[cfg_attr(not(test), expect(dead_code, reason = "wired into session in step 4b"))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChargePointStatus {
    Available,
    Preparing,
    Charging,
    SuspendedEV,
    SuspendedEVSE,
    Finishing,
    Reserved,
    Unavailable,
    Faulted,
}

/// Every error code of the schema.
#[cfg_attr(not(test), expect(dead_code, reason = "wired into session in step 4b"))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChargePointErrorCode {
    ConnectorLockFailure,
    EVCommunicationError,
    GroundFailure,
    HighTemperature,
    InternalError,
    LocalListConflict,
    NoError,
    OtherError,
    OverCurrentFailure,
    PowerMeterFailure,
    PowerSwitchFailure,
    ReaderFailure,
    ResetFailure,
    UnderVoltage,
    OverVoltage,
    WeakSignal,
}

impl Request for StatusNotificationRequest {
    const ACTION: &'static str = "StatusNotification";
    type Response = StatusNotificationResponse;
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

    fn status_notification() -> StatusNotificationRequest {
        StatusNotificationRequest {
            connector_id: 1,
            error_code: ChargePointErrorCode::NoError,
            info: None,
            status: ChargePointStatus::Preparing,
            timestamp: None,
            vendor_id: None,
            vendor_error_code: None,
        }
    }

    #[test]
    fn status_notification_request_omits_absent_optionals() {
        assert_eq!(
            serde_json::to_value(status_notification()).unwrap(),
            json!({"connectorId": 1, "errorCode": "NoError", "status": "Preparing"})
        );
    }

    #[test]
    fn status_notification_request_includes_present_optionals() {
        let req = StatusNotificationRequest {
            info: Some("cable stuck".into()),
            timestamp: Some("2026-11-05T12:00:00Z".into()),
            vendor_id: Some("Semyon".into()),
            vendor_error_code: Some("E42".into()),
            ..status_notification()
        };
        assert_eq!(
            serde_json::to_value(req).unwrap(),
            json!({
                "connectorId": 1,
                "errorCode": "NoError",
                "info": "cable stuck",
                "status": "Preparing",
                "timestamp": "2026-11-05T12:00:00Z",
                "vendorId": "Semyon",
                "vendorErrorCode": "E42",
            })
        );
    }

    #[test]
    fn status_notification_response_parses_empty_object() {
        let resp = serde_json::from_str::<StatusNotificationResponse>("{}").unwrap();
        assert_eq!(resp, StatusNotificationResponse {});
    }

    #[test]
    fn charge_point_status_matches_the_schema_strings() {
        // The `status` enum of StatusNotification.json, in schema order.
        let schema = [
            (ChargePointStatus::Available, "Available"),
            (ChargePointStatus::Preparing, "Preparing"),
            (ChargePointStatus::Charging, "Charging"),
            (ChargePointStatus::SuspendedEV, "SuspendedEV"),
            (ChargePointStatus::SuspendedEVSE, "SuspendedEVSE"),
            (ChargePointStatus::Finishing, "Finishing"),
            (ChargePointStatus::Reserved, "Reserved"),
            (ChargePointStatus::Unavailable, "Unavailable"),
            (ChargePointStatus::Faulted, "Faulted"),
        ];
        for (status, text) in schema {
            assert_eq!(serde_json::to_value(status).unwrap(), json!(text));
            assert_eq!(
                serde_json::from_value::<ChargePointStatus>(json!(text)).unwrap(),
                status
            );
        }
    }

    #[test]
    fn charge_point_error_code_matches_the_schema_strings() {
        // The `errorCode` enum of StatusNotification.json, in schema order.
        let schema = [
            (
                ChargePointErrorCode::ConnectorLockFailure,
                "ConnectorLockFailure",
            ),
            (
                ChargePointErrorCode::EVCommunicationError,
                "EVCommunicationError",
            ),
            (ChargePointErrorCode::GroundFailure, "GroundFailure"),
            (ChargePointErrorCode::HighTemperature, "HighTemperature"),
            (ChargePointErrorCode::InternalError, "InternalError"),
            (ChargePointErrorCode::LocalListConflict, "LocalListConflict"),
            (ChargePointErrorCode::NoError, "NoError"),
            (ChargePointErrorCode::OtherError, "OtherError"),
            (
                ChargePointErrorCode::OverCurrentFailure,
                "OverCurrentFailure",
            ),
            (ChargePointErrorCode::PowerMeterFailure, "PowerMeterFailure"),
            (
                ChargePointErrorCode::PowerSwitchFailure,
                "PowerSwitchFailure",
            ),
            (ChargePointErrorCode::ReaderFailure, "ReaderFailure"),
            (ChargePointErrorCode::ResetFailure, "ResetFailure"),
            (ChargePointErrorCode::UnderVoltage, "UnderVoltage"),
            (ChargePointErrorCode::OverVoltage, "OverVoltage"),
            (ChargePointErrorCode::WeakSignal, "WeakSignal"),
        ];
        for (code, text) in schema {
            assert_eq!(serde_json::to_value(code).unwrap(), json!(text));
            assert_eq!(
                serde_json::from_value::<ChargePointErrorCode>(json!(text)).unwrap(),
                code
            );
        }
    }

    #[test]
    fn unknown_charge_point_status_fails() {
        let err = serde_json::from_value::<ChargePointStatus>(json!("Occupied")).unwrap_err();
        assert!(err.is_data(), "unexpected error kind: {err}");
    }
}
