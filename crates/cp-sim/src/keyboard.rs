//! Commands typed on the keyboard, as connector events.
//!
//! Until buttons exist (roadmap step 8), the keyboard plays the plug, the
//! card reader and the fault detector. This module only turns a line of
//! text into an `Event`; reading stdin is a separate, thin piece of I/O.
//!
//! Limitations: a command is checked against the state of the connector
//! only when the session applies it, which happens after the station has
//! been accepted by the CSMS, so the message for a command that does not
//! fit can come late. The channel to the session is bounded; if it is full
//! (the station has been offline for a long time), the keyboard thread waits
//! and stops reading lines until the session takes the inputs.

use std::fmt;

use crate::connector::{Event, Fault};

/// Printed by the `help` command and when stdin is a terminal.
pub const HELP: &str = "commands: plug, start, stop, unplug, clear, fault <kind>, help\n\
                        fault kinds: ground, overcurrent, overvoltage, undervoltage, temperature, other";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseError {
    Empty,
    Unknown(String),
    MissingFaultKind,
    UnknownFaultKind(String),
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParseError::Empty => write!(f, "empty command"),
            ParseError::Unknown(text) => write!(f, "unknown command {text:?}\n{HELP}"),
            ParseError::MissingFaultKind => write!(f, "fault needs a kind\n{HELP}"),
            ParseError::UnknownFaultKind(kind) => {
                write!(f, "unknown fault kind {kind:?}\n{HELP}")
            }
        }
    }
}

/// One command per line, case-insensitive, extra whitespace ignored.
pub fn parse(line: &str) -> Result<Event, ParseError> {
    let lowered = line.to_lowercase();
    let words: Vec<&str> = lowered.split_whitespace().collect();
    match words.as_slice() {
        [] => Err(ParseError::Empty),
        ["plug"] => Ok(Event::PlugIn),
        ["start"] => Ok(Event::StartCharging),
        ["stop"] => Ok(Event::StopCharging),
        ["unplug"] => Ok(Event::Unplug),
        ["clear"] => Ok(Event::ClearFault),
        ["fault"] => Err(ParseError::MissingFaultKind),
        ["fault", kind] => fault(kind).map(Event::Fault),
        _ => Err(ParseError::Unknown(words.join(" "))),
    }
}

fn fault(kind: &str) -> Result<Fault, ParseError> {
    match kind {
        "ground" => Ok(Fault::GroundFailure),
        "overcurrent" => Ok(Fault::OverCurrent),
        "overvoltage" => Ok(Fault::OverVoltage),
        "undervoltage" => Ok(Fault::UnderVoltage),
        "temperature" => Ok(Fault::HighTemperature),
        "other" => Ok(Fault::Other),
        _ => Err(ParseError::UnknownFaultKind(kind.to_owned())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simple_commands() {
        assert_eq!(parse("plug"), Ok(Event::PlugIn));
        assert_eq!(parse("start"), Ok(Event::StartCharging));
        assert_eq!(parse("stop"), Ok(Event::StopCharging));
        assert_eq!(parse("unplug"), Ok(Event::Unplug));
        assert_eq!(parse("clear"), Ok(Event::ClearFault));
    }

    #[test]
    fn case_and_whitespace_do_not_matter() {
        assert_eq!(parse("  PLUG \n"), Ok(Event::PlugIn));
        assert_eq!(parse("Start"), Ok(Event::StartCharging));
        assert_eq!(
            parse("fault   OverCurrent\t"),
            Ok(Event::Fault(Fault::OverCurrent))
        );
    }

    #[test]
    fn every_fault_kind() {
        let kinds = [
            ("ground", Fault::GroundFailure),
            ("overcurrent", Fault::OverCurrent),
            ("overvoltage", Fault::OverVoltage),
            ("undervoltage", Fault::UnderVoltage),
            ("temperature", Fault::HighTemperature),
            ("other", Fault::Other),
        ];
        for (word, fault) in kinds {
            assert_eq!(parse(&format!("fault {word}")), Ok(Event::Fault(fault)));
        }
    }

    #[test]
    fn fault_needs_a_known_kind() {
        assert_eq!(parse("fault"), Err(ParseError::MissingFaultKind));
        assert_eq!(
            parse("fault smoke"),
            Err(ParseError::UnknownFaultKind("smoke".into()))
        );
    }

    #[test]
    fn unknown_commands_and_extra_words_are_errors() {
        assert_eq!(parse("fly"), Err(ParseError::Unknown("fly".into())));
        assert_eq!(
            parse("plug now"),
            Err(ParseError::Unknown("plug now".into()))
        );
        assert_eq!(
            parse("fault ground badly"),
            Err(ParseError::Unknown("fault ground badly".into()))
        );
    }

    #[test]
    fn an_empty_line_is_empty() {
        assert_eq!(parse(""), Err(ParseError::Empty));
        assert_eq!(parse("   \n"), Err(ParseError::Empty));
    }

    #[test]
    fn errors_tell_the_user_what_to_type() {
        let message = ParseError::Unknown("fly".into()).to_string();
        assert!(
            message.contains("fly") && message.contains("plug"),
            "{message}"
        );
    }
}
