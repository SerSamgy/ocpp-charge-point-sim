//! OCPP 1.6 message payloads: the inner JSON object of CALL / CALLRESULT.

use serde::{Deserialize, Serialize, de::DeserializeOwned};
use time::UtcDateTime;

use crate::timestamp;

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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StatusNotificationResponse {}

/// Every status of the schema, although the connector state machine uses
/// only some of them so far.
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

// --- Authorization (IdTagInfo, section 7.30; AuthorizationStatus, 7.2) ---

/// The answer of the CSMS about an idTag, part of Authorize.conf,
/// StartTransaction.conf and StopTransaction.conf. The two optional fields
/// are parsed but not used yet (no Authorization Cache).
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IdTagInfo {
    pub expiry_date: Option<String>,
    pub parent_id_tag: Option<String>,
    pub status: AuthorizationStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum AuthorizationStatus {
    Accepted,
    Blocked,
    Expired,
    Invalid,
    ConcurrentTx,
}

// --- Authorize ---

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthorizeRequest {
    pub id_tag: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthorizeResponse {
    pub id_tag_info: IdTagInfo,
}

impl Request for AuthorizeRequest {
    const ACTION: &'static str = "Authorize";
    type Response = AuthorizeResponse;
}

// --- StartTransaction ---

/// `reservationId` is left out: the station has no reservations.
/// `meterStart` is in Wh; `timestamp` is the instant the transaction
/// started, written as in StatusNotification (see `timestamp`).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StartTransactionRequest {
    pub connector_id: u32,
    pub id_tag: String,
    pub meter_start: u64,
    #[serde(serialize_with = "timestamp::serialize")]
    pub timestamp: UtcDateTime,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartTransactionResponse {
    pub id_tag_info: IdTagInfo,
    pub transaction_id: i32,
}

impl Request for StartTransactionRequest {
    const ACTION: &'static str = "StartTransaction";
    type Response = StartTransactionResponse;
}

// --- StopTransaction ---

/// `transactionData` (meter values of the transaction) is left out until
/// MeterValues exist (roadmap step 5b). `idTag` is the identifier that
/// stopped the transaction; it may be absent when the station stops it by
/// itself (section 4.10). `reason` absent means `Local`.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StopTransactionRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id_tag: Option<String>,
    pub meter_stop: u64,
    #[serde(serialize_with = "timestamp::serialize")]
    pub timestamp: UtcDateTime,
    pub transaction_id: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<StopReason>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StopTransactionResponse {
    pub id_tag_info: Option<IdTagInfo>,
}

/// Every reason of the schema (section 7.42), although the station sends
/// only some of them so far.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "the reasons of the schema the station does not send yet"
    )
)]
pub enum StopReason {
    EmergencyStop,
    EVDisconnected,
    HardReset,
    Local,
    Other,
    PowerLoss,
    Reboot,
    Remote,
    SoftReset,
    UnlockCommand,
    DeAuthorized,
}

impl Request for StopTransactionRequest {
    const ACTION: &'static str = "StopTransaction";
    type Response = StopTransactionResponse;
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

    // --- Transactions ---

    const NOON: UtcDateTime = UtcDateTime::UNIX_EPOCH;

    #[test]
    fn authorize_request_carries_the_id_tag() {
        let req = AuthorizeRequest {
            id_tag: "DEMO-TAG".into(),
        };
        assert_eq!(
            serde_json::to_value(req).unwrap(),
            json!({"idTag": "DEMO-TAG"})
        );
    }

    #[test]
    fn authorize_response_parses_with_and_without_optionals() {
        let raw = r#"{"idTagInfo":{"status":"Accepted"}}"#;
        let resp = serde_json::from_str::<AuthorizeResponse>(raw).unwrap();
        assert_eq!(resp.id_tag_info.status, AuthorizationStatus::Accepted);
        assert_eq!(resp.id_tag_info.parent_id_tag, None);

        let raw = r#"{"idTagInfo":{"status":"Blocked","expiryDate":"2026-12-01T00:00:00Z","parentIdTag":"FLEET"}}"#;
        let info = serde_json::from_str::<AuthorizeResponse>(raw)
            .unwrap()
            .id_tag_info;
        assert_eq!(info.status, AuthorizationStatus::Blocked);
        assert_eq!(info.expiry_date.as_deref(), Some("2026-12-01T00:00:00Z"));
        assert_eq!(info.parent_id_tag.as_deref(), Some("FLEET"));
    }

    #[test]
    fn authorization_status_matches_the_schema_strings() {
        let schema = [
            (AuthorizationStatus::Accepted, "Accepted"),
            (AuthorizationStatus::Blocked, "Blocked"),
            (AuthorizationStatus::Expired, "Expired"),
            (AuthorizationStatus::Invalid, "Invalid"),
            (AuthorizationStatus::ConcurrentTx, "ConcurrentTx"),
        ];
        for (status, text) in schema {
            assert_eq!(
                serde_json::from_value::<AuthorizationStatus>(json!(text)).unwrap(),
                status
            );
        }
        let err = serde_json::from_value::<AuthorizationStatus>(json!("Maybe")).unwrap_err();
        assert!(err.is_data(), "unexpected error kind: {err}");
    }

    #[test]
    fn start_transaction_request_has_every_required_field() {
        let req = StartTransactionRequest {
            connector_id: 1,
            id_tag: "DEMO-TAG".into(),
            meter_start: 246,
            timestamp: NOON,
        };
        assert_eq!(
            serde_json::to_value(req).unwrap(),
            json!({
                "connectorId": 1,
                "idTag": "DEMO-TAG",
                "meterStart": 246,
                "timestamp": "1970-01-01T00:00:00Z",
            })
        );
    }

    #[test]
    fn start_transaction_response_needs_a_transaction_id() {
        let raw = r#"{"idTagInfo":{"status":"Accepted"},"transactionId":7}"#;
        let resp = serde_json::from_str::<StartTransactionResponse>(raw).unwrap();
        assert_eq!(resp.transaction_id, 7);
        assert_eq!(resp.id_tag_info.status, AuthorizationStatus::Accepted);

        let raw = r#"{"idTagInfo":{"status":"Accepted"}}"#;
        let err = serde_json::from_str::<StartTransactionResponse>(raw).unwrap_err();
        assert!(err.is_data(), "unexpected error kind: {err}");
    }

    #[test]
    fn stop_transaction_request_omits_absent_optionals() {
        let req = StopTransactionRequest {
            id_tag: None,
            meter_stop: 492,
            timestamp: NOON,
            transaction_id: 7,
            reason: None,
        };
        assert_eq!(
            serde_json::to_value(req).unwrap(),
            json!({"meterStop": 492, "timestamp": "1970-01-01T00:00:00Z", "transactionId": 7})
        );
    }

    #[test]
    fn stop_transaction_request_includes_present_optionals() {
        let req = StopTransactionRequest {
            id_tag: Some("DEMO-TAG".into()),
            meter_stop: 492,
            timestamp: NOON,
            transaction_id: 7,
            reason: Some(StopReason::EVDisconnected),
        };
        assert_eq!(
            serde_json::to_value(req).unwrap(),
            json!({
                "idTag": "DEMO-TAG",
                "meterStop": 492,
                "timestamp": "1970-01-01T00:00:00Z",
                "transactionId": 7,
                "reason": "EVDisconnected",
            })
        );
    }

    #[test]
    fn stop_transaction_response_may_be_empty() {
        let resp = serde_json::from_str::<StopTransactionResponse>("{}").unwrap();
        assert_eq!(resp.id_tag_info, None);
        let raw = r#"{"idTagInfo":{"status":"Accepted"}}"#;
        let resp = serde_json::from_str::<StopTransactionResponse>(raw).unwrap();
        assert_eq!(
            resp.id_tag_info.map(|info| info.status),
            Some(AuthorizationStatus::Accepted)
        );
    }

    #[test]
    fn stop_reason_matches_the_schema_strings() {
        // The `reason` enum of StopTransaction.json, in schema order.
        let schema = [
            (StopReason::EmergencyStop, "EmergencyStop"),
            (StopReason::EVDisconnected, "EVDisconnected"),
            (StopReason::HardReset, "HardReset"),
            (StopReason::Local, "Local"),
            (StopReason::Other, "Other"),
            (StopReason::PowerLoss, "PowerLoss"),
            (StopReason::Reboot, "Reboot"),
            (StopReason::Remote, "Remote"),
            (StopReason::SoftReset, "SoftReset"),
            (StopReason::UnlockCommand, "UnlockCommand"),
            (StopReason::DeAuthorized, "DeAuthorized"),
        ];
        for (reason, text) in schema {
            assert_eq!(serde_json::to_value(reason).unwrap(), json!(text));
        }
    }
}
