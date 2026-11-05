//! Turns a connector state into an OCPP 1.6 StatusNotification (section 4.9).
//!
//! The mapping depends on the protocol version: OCPP 2.0.1 will map
//! Preparing, Charging and Finishing to a single `Occupied`, which is why
//! the state itself lives in `connector`.

use std::num::NonZeroU32;

use time::UtcDateTime;

use crate::{
    connector::{Connector, Fault, State},
    ocpp16::{ChargePointErrorCode, ChargePointStatus, StatusNotificationRequest},
    timestamp,
};

/// What the station reports after BootNotification was accepted and after a
/// reconnect (errata 3.22): connector 0, the charge point as a whole, then
/// every connector with its current status. In that order; the caller sends
/// them one after the other.
///
/// Connector 0 may only be Available, Unavailable or Faulted, and the
/// station models only Available, so it is built by a helper that takes no
/// state; `started` is the time that status was reached.
pub fn report(started: UtcDateTime, connectors: &[Connector]) -> Vec<StatusNotificationRequest> {
    let mut requests = vec![charge_point_status(started)];
    requests.extend(
        connectors
            .iter()
            .map(|c| status_notification(c.id, c.state, c.since)),
    );
    requests
}

fn charge_point_status(at: UtcDateTime) -> StatusNotificationRequest {
    StatusNotificationRequest {
        connector_id: 0,
        error_code: ChargePointErrorCode::NoError,
        info: None,
        status: ChargePointStatus::Available,
        timestamp: timestamp::rfc3339(at),
        vendor_id: None,
        vendor_error_code: None,
    }
}

/// The report for connector `connector_id` in `state`, reached at `at`.
///
/// The clock is a parameter, not read here, so the function stays pure.
///
/// Connector 0 (the charge point as a whole) is not representable here:
/// `report` builds it.
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

    fn instant(unix: i64) -> UtcDateTime {
        UtcDateTime::from_unix_timestamp(unix).unwrap()
    }

    #[test]
    fn report_starts_with_connector_0_available_since_the_start() {
        let started = instant(1_793_880_000);
        let reports = report(started, &[]);
        assert_eq!(reports.len(), 1);
        let zero = &reports[0];
        assert_eq!(zero.connector_id, 0);
        assert_eq!(zero.status, ChargePointStatus::Available);
        assert_eq!(zero.error_code, ChargePointErrorCode::NoError);
        assert_eq!(zero.timestamp.as_deref(), Some("2026-11-05T12:00:00Z"));
    }

    #[test]
    fn report_lists_each_connector_with_its_own_state_and_instant() {
        let started = instant(1_793_880_000);
        let one = Connector {
            id: NonZeroU32::new(1).unwrap(),
            state: State::Charging,
            since: instant(1_793_880_060),
        };
        let two = Connector {
            id: NonZeroU32::new(2).unwrap(),
            state: State::Available,
            since: started,
        };
        let reports = report(started, &[one, two]);
        let summary: Vec<_> = reports
            .iter()
            .map(|r| (r.connector_id, r.status, r.timestamp.as_deref()))
            .collect();
        assert_eq!(
            summary,
            [
                (
                    0,
                    ChargePointStatus::Available,
                    Some("2026-11-05T12:00:00Z")
                ),
                (1, ChargePointStatus::Charging, Some("2026-11-05T12:01:00Z")),
                (
                    2,
                    ChargePointStatus::Available,
                    Some("2026-11-05T12:00:00Z")
                ),
            ]
        );
    }

    #[test]
    fn report_keeps_the_error_code_of_a_faulted_connector() {
        let faulted = Connector {
            id: ONE,
            state: State::Faulted {
                fault: Fault::GroundFailure,
                resume: Resume::Charging,
            },
            since: AT,
        };
        let reports = report(AT, &[faulted]);
        assert_eq!(reports[1].status, ChargePointStatus::Faulted);
        assert_eq!(reports[1].error_code, ChargePointErrorCode::GroundFailure);
    }

    #[test]
    fn optional_fields_stay_empty() {
        let req = status_notification(ONE, State::Preparing, AT);
        assert_eq!(req.info, None);
        assert_eq!(req.vendor_id, None);
        assert_eq!(req.vendor_error_code, None);
    }
}
