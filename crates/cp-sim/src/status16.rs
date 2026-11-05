//! Turns a connector state into an OCPP 1.6 StatusNotification (section 4.9).
//!
//! The mapping depends on the protocol version: OCPP 2.0.1 will map
//! Preparing, Charging and Finishing to a single `Occupied`, which is why
//! the state itself lives in `connector`.

use std::num::NonZeroU32;

use time::UtcDateTime;

use crate::{
    connector::{Fault, State},
    ocpp16::{ChargePointErrorCode, ChargePointStatus, StatusNotificationRequest},
    timestamp,
};

/// The report for connector `connector_id` in `state`, reached at `at`.
///
/// The clock is a parameter, not read here, so the function stays pure.
/// Which instant to pass is up to the caller (step 4b).
///
/// Connector 0 (the charge point as a whole) is not representable here:
/// it may only be Available, Unavailable or Faulted, so it gets its own
/// function when it is needed (step 4b).
pub fn status_notification(
    connector_id: NonZeroU32,
    state: State,
    at: UtcDateTime,
) -> StatusNotificationRequest {
    let (status, error_code) = match state {
        State::Available => (ChargePointStatus::Available, ChargePointErrorCode::NoError),
        State::Preparing => (ChargePointStatus::Preparing, ChargePointErrorCode::NoError),
        State::Charging => (ChargePointStatus::Charging, ChargePointErrorCode::NoError),
        State::Finishing => (ChargePointStatus::Finishing, ChargePointErrorCode::NoError),
        State::Faulted { fault, .. } => (ChargePointStatus::Faulted, error_code(fault)),
    };
    StatusNotificationRequest {
        connector_id: connector_id.get(),
        error_code,
        info: None,
        status,
        timestamp: timestamp::rfc3339(at),
        vendor_id: None,
        vendor_error_code: None,
    }
}

fn error_code(fault: Fault) -> ChargePointErrorCode {
    match fault {
        Fault::GroundFailure => ChargePointErrorCode::GroundFailure,
        Fault::OverCurrent => ChargePointErrorCode::OverCurrentFailure,
        Fault::OverVoltage => ChargePointErrorCode::OverVoltage,
        Fault::UnderVoltage => ChargePointErrorCode::UnderVoltage,
        Fault::HighTemperature => ChargePointErrorCode::HighTemperature,
        Fault::Other => ChargePointErrorCode::OtherError,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::connector::Resume;

    const ONE: NonZeroU32 = NonZeroU32::new(1).unwrap();
    const AT: UtcDateTime = UtcDateTime::UNIX_EPOCH;

    const FAULTS: [(Fault, ChargePointErrorCode); 6] = [
        (Fault::GroundFailure, ChargePointErrorCode::GroundFailure),
        (Fault::OverCurrent, ChargePointErrorCode::OverCurrentFailure),
        (Fault::OverVoltage, ChargePointErrorCode::OverVoltage),
        (Fault::UnderVoltage, ChargePointErrorCode::UnderVoltage),
        (
            Fault::HighTemperature,
            ChargePointErrorCode::HighTemperature,
        ),
        (Fault::Other, ChargePointErrorCode::OtherError),
    ];

    #[test]
    fn states_without_a_fault_report_no_error() {
        let states = [
            (State::Available, ChargePointStatus::Available),
            (State::Preparing, ChargePointStatus::Preparing),
            (State::Charging, ChargePointStatus::Charging),
            (State::Finishing, ChargePointStatus::Finishing),
        ];
        for (state, status) in states {
            let req = status_notification(ONE, state, AT);
            assert_eq!(req.status, status);
            assert_eq!(req.error_code, ChargePointErrorCode::NoError);
        }
    }

    #[test]
    fn every_fault_maps_to_its_error_code() {
        for (fault, code) in FAULTS {
            let state = State::Faulted {
                fault,
                resume: Resume::Available,
            };
            let req = status_notification(ONE, state, AT);
            assert_eq!(req.status, ChargePointStatus::Faulted);
            assert_eq!(req.error_code, code, "{fault:?}");
        }
    }

    #[test]
    fn faulted_status_does_not_depend_on_resume() {
        for resume in [
            Resume::Available,
            Resume::Preparing,
            Resume::Charging,
            Resume::Finishing,
        ] {
            let state = State::Faulted {
                fault: Fault::OverCurrent,
                resume,
            };
            let req = status_notification(ONE, state, AT);
            assert_eq!(req.status, ChargePointStatus::Faulted);
            assert_eq!(req.error_code, ChargePointErrorCode::OverCurrentFailure);
        }
    }

    #[test]
    fn no_fault_maps_to_ev_communication_error() {
        // Section 4.9: EVCommunicationError is a warning and must never go
        // together with the status Faulted.
        for (fault, _) in FAULTS {
            let state = State::Faulted {
                fault,
                resume: Resume::Charging,
            };
            assert_ne!(
                status_notification(ONE, state, AT).error_code,
                ChargePointErrorCode::EVCommunicationError
            );
        }
    }

    #[test]
    fn connector_id_is_passed_through() {
        let two = NonZeroU32::new(2).unwrap();
        assert_eq!(
            status_notification(two, State::Available, AT).connector_id,
            2
        );
    }

    #[test]
    fn timestamp_is_the_given_instant() {
        let at = UtcDateTime::from_unix_timestamp(1_793_880_000).unwrap();
        let req = status_notification(ONE, State::Preparing, at);
        assert_eq!(req.timestamp.as_deref(), Some("2026-11-05T12:00:00Z"));
    }

    #[test]
    fn optional_fields_stay_empty() {
        let req = status_notification(ONE, State::Preparing, AT);
        assert_eq!(req.info, None);
        assert_eq!(req.vendor_id, None);
        assert_eq!(req.vendor_error_code, None);
    }
}
