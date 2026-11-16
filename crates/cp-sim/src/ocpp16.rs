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

/// `transactionData` (meter values of the transaction) is left out: the
/// station behaves as if `StopTxnSampledData` and `StopTxnAlignedData` were
/// empty, and then it SHALL NOT put meter values here (section 3.16.5;
/// errata 3.14 recommends Transaction.Begin and .End values only when they
/// are configured). `idTag` is the identifier that
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

// --- MeterValues (section 4.7; SampledValue and its enums, section 7) ---

/// Meter values of `connector_id`. `transaction_id` is the transaction going
/// on at that connector when the sample was taken (errata 3.16).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MeterValuesRequest {
    pub connector_id: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transaction_id: Option<i32>,
    pub meter_value: Vec<MeterValue>,
}

/// Values sampled at the same instant.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MeterValue {
    #[serde(serialize_with = "timestamp::serialize")]
    pub timestamp: UtcDateTime,
    pub sampled_value: Vec<SampledValue>,
}

/// One value. Every field but `value` is optional; their defaults (section
/// 4.7) make a bare value a register reading of active import energy in
/// Wh, sampled periodically. `format`, `phase` and `location` are part of
/// the schema but not filled by the station yet: it has one meter, on the
/// outlet, without phases.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SampledValue {
    /// A decimal number, or signed data, as a string (section 7.43).
    pub value: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<ReadingContext>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<ValueFormat>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub measurand: Option<Measurand>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub phase: Option<Phase>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub location: Option<Location>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unit: Option<UnitOfMeasure>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct MeterValuesResponse {}

impl Request for MeterValuesRequest {
    const ACTION: &'static str = "MeterValues";
    type Response = MeterValuesResponse;
}

/// Why a value was taken; every value of the schema.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "values of the schema the station does not send yet"
    )
)]
pub enum ReadingContext {
    #[serde(rename = "Interruption.Begin")]
    InterruptionBegin,
    #[serde(rename = "Interruption.End")]
    InterruptionEnd,
    #[serde(rename = "Sample.Clock")]
    SampleClock,
    #[serde(rename = "Sample.Periodic")]
    SamplePeriodic,
    #[serde(rename = "Transaction.Begin")]
    TransactionBegin,
    #[serde(rename = "Transaction.End")]
    TransactionEnd,
    Trigger,
    Other,
}

/// Raw number or signed data; every value of the schema.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "values of the schema the station does not send yet"
    )
)]
pub enum ValueFormat {
    Raw,
    SignedData,
}

/// What a value measures; every value of the schema.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "values of the schema the station does not send yet"
    )
)]
pub enum Measurand {
    #[serde(rename = "Energy.Active.Export.Register")]
    EnergyActiveExportRegister,
    #[serde(rename = "Energy.Active.Import.Register")]
    EnergyActiveImportRegister,
    #[serde(rename = "Energy.Reactive.Export.Register")]
    EnergyReactiveExportRegister,
    #[serde(rename = "Energy.Reactive.Import.Register")]
    EnergyReactiveImportRegister,
    #[serde(rename = "Energy.Active.Export.Interval")]
    EnergyActiveExportInterval,
    #[serde(rename = "Energy.Active.Import.Interval")]
    EnergyActiveImportInterval,
    #[serde(rename = "Energy.Reactive.Export.Interval")]
    EnergyReactiveExportInterval,
    #[serde(rename = "Energy.Reactive.Import.Interval")]
    EnergyReactiveImportInterval,
    #[serde(rename = "Power.Active.Export")]
    PowerActiveExport,
    #[serde(rename = "Power.Active.Import")]
    PowerActiveImport,
    #[serde(rename = "Power.Offered")]
    PowerOffered,
    #[serde(rename = "Power.Reactive.Export")]
    PowerReactiveExport,
    #[serde(rename = "Power.Reactive.Import")]
    PowerReactiveImport,
    #[serde(rename = "Power.Factor")]
    PowerFactor,
    #[serde(rename = "Current.Import")]
    CurrentImport,
    #[serde(rename = "Current.Export")]
    CurrentExport,
    #[serde(rename = "Current.Offered")]
    CurrentOffered,
    Voltage,
    Frequency,
    Temperature,
    SoC,
    #[serde(rename = "RPM")]
    Rpm,
}

/// The phase a value belongs to; every value of the schema.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "values of the schema the station does not send yet"
    )
)]
pub enum Phase {
    L1,
    L2,
    L3,
    N,
    #[serde(rename = "L1-N")]
    L1N,
    #[serde(rename = "L2-N")]
    L2N,
    #[serde(rename = "L3-N")]
    L3N,
    #[serde(rename = "L1-L2")]
    L1L2,
    #[serde(rename = "L2-L3")]
    L2L3,
    #[serde(rename = "L3-L1")]
    L3L1,
}

/// Where a value is measured; every value of the schema.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "values of the schema the station does not send yet"
    )
)]
pub enum Location {
    Cable,
    EV,
    Inlet,
    Outlet,
    Body,
}

/// The unit of a value; every value of the schema. Both spellings of
/// Celsius are there: the 1.6 schemas had only the misspelt `Celcius`, and
/// OCPP-J 1.6 errata 2.1 (2025-04) adds `Celsius` but advises a charge
/// point to keep sending `Celcius` for interoperability.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "values of the schema the station does not send yet"
    )
)]
pub enum UnitOfMeasure {
    Wh,
    #[serde(rename = "kWh")]
    KWh,
    #[serde(rename = "varh")]
    Varh,
    #[serde(rename = "kvarh")]
    Kvarh,
    W,
    #[serde(rename = "kW")]
    KW,
    VA,
    #[serde(rename = "kVA")]
    Kva,
    #[serde(rename = "var")]
    Var,
    #[serde(rename = "kvar")]
    Kvar,
    A,
    V,
    K,
    Celcius,
    Celsius,
    Fahrenheit,
    Percent,
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

    // --- MeterValues ---

    #[test]
    fn meter_values_request_omits_absent_optionals() {
        let req = MeterValuesRequest {
            connector_id: 1,
            transaction_id: None,
            meter_value: vec![MeterValue {
                timestamp: NOON,
                sampled_value: vec![SampledValue {
                    value: "246".into(),
                    context: None,
                    format: None,
                    measurand: None,
                    phase: None,
                    location: None,
                    unit: None,
                }],
            }],
        };
        assert_eq!(
            serde_json::to_value(req).unwrap(),
            json!({
                "connectorId": 1,
                "meterValue": [{
                    "timestamp": "1970-01-01T00:00:00Z",
                    "sampledValue": [{"value": "246"}],
                }],
            })
        );
    }

    #[test]
    fn sampled_value_includes_present_optionals() {
        let value = SampledValue {
            value: "230.5".into(),
            context: Some(ReadingContext::SamplePeriodic),
            format: Some(ValueFormat::Raw),
            measurand: Some(Measurand::Voltage),
            phase: Some(Phase::L1N),
            location: Some(Location::Outlet),
            unit: Some(UnitOfMeasure::V),
        };
        assert_eq!(
            serde_json::to_value(value).unwrap(),
            json!({
                "value": "230.5",
                "context": "Sample.Periodic",
                "format": "Raw",
                "measurand": "Voltage",
                "phase": "L1-N",
                "location": "Outlet",
                "unit": "V",
            })
        );
    }

    #[test]
    fn meter_values_response_parses_empty_object() {
        let resp = serde_json::from_str::<MeterValuesResponse>("{}").unwrap();
        assert_eq!(resp, MeterValuesResponse {});
    }

    /// Asserts that `values` serialize to `texts`, in order: the enum of the
    /// schema, in schema order.
    fn assert_schema_strings<T: Serialize>(values: &[T], texts: &[&str]) {
        assert_eq!(values.len(), texts.len(), "not every value of the schema");
        for (value, text) in values.iter().zip(texts) {
            assert_eq!(serde_json::to_value(value).unwrap(), json!(text));
        }
    }

    #[test]
    fn sampled_value_enums_match_the_schema_strings() {
        assert_schema_strings(
            &[
                ReadingContext::InterruptionBegin,
                ReadingContext::InterruptionEnd,
                ReadingContext::SampleClock,
                ReadingContext::SamplePeriodic,
                ReadingContext::TransactionBegin,
                ReadingContext::TransactionEnd,
                ReadingContext::Trigger,
                ReadingContext::Other,
            ],
            &[
                "Interruption.Begin",
                "Interruption.End",
                "Sample.Clock",
                "Sample.Periodic",
                "Transaction.Begin",
                "Transaction.End",
                "Trigger",
                "Other",
            ],
        );
        assert_schema_strings(
            &[ValueFormat::Raw, ValueFormat::SignedData],
            &["Raw", "SignedData"],
        );
        assert_schema_strings(
            &[
                Measurand::EnergyActiveExportRegister,
                Measurand::EnergyActiveImportRegister,
                Measurand::EnergyReactiveExportRegister,
                Measurand::EnergyReactiveImportRegister,
                Measurand::EnergyActiveExportInterval,
                Measurand::EnergyActiveImportInterval,
                Measurand::EnergyReactiveExportInterval,
                Measurand::EnergyReactiveImportInterval,
                Measurand::PowerActiveExport,
                Measurand::PowerActiveImport,
                Measurand::PowerOffered,
                Measurand::PowerReactiveExport,
                Measurand::PowerReactiveImport,
                Measurand::PowerFactor,
                Measurand::CurrentImport,
                Measurand::CurrentExport,
                Measurand::CurrentOffered,
                Measurand::Voltage,
                Measurand::Frequency,
                Measurand::Temperature,
                Measurand::SoC,
                Measurand::Rpm,
            ],
            &[
                "Energy.Active.Export.Register",
                "Energy.Active.Import.Register",
                "Energy.Reactive.Export.Register",
                "Energy.Reactive.Import.Register",
                "Energy.Active.Export.Interval",
                "Energy.Active.Import.Interval",
                "Energy.Reactive.Export.Interval",
                "Energy.Reactive.Import.Interval",
                "Power.Active.Export",
                "Power.Active.Import",
                "Power.Offered",
                "Power.Reactive.Export",
                "Power.Reactive.Import",
                "Power.Factor",
                "Current.Import",
                "Current.Export",
                "Current.Offered",
                "Voltage",
                "Frequency",
                "Temperature",
                "SoC",
                "RPM",
            ],
        );
        assert_schema_strings(
            &[
                Phase::L1,
                Phase::L2,
                Phase::L3,
                Phase::N,
                Phase::L1N,
                Phase::L2N,
                Phase::L3N,
                Phase::L1L2,
                Phase::L2L3,
                Phase::L3L1,
            ],
            &[
                "L1", "L2", "L3", "N", "L1-N", "L2-N", "L3-N", "L1-L2", "L2-L3", "L3-L1",
            ],
        );
        assert_schema_strings(
            &[
                Location::Cable,
                Location::EV,
                Location::Inlet,
                Location::Outlet,
                Location::Body,
            ],
            &["Cable", "EV", "Inlet", "Outlet", "Body"],
        );
        assert_schema_strings(
            &[
                UnitOfMeasure::Wh,
                UnitOfMeasure::KWh,
                UnitOfMeasure::Varh,
                UnitOfMeasure::Kvarh,
                UnitOfMeasure::W,
                UnitOfMeasure::KW,
                UnitOfMeasure::VA,
                UnitOfMeasure::Kva,
                UnitOfMeasure::Var,
                UnitOfMeasure::Kvar,
                UnitOfMeasure::A,
                UnitOfMeasure::V,
                UnitOfMeasure::K,
                UnitOfMeasure::Celcius,
                UnitOfMeasure::Celsius,
                UnitOfMeasure::Fahrenheit,
                UnitOfMeasure::Percent,
            ],
            &[
                "Wh",
                "kWh",
                "varh",
                "kvarh",
                "W",
                "kW",
                "VA",
                "kVA",
                "var",
                "kvar",
                "A",
                "V",
                "K",
                "Celcius",
                "Celsius",
                "Fahrenheit",
                "Percent",
            ],
        );
    }
}
