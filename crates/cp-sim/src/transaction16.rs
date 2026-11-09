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
//!   (section 4.10).

use std::{fmt, num::NonZeroU32};

use time::UtcDateTime;

use crate::ocpp16::{
    AuthorizationStatus, AuthorizeRequest, AuthorizeResponse, StartTransactionRequest,
    StartTransactionResponse, StopReason, StopTransactionRequest,
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

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for IdTag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// A transaction the CSMS knows about: it answered StartTransaction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Transaction {
    /// Chosen by the CSMS in StartTransaction.conf.
    pub id: i32,
    /// The idTag that started it.
    pub id_tag: IdTag,
    /// The meter register at the start, in Wh.
    pub meter_start: u64,
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

pub fn on_start_conf(response: &StartTransactionResponse) -> StartDecision {
    match response.id_tag_info.status {
        AuthorizationStatus::Accepted => StartDecision::Charge,
        _ => StartDecision::StopDeAuthorized,
    }
}

/// StopTransaction for `transaction`, ended at `at` with the meter at
/// `meter_stop` Wh.
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
) -> StopTransactionRequest {
    let id_tag = match cause {
        StopCause::Local => Some(transaction.id_tag.0.clone()),
        StopCause::EvDisconnected | StopCause::DeAuthorized => None,
    };
    StopTransactionRequest {
        id_tag,
        meter_stop,
        timestamp: at,
        transaction_id: transaction.id,
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
            id: 7,
            id_tag: IdTag::new("DEMO-TAG").unwrap(),
            meter_start: 100,
        }
    }

    #[test]
    fn an_id_tag_keeps_its_case() {
        assert_eq!(IdTag::new("Card-42").unwrap().as_str(), "Card-42");
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
        assert_eq!(IdTag::new(&tag).unwrap().as_str(), tag);
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

    #[test]
    fn a_start_conf_not_accepted_stops_the_transaction() {
        let response = |status| StartTransactionResponse {
            id_tag_info: info(status),
            transaction_id: 7,
        };
        assert_eq!(
            on_start_conf(&response(AuthorizationStatus::Accepted)),
            StartDecision::Charge
        );
        for status in REFUSED {
            assert_eq!(
                on_start_conf(&response(status)),
                StartDecision::StopDeAuthorized,
                "{status:?}"
            );
        }
    }

    #[test]
    fn a_stop_by_the_user_carries_the_id_tag_and_no_reason() {
        assert_eq!(
            stop(&transaction(), StopCause::Local, 346, AT),
            StopTransactionRequest {
                id_tag: Some("DEMO-TAG".into()),
                meter_stop: 346,
                timestamp: AT,
                transaction_id: 7,
                reason: None,
            }
        );
    }

    #[test]
    fn a_stop_by_the_station_has_a_reason_and_no_id_tag() {
        let causes = [
            (StopCause::EvDisconnected, StopReason::EVDisconnected),
            (StopCause::DeAuthorized, StopReason::DeAuthorized),
        ];
        for (cause, reason) in causes {
            let request = stop(&transaction(), cause, 100, AT);
            assert_eq!(request.reason, Some(reason));
            assert_eq!(request.id_tag, None);
            assert_eq!(request.transaction_id, 7);
            assert_eq!(request.meter_stop, 100);
        }
    }
}
