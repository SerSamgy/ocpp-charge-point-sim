//! OCPP 1.6 transactions: the requests and the decisions about their
//! answers (sections 4.1 Authorize, 4.8 StartTransaction, 4.10
//! StopTransaction, errata 3.17).
//!
//! Pure logic, like `status16`: `session` runs the steps (sends the CALLs,
//! changes the connector) and asks this module what each answer means. The
//! transaction itself (`Transaction`) is 1.6 too: OCPP 2.0.1 replaces
//! Start/StopTransaction with TransactionEvent, where the station chooses a
//! string `transactionId` and the idTag becomes an `IdToken` object.
//!
//! Configuration keys do not exist yet, so the station behaves as if
//! - `StopTransactionOnInvalidId` were true: a transaction whose idTag the
//!   CSMS does not accept in StartTransaction.conf is stopped with reason
//!   `DeAuthorized` (errata 3.17);
//! - `StopTransactionOnEVSideDisconnect` were true: unplugging the cable
//!   while charging stops the transaction with reason `EVDisconnected`
//!   (section 4.10);
//! - `MeterValuesSampledData` were its default,
//!   `Energy.Active.Import.Register` (section 9.1.17): a sample carries the
//!   meter register and nothing else.

use std::{collections::HashMap, fmt, num::NonZeroU32};

use time::UtcDateTime;

use crate::ocpp16::{
    AuthorizationStatus, AuthorizeRequest, AuthorizeResponse, Measurand, MeterValue,
    MeterValuesRequest, ReadingContext, Request, SampledValue, StartTransactionRequest,
    StartTransactionResponse, StopReason, StopTransactionRequest, UnitOfMeasure,
};

/// An idTag (1.6 section 3.9): the identifier of the user, such as the
/// number of an RFID card. At most 20 characters (`IdToken`, a
/// `CiString20Type`, section 7.28; `maxLength` 20 in the schemas).
///
/// The text is kept as typed. 1.6 compares idTags without regard to case,
/// which is up to the CSMS.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdTag(String);

/// Longest idTag, in characters (`maxLength` of the JSON schemas, which
/// counts characters, not bytes).
pub const ID_TAG_MAX_CHARS: usize = 20;

/// Why a text is not an idTag.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InvalidIdTag {
    Empty,
    TooLong { chars: usize },
}

impl fmt::Display for InvalidIdTag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            InvalidIdTag::Empty => write!(f, "an idTag cannot be empty"),
            InvalidIdTag::TooLong { chars } => write!(
                f,
                "an idTag has at most {ID_TAG_MAX_CHARS} characters, this one has {chars}"
            ),
        }
    }
}

impl IdTag {
    pub fn new(text: &str) -> Result<IdTag, InvalidIdTag> {
        let chars = text.chars().count();
        if chars == 0 {
            Err(InvalidIdTag::Empty)
        } else if chars > ID_TAG_MAX_CHARS {
            Err(InvalidIdTag::TooLong { chars })
        } else {
            Ok(IdTag(text.to_owned()))
        }
    }
}

impl fmt::Display for IdTag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// A transaction of the station. `local` is the station's own number for
/// it, counted from 1 in each run; the ID the CSMS gives it lives in
/// `TxIds`, the one place for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Transaction {
    pub local: u32,
    /// The idTag that started it.
    pub id_tag: IdTag,
    /// The meter register at the start, in Wh.
    pub meter_start: u64,
}

/// The transaction IDs the CSMS gave, by local number.
#[derive(Debug, Default)]
pub struct TxIds {
    ids: HashMap<u32, i32>,
}

impl TxIds {
    /// The CSMS answered the StartTransaction of `local`: keep its ID and
    /// say what the answer means (errata 3.17).
    pub fn on_delivered(
        &mut self,
        local: u32,
        response: &StartTransactionResponse,
    ) -> StartDecision {
        self.ids.insert(local, response.transaction_id);
        match response.id_tag_info.status {
            AuthorizationStatus::Accepted => StartDecision::Charge,
            _ => StartDecision::StopDeAuthorized,
        }
    }

    /// The ID of `local`, if the CSMS has given one.
    pub fn get(&self, local: u32) -> Option<i32> {
        self.ids.get(&local).copied()
    }

    /// The ID to send for `local`: -1 if the CSMS never gave one, because
    /// its StartTransaction was not delivered (errata 3.18).
    pub fn resolve(&self, local: u32) -> i32 {
        self.get(local).unwrap_or(-1)
    }

    /// The last message of `local` (its StopTransaction) is done: the ID
    /// is not needed any more.
    pub fn forget(&mut self, local: u32) {
        self.ids.remove(&local);
    }
}

/// Why a transaction ends, in our words; `stop` turns it into the 1.6
/// `reason`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopCause {
    /// The user stopped it (`stop`).
    Local,
    /// The cable was unplugged while charging.
    EvDisconnected,
    /// The CSMS did not accept the idTag in StartTransaction.conf.
    DeAuthorized,
}

pub fn authorize(id_tag: &IdTag) -> AuthorizeRequest {
    AuthorizeRequest {
        id_tag: id_tag.0.clone(),
    }
}

/// `Ok` if the CSMS accepted the idTag, otherwise the status it gave.
/// Only an accepted idTag may charge (section 4.1: "The Charge Point SHALL
/// only supply energy after authorization").
pub fn authorized(response: &AuthorizeResponse) -> Result<(), AuthorizationStatus> {
    match response.id_tag_info.status {
        AuthorizationStatus::Accepted => Ok(()),
        refused => Err(refused),
    }
}

/// StartTransaction for `id_tag` on `connector`, started at `at` with the
/// meter at `meter_start` Wh.
pub fn start(
    connector: NonZeroU32,
    id_tag: &IdTag,
    meter_start: u64,
    at: UtcDateTime,
) -> StartTransactionRequest {
    StartTransactionRequest {
        connector_id: connector.get(),
        id_tag: id_tag.0.clone(),
        meter_start,
        timestamp: at,
    }
}

/// What to do once StartTransaction.conf has arrived.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StartDecision {
    /// The idTag is accepted: charge.
    Charge,
    /// The transaction exists (the CSMS gave it an id), but the CSMS does
    /// not accept the idTag: stop it at once with `StopCause::DeAuthorized`
    /// (errata 3.17, `StopTransactionOnInvalidId` = true).
    StopDeAuthorized,
}

/// What a StopTransaction carries, without the transaction ID: the ID is
/// filled in when the message is sent (`TxMessage::outgoing`), because the
/// CSMS may not have given it yet.
#[derive(Debug, Clone, PartialEq)]
pub struct StopData {
    id_tag: Option<String>,
    pub meter_stop: u64,
    timestamp: UtcDateTime,
    pub reason: Option<StopReason>,
}

/// The stop of `transaction`, ended at `at` with the meter at `meter_stop`
/// Wh.
///
/// `idTag` is the identifier that stopped the transaction. Only a stop by
/// the user carries it (the same idTag that started it: the keyboard has
/// no way to present another one, so no Authorize is needed, section 4.1).
/// When the station stops the transaction by itself, nobody presented an
/// identifier, and section 4.10 allows leaving it out.
pub fn stop(
    transaction: &Transaction,
    cause: StopCause,
    meter_stop: u64,
    at: UtcDateTime,
) -> StopData {
    let id_tag = match cause {
        StopCause::Local => Some(transaction.id_tag.0.clone()),
        StopCause::EvDisconnected | StopCause::DeAuthorized => None,
    };
    StopData {
        id_tag,
        meter_stop,
        timestamp: at,
        reason: reason(cause),
    }
}

/// The 1.6 `reason`. A normal stop by the user leaves it out: section 4.10
/// says the CSMS then assumes `Local`.
fn reason(cause: StopCause) -> Option<StopReason> {
    match cause {
        StopCause::Local => None,
        StopCause::EvDisconnected => Some(StopReason::EVDisconnected),
        StopCause::DeAuthorized => Some(StopReason::DeAuthorized),
    }
}

/// What a periodic sample carries, without the transaction ID (see
/// `StopData`): the meter register, `register_wh`, on `connector` at `at`
/// (section 3.16.1, sampled meter data).
#[derive(Debug, Clone, PartialEq)]
pub struct SampleData {
    connector: NonZeroU32,
    pub register_wh: u64,
    at: UtcDateTime,
}

pub fn sample(connector: NonZeroU32, register_wh: u64, at: UtcDateTime) -> SampleData {
    SampleData {
        connector,
        register_wh,
        at,
    }
}

/// A transaction-related message (section 3.7) waiting in the queue of the
/// station, for the transaction `local`. StartTransaction is not queued
/// yet: it goes out directly (roadmap step 5c2).
#[derive(Debug, Clone, PartialEq)]
pub enum TxMessage {
    Stop { local: u32, data: StopData },
    Sample { local: u32, data: SampleData },
}

/// A queued message as it goes on the wire.
#[derive(Debug, Clone, PartialEq)]
pub enum Outgoing {
    Stop(StopTransactionRequest),
    Sample(MeterValuesRequest),
}

impl TxMessage {
    /// The action name, for the log.
    pub fn action(&self) -> &'static str {
        match self {
            TxMessage::Stop { .. } => StopTransactionRequest::ACTION,
            TxMessage::Sample { .. } => MeterValuesRequest::ACTION,
        }
    }

    /// The transaction the message belongs to.
    pub fn local(&self) -> u32 {
        match self {
            TxMessage::Stop { local, .. } | TxMessage::Sample { local, .. } => *local,
        }
    }

    /// Whether the limit on waiting samples may drop the message. Only a
    /// sample may go: the next one carries a newer register, while a
    /// StopTransaction carries the meterStop the CSMS bills.
    pub fn is_sample(&self) -> bool {
        matches!(self, TxMessage::Sample { .. })
    }

    /// The request to send, with the transaction ID the CSMS gave, or -1
    /// if it never gave one (errata 3.18).
    ///
    /// Measurand, unit and context of a sample are written out although
    /// they are the defaults of section 4.7, so a CSMS need not know the
    /// defaults to read the value.
    pub fn outgoing(&self, ids: &TxIds) -> Outgoing {
        let transaction_id = ids.resolve(self.local());
        match self {
            TxMessage::Stop { data, .. } => Outgoing::Stop(StopTransactionRequest {
                id_tag: data.id_tag.clone(),
                meter_stop: data.meter_stop,
                timestamp: data.timestamp,
                transaction_id,
                reason: data.reason,
            }),
            TxMessage::Sample { data, .. } => Outgoing::Sample(MeterValuesRequest {
                connector_id: data.connector.get(),
                transaction_id: Some(transaction_id),
                meter_value: vec![MeterValue {
                    timestamp: data.at,
                    sampled_value: vec![SampledValue {
                        value: data.register_wh.to_string(),
                        context: Some(ReadingContext::SamplePeriodic),
                        format: None,
                        measurand: Some(Measurand::EnergyActiveImportRegister),
                        phase: None,
                        location: None,
                        unit: Some(UnitOfMeasure::Wh),
                    }],
                }],
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ocpp16::IdTagInfo;

    const ONE: NonZeroU32 = NonZeroU32::new(1).unwrap();
    const AT: UtcDateTime = UtcDateTime::UNIX_EPOCH;

    fn info(status: AuthorizationStatus) -> IdTagInfo {
        IdTagInfo {
            expiry_date: None,
            parent_id_tag: None,
            status,
        }
    }

    const REFUSED: [AuthorizationStatus; 4] = [
        AuthorizationStatus::Blocked,
        AuthorizationStatus::Expired,
        AuthorizationStatus::Invalid,
        AuthorizationStatus::ConcurrentTx,
    ];

    fn transaction() -> Transaction {
        Transaction {
            local: 3,
            id_tag: IdTag::new("DEMO-TAG").unwrap(),
            meter_start: 100,
        }
    }

    #[test]
    fn an_id_tag_keeps_its_case() {
        assert_eq!(IdTag::new("Card-42").unwrap().to_string(), "Card-42");
    }

    #[test]
    fn an_id_tag_has_one_to_twenty_characters() {
        assert_eq!(IdTag::new(""), Err(InvalidIdTag::Empty));
        assert!(IdTag::new(&"A".repeat(20)).is_ok());
        assert_eq!(
            IdTag::new(&"A".repeat(21)),
            Err(InvalidIdTag::TooLong { chars: 21 })
        );
    }

    #[test]
    fn the_length_counts_characters_not_bytes() {
        // 20 Cyrillic letters are 40 bytes in UTF-8, but 20 characters.
        let tag = "Ж".repeat(20);
        assert_eq!(tag.len(), 40);
        assert_eq!(IdTag::new(&tag).unwrap().to_string(), tag);
        assert_eq!(
            IdTag::new(&"Ж".repeat(21)),
            Err(InvalidIdTag::TooLong { chars: 21 })
        );
    }

    #[test]
    fn only_accepted_is_authorized() {
        let accepted = AuthorizeResponse {
            id_tag_info: info(AuthorizationStatus::Accepted),
        };
        assert_eq!(authorized(&accepted), Ok(()));
        for status in REFUSED {
            let response = AuthorizeResponse {
                id_tag_info: info(status),
            };
            assert_eq!(authorized(&response), Err(status));
        }
    }

    #[test]
    fn authorize_and_start_carry_the_id_tag() {
        let tag = IdTag::new("Card-42").unwrap();
        assert_eq!(authorize(&tag).id_tag, "Card-42");
        let request = start(ONE, &tag, 246, AT);
        assert_eq!(
            request,
            StartTransactionRequest {
                connector_id: 1,
                id_tag: "Card-42".into(),
                meter_start: 246,
                timestamp: AT,
            }
        );
    }

    fn start_response(status: AuthorizationStatus) -> StartTransactionResponse {
        StartTransactionResponse {
            id_tag_info: info(status),
            transaction_id: 7,
        }
    }

    /// IDs where transaction 3 has the CSMS ID 7.
    fn ids() -> TxIds {
        let mut ids = TxIds::default();
        ids.on_delivered(3, &start_response(AuthorizationStatus::Accepted));
        ids
    }

    #[test]
    fn a_start_conf_not_accepted_stops_the_transaction() {
        let mut ids = TxIds::default();
        assert_eq!(
            ids.on_delivered(1, &start_response(AuthorizationStatus::Accepted)),
            StartDecision::Charge
        );
        for status in REFUSED {
            assert_eq!(
                ids.on_delivered(1, &start_response(status)),
                StartDecision::StopDeAuthorized,
                "{status:?}"
            );
        }
    }

    #[test]
    fn an_id_is_kept_until_forgotten_and_minus_one_without_one() {
        let mut ids = ids();
        assert_eq!(ids.get(3), Some(7));
        assert_eq!(ids.resolve(3), 7);
        // Errata 3.18: a transaction whose StartTransaction was never
        // answered is sent with -1.
        assert_eq!(ids.get(4), None);
        assert_eq!(ids.resolve(4), -1);
        ids.forget(3);
        assert_eq!(ids.get(3), None);
    }

    #[test]
    fn a_stop_by_the_user_carries_the_id_tag_and_no_reason() {
        let message = TxMessage::Stop {
            local: 3,
            data: stop(&transaction(), StopCause::Local, 346, AT),
        };
        assert_eq!(
            message.outgoing(&ids()),
            Outgoing::Stop(StopTransactionRequest {
                id_tag: Some("DEMO-TAG".into()),
                meter_stop: 346,
                timestamp: AT,
                transaction_id: 7,
                reason: None,
            })
        );
    }

    #[test]
    fn a_stop_by_the_station_has_a_reason_and_no_id_tag() {
        let causes = [
            (StopCause::EvDisconnected, StopReason::EVDisconnected),
            (StopCause::DeAuthorized, StopReason::DeAuthorized),
        ];
        for (cause, reason) in causes {
            let message = TxMessage::Stop {
                local: 3,
                data: stop(&transaction(), cause, 100, AT),
            };
            let Outgoing::Stop(request) = message.outgoing(&ids()) else {
                panic!("not a stop");
            };
            assert_eq!(request.reason, Some(reason));
            assert_eq!(request.id_tag, None);
            assert_eq!(request.transaction_id, 7);
            assert_eq!(request.meter_stop, 100);
        }
    }

    #[test]
    fn a_sample_carries_the_register_of_the_transaction() {
        let message = TxMessage::Sample {
            local: 3,
            data: sample(ONE, 246, AT),
        };
        let Outgoing::Sample(request) = message.outgoing(&ids()) else {
            panic!("not a sample");
        };
        assert_eq!(
            serde_json::to_value(request).unwrap(),
            serde_json::json!({
                "connectorId": 1,
                "transactionId": 7,
                "meterValue": [{
                    "timestamp": "1970-01-01T00:00:00Z",
                    "sampledValue": [{
                        "value": "246",
                        "context": "Sample.Periodic",
                        "measurand": "Energy.Active.Import.Register",
                        "unit": "Wh",
                    }],
                }],
            })
        );
    }

    #[test]
    fn messages_without_an_id_go_with_minus_one() {
        let ids = TxIds::default();
        let stop = TxMessage::Stop {
            local: 3,
            data: stop(&transaction(), StopCause::Local, 346, AT),
        };
        let Outgoing::Stop(request) = stop.outgoing(&ids) else {
            panic!("not a stop");
        };
        assert_eq!(request.transaction_id, -1);
        let sample = TxMessage::Sample {
            local: 3,
            data: sample(ONE, 246, AT),
        };
        let Outgoing::Sample(request) = sample.outgoing(&ids) else {
            panic!("not a sample");
        };
        assert_eq!(request.transaction_id, Some(-1));
    }

    #[test]
    fn a_queued_message_names_its_action_and_transaction() {
        let stop = TxMessage::Stop {
            local: 3,
            data: stop(&transaction(), StopCause::Local, 346, AT),
        };
        assert_eq!(stop.action(), "StopTransaction");
        assert_eq!(stop.local(), 3);
        assert!(!stop.is_sample());

        let sample = TxMessage::Sample {
            local: 3,
            data: sample(ONE, 246, AT),
        };
        assert_eq!(sample.action(), "MeterValues");
        assert_eq!(sample.local(), 3);
        assert!(sample.is_sample());
    }
}
