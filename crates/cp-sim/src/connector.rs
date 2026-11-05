//! The state of one connector and the events that change it.
//!
//! Pure logic: no I/O, no clock, no OCPP types. The statuses follow the
//! table of OCPP 1.6 section 4.9, but they are our own enum, because OCPP
//! 2.0.1 reports a coarser status (Preparing, Charging and Finishing are all
//! `Occupied` there). Turning a state into a wire status is the job of the
//! version-specific module (`status16`).
//!
//! Limitations of this first version:
//! - While faulted, plug and unplug events are rejected, so `resume` may
//!   disagree with the real cable until the hardware reports it (step 8).
//! - A second fault while faulted is rejected: its cause is lost and
//!   `ClearFault` clears the fault as a whole.
//! - B6 (Preparing to Finishing) and F2 (Finishing to Preparing) are in the
//!   1.6 table, but no event produces them yet. After Finishing with the
//!   cable still plugged in, only `Unplug` works.

use std::num::NonZeroU32;

use time::UtcDateTime;

/// Why a connector is faulted. Each cause has one OCPP error code; the
/// warning-only `EVCommunicationError` is deliberately not here, because
/// section 4.9 forbids it together with the status Faulted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fault {
    GroundFailure,
    OverCurrent,
    OverVoltage,
    UnderVoltage,
    HighTemperature,
    Other,
}

/// The state a connector had before it became faulted: where it returns to
/// when the fault is cleared (1.6 table, I1-I8). A separate enum without
/// `Faulted`, so a fault cannot nest.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Resume {
    Available,
    Preparing,
    Charging,
    Finishing,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Available,
    Preparing,
    Charging,
    Finishing,
    Faulted { fault: Fault, resume: Resume },
}

impl From<Resume> for State {
    fn from(resume: Resume) -> Self {
        match resume {
            Resume::Available => State::Available,
            Resume::Preparing => State::Preparing,
            Resume::Charging => State::Charging,
            Resume::Finishing => State::Finishing,
        }
    }
}

/// What happens at the connector, in our words rather than OCPP statuses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event {
    /// The user starts using the connector (cable inserted).
    PlugIn,
    /// Authorized and the EV takes energy.
    StartCharging,
    /// Charging ends, the cable stays plugged in.
    StopCharging,
    /// The cable is removed.
    Unplug,
    Fault(Fault),
    ClearFault,
}

/// An event that makes no sense in the current state; the state stays as it was.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InvalidEvent {
    pub state: State,
    pub event: Event,
}

/// One connector as the station keeps it: its number, its state and the
/// instant the state was reached (OCPP 1.6 section 6.47: the `timestamp` of
/// a StatusNotification is the time for which the status is reported).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Connector {
    pub id: NonZeroU32,
    pub state: State,
    pub since: UtcDateTime,
}

impl Connector {
    /// A connector that is Available since `since`. The instant is passed
    /// in, not read from a clock, so this module stays free of I/O.
    pub fn new(id: NonZeroU32, since: UtcDateTime) -> Self {
        Connector {
            id,
            state: State::Available,
            since,
        }
    }
}

impl State {
    /// The state after `event`, or `InvalidEvent` if the event does not fit
    /// (for example `StartCharging` while Available). Events are never
    /// silently ignored, so the caller can log them.
    pub fn on(self, event: Event) -> Result<State, InvalidEvent> {
        let faulted = |fault, resume| Ok(State::Faulted { fault, resume });
        match (self, event) {
            // A2
            (State::Available, Event::PlugIn) => Ok(State::Preparing),
            // B3
            (State::Preparing, Event::StartCharging) => Ok(State::Charging),
            // C6
            (State::Charging, Event::StopCharging) => Ok(State::Finishing),
            // B1, C1 (the EV is gone, the transaction ends), F1
            (State::Preparing | State::Charging | State::Finishing, Event::Unplug) => {
                Ok(State::Available)
            }
            // I1-I8
            (State::Faulted { resume, .. }, Event::ClearFault) => Ok(resume.into()),
            // A9, B9, C9, F9
            (State::Available, Event::Fault(fault)) => faulted(fault, Resume::Available),
            (State::Preparing, Event::Fault(fault)) => faulted(fault, Resume::Preparing),
            (State::Charging, Event::Fault(fault)) => faulted(fault, Resume::Charging),
            (State::Finishing, Event::Fault(fault)) => faulted(fault, Resume::Finishing),
            // Everything else, in particular every event except `ClearFault`
            // while faulted: none of the arms above matches a faulted state.
            _ => Err(InvalidEvent { state: self, event }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FAULTS: [Fault; 6] = [
        Fault::GroundFailure,
        Fault::OverCurrent,
        Fault::OverVoltage,
        Fault::UnderVoltage,
        Fault::HighTemperature,
        Fault::Other,
    ];

    const RESUMES: [Resume; 4] = [
        Resume::Available,
        Resume::Preparing,
        Resume::Charging,
        Resume::Finishing,
    ];

    const NORMAL: [State; 4] = [
        State::Available,
        State::Preparing,
        State::Charging,
        State::Finishing,
    ];

    fn all_states() -> Vec<State> {
        let mut states = NORMAL.to_vec();
        for fault in FAULTS {
            for resume in RESUMES {
                states.push(State::Faulted { fault, resume });
            }
        }
        states
    }

    fn all_events() -> Vec<Event> {
        let mut events = vec![
            Event::PlugIn,
            Event::StartCharging,
            Event::StopCharging,
            Event::Unplug,
            Event::ClearFault,
        ];
        events.extend(FAULTS.map(Event::Fault));
        events
    }

    /// The resume state of a state that is not faulted.
    fn resume_of(state: State) -> Resume {
        match state {
            State::Available => Resume::Available,
            State::Preparing => Resume::Preparing,
            State::Charging => Resume::Charging,
            State::Finishing => Resume::Finishing,
            State::Faulted { .. } => panic!("a faulted state has no resume of its own"),
        }
    }

    /// Every transition the machine may make, written out as data: the
    /// edges of the 1.6 table that our events produce, plus the fault edges.
    /// Deliberately not derived from `on`, so the two can disagree.
    fn valid_edges() -> Vec<(State, Event, State)> {
        let mut edges = vec![
            (State::Available, Event::PlugIn, State::Preparing),
            (State::Preparing, Event::StartCharging, State::Charging),
            (State::Charging, Event::StopCharging, State::Finishing),
            (State::Preparing, Event::Unplug, State::Available),
            (State::Charging, Event::Unplug, State::Available),
            (State::Finishing, Event::Unplug, State::Available),
        ];
        for fault in FAULTS {
            for state in NORMAL {
                let faulted = State::Faulted {
                    fault,
                    resume: resume_of(state),
                };
                edges.push((state, Event::Fault(fault), faulted));
                edges.push((faulted, Event::ClearFault, state));
            }
        }
        edges
    }

    fn step(state: State, event: Event) -> State {
        state
            .on(event)
            .unwrap_or_else(|err| panic!("{err:?} should be valid"))
    }

    #[test]
    fn plug_in_moves_available_to_preparing_a2() {
        assert_eq!(step(State::Available, Event::PlugIn), State::Preparing);
    }

    #[test]
    fn start_charging_moves_preparing_to_charging_b3() {
        assert_eq!(
            step(State::Preparing, Event::StartCharging),
            State::Charging
        );
    }

    #[test]
    fn stop_charging_moves_charging_to_finishing_c6() {
        assert_eq!(step(State::Charging, Event::StopCharging), State::Finishing);
    }

    #[test]
    fn unplug_ends_the_usage_in_preparing_b1() {
        assert_eq!(step(State::Preparing, Event::Unplug), State::Available);
    }

    #[test]
    fn unplug_while_charging_goes_straight_to_available_c1() {
        assert_eq!(step(State::Charging, Event::Unplug), State::Available);
    }

    #[test]
    fn unplug_completes_finishing_f1() {
        assert_eq!(step(State::Finishing, Event::Unplug), State::Available);
    }

    #[test]
    fn fault_from_every_state_remembers_it_and_clear_fault_resumes() {
        for state in NORMAL {
            for fault in FAULTS {
                let faulted = step(state, Event::Fault(fault));
                assert_eq!(
                    faulted,
                    State::Faulted {
                        fault,
                        resume: resume_of(state)
                    }
                );
                assert_eq!(step(faulted, Event::ClearFault), state);
            }
        }
    }

    #[test]
    fn second_fault_while_faulted_is_invalid() {
        let faulted = State::Faulted {
            fault: Fault::OverCurrent,
            resume: Resume::Charging,
        };
        assert_eq!(
            faulted.on(Event::Fault(Fault::GroundFailure)),
            Err(InvalidEvent {
                state: faulted,
                event: Event::Fault(Fault::GroundFailure)
            })
        );
    }

    #[test]
    fn invalid_events_leave_state_unchanged() {
        let edges = valid_edges();
        let mut invalid = 0;
        for state in all_states() {
            for event in all_events() {
                if edges.iter().any(|&(s, e, _)| (s, e) == (state, event)) {
                    continue;
                }
                invalid += 1;
                assert_eq!(
                    state.on(event),
                    Err(InvalidEvent { state, event }),
                    "{event:?} in {state:?} should be invalid"
                );
            }
        }
        // 28 states x 11 events, minus the valid edges: a guard against the
        // loops above silently covering nothing.
        assert_eq!(invalid, 28 * 11 - edges.len());
    }

    #[test]
    fn every_valid_edge_leads_where_the_table_says() {
        for (state, event, next) in valid_edges() {
            assert_eq!(state.on(event), Ok(next), "{event:?} in {state:?}");
        }
    }

    /// Name of the status a state reports, for the table below.
    fn name(state: State) -> &'static str {
        match state {
            State::Available => "Available",
            State::Preparing => "Preparing",
            State::Charging => "Charging",
            State::Finishing => "Finishing",
            State::Faulted { .. } => "Faulted",
        }
    }

    #[test]
    fn no_transition_outside_the_16_table() {
        // OCPP 1.6 section 4.9 (A2, A9, B1, B3, B6, B9, C1, C6, C9, F1, F2,
        // F9) restricted to our four statuses and Faulted.
        const TABLE: [(&str, &str); 12] = [
            ("Available", "Preparing"),
            ("Available", "Faulted"),
            ("Preparing", "Available"),
            ("Preparing", "Charging"),
            ("Preparing", "Finishing"),
            ("Preparing", "Faulted"),
            ("Charging", "Available"),
            ("Charging", "Finishing"),
            ("Charging", "Faulted"),
            ("Finishing", "Available"),
            ("Finishing", "Preparing"),
            ("Finishing", "Faulted"),
        ];
        let mut transitions = 0;
        for from in all_states() {
            for event in all_events() {
                let Ok(to) = from.on(event) else { continue };
                transitions += 1;
                match from {
                    // I1-I8: back to the pre-fault state, nowhere else.
                    State::Faulted { resume, .. } => assert_eq!(to, State::from(resume)),
                    _ => assert!(
                        TABLE.contains(&(name(from), name(to))),
                        "{event:?} would move {from:?} to {to:?}, which the table does not allow"
                    ),
                }
            }
        }
        assert_eq!(transitions, valid_edges().len());
    }

    #[test]
    fn a_new_connector_is_available_since_the_given_instant() {
        let id = NonZeroU32::new(1).unwrap();
        let since = UtcDateTime::from_unix_timestamp(1_793_880_000).unwrap();
        assert_eq!(
            Connector::new(id, since),
            Connector {
                id,
                state: State::Available,
                since
            }
        );
    }
}
