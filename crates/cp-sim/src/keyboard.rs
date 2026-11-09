//! Commands typed on the keyboard.
//!
//! Until buttons exist (roadmap step 8), the keyboard plays the plug, the
//! card reader and the fault detector. This module only turns a line of
//! text into a `Command`; reading stdin is a separate, thin piece of I/O.
//! A command says what the user did; `session` decides what it means for
//! the connector.
//!
//! Limitations: a command is checked against the state of the connector
//! only when the session applies it, which happens after the station has
//! been accepted by the CSMS, so the message for a command that does not
//! fit can come late. The channel to the session is bounded; if it is full
//! (the station has been offline for a long time), the keyboard thread waits
//! and stops reading lines until the session takes the inputs.

use std::{
    fmt,
    io::{self, BufRead, IsTerminal},
    thread,
};

use time::UtcDateTime;
use tokio::sync::mpsc;
use tracing::warn;

use crate::connector::Fault;

/// Printed by the `help` command and when stdin is a terminal.
pub const HELP: &str = "commands: plug, start [idTag], stop, unplug, clear, fault <kind>, help\n\
                        start without an idTag presents DEMO-TAG\n\
                        fault kinds: ground, overcurrent, overvoltage, undervoltage, temperature, other";

/// The idTag that `start` presents when none is typed: the card a demo
/// user would hold. Any idTag works, unless the CSMS blocks it.
pub const DEFAULT_ID_TAG: &str = "DEMO-TAG";

/// What the user did at the charge point.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// `plug`: the cable is inserted.
    Plug,
    /// `start [idTag]`: the user presents an identifier, like an RFID card
    /// held to the reader. Whether charging starts is up to the CSMS. The
    /// text is kept as typed; `session` checks that it is a valid idTag.
    Present { id_tag: String },
    /// `stop`: charging stops, the cable stays plugged in.
    Stop,
    /// `unplug`: the cable is removed.
    Unplug,
    /// `fault <kind>`: a fault is detected.
    Fault(Fault),
    /// `clear`: the fault is gone.
    Clear,
}

/// A command with the instant the user typed it. The instant is the time the
/// connector reaches its new status, even if the station applies the command
/// later, after a reconnect.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Input {
    pub command: Command,
    pub at: UtcDateTime,
}

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

/// Starts the keyboard on a thread of its own and returns at once.
///
/// Not `tokio::io::stdin`: a pending read of that sits in Tokio's blocking
/// pool, and dropping the runtime waits for blocking tasks, so after Ctrl+C
/// the program would hang until the user pressed Enter. A plain thread that
/// is never joined dies with the process. The thread ends at end of file, on
/// a read error, or when the session has dropped the receiver.
#[expect(
    clippy::print_stderr,
    reason = "replies to what the owner types, not log events: a log filter must not hide them"
)]
pub fn spawn(inputs: mpsc::Sender<Input>) -> io::Result<()> {
    thread::Builder::new()
        .name("keyboard".into())
        .spawn(move || {
            let stdin = io::stdin();
            if stdin.is_terminal() {
                eprintln!("{HELP}");
            }
            read_inputs(stdin.lock(), &inputs, UtcDateTime::now);
            warn!("keyboard input ended; the station keeps running without it");
        })?;
    Ok(())
}

/// Reads lines from `reader` and sends the commands as inputs stamped with
/// `now()`, the instant the line arrived. A bad line prints a message and
/// reading goes on, also when the line is not valid UTF-8 (the bytes of it
/// are consumed, so there is no loop); any other read error ends it, because
/// a failing input would fail again at once. `blocking_send` waits when the
/// channel is full. Returns at end of file, on a read error, or when the
/// receiver is gone.
#[expect(
    clippy::print_stderr,
    reason = "replies to what the owner types, not log events: a log filter must not hide them"
)]
fn read_inputs(
    mut reader: impl BufRead,
    inputs: &mpsc::Sender<Input>,
    now: impl Fn() -> UtcDateTime,
) {
    let mut line = String::new();
    loop {
        line.clear();
        match reader.read_line(&mut line) {
            Ok(0) => return,
            Ok(_) => {}
            Err(err) if err.kind() == io::ErrorKind::InvalidData => {
                eprintln!("that line is not valid UTF-8, ignored");
                continue;
            }
            Err(err) => {
                warn!(%err, "cannot read the keyboard");
                return;
            }
        }
        let text = line.trim();
        if text.is_empty() {
            continue;
        }
        if text.eq_ignore_ascii_case("help") {
            eprintln!("{HELP}");
            continue;
        }
        match parse(text) {
            Ok(command) => {
                let input = Input { command, at: now() };
                if inputs.blocking_send(input).is_err() {
                    return;
                }
            }
            Err(err) => eprintln!("{err}"),
        }
    }
}

/// One command per line, extra whitespace ignored. Command words are
/// case-insensitive; an idTag keeps its case, because it is the user's
/// identifier, not ours.
pub fn parse(line: &str) -> Result<Command, ParseError> {
    let original: Vec<&str> = line.split_whitespace().collect();
    let lowered: Vec<String> = original.iter().map(|word| word.to_lowercase()).collect();
    let words: Vec<&str> = lowered.iter().map(String::as_str).collect();
    match words.as_slice() {
        [] => Err(ParseError::Empty),
        ["plug"] => Ok(Command::Plug),
        ["start"] => Ok(Command::Present {
            id_tag: DEFAULT_ID_TAG.to_owned(),
        }),
        ["start", _] => Ok(Command::Present {
            id_tag: original[1].to_owned(),
        }),
        ["stop"] => Ok(Command::Stop),
        ["unplug"] => Ok(Command::Unplug),
        ["clear"] => Ok(Command::Clear),
        ["fault"] => Err(ParseError::MissingFaultKind),
        ["fault", kind] => fault(kind).map(Command::Fault),
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
    use std::io::Cursor;

    use super::*;

    const T0: UtcDateTime = UtcDateTime::UNIX_EPOCH;

    /// Runs `read_inputs` over `bytes` and returns what it sent.
    fn read_all(bytes: &[u8]) -> Vec<Input> {
        let (tx, mut rx) = mpsc::channel(16);
        read_inputs(Cursor::new(bytes.to_vec()), &tx, || T0);
        drop(tx);
        let mut inputs = Vec::new();
        while let Some(input) = rx.blocking_recv() {
            inputs.push(input);
        }
        inputs
    }

    #[test]
    fn lines_become_inputs_stamped_with_the_clock() {
        let inputs = read_all(b"plug\nstart\n");
        assert_eq!(
            inputs,
            [
                Input {
                    command: Command::Plug,
                    at: T0
                },
                Input {
                    command: Command::Present {
                        id_tag: DEFAULT_ID_TAG.into()
                    },
                    at: T0
                },
            ]
        );
    }

    #[test]
    fn bad_empty_and_help_lines_send_nothing_and_reading_goes_on() {
        let inputs = read_all(b"fly\n\n   \nhelp\nfault\nunplug\n");
        assert_eq!(
            inputs,
            [Input {
                command: Command::Unplug,
                at: T0
            }]
        );
    }

    #[test]
    fn a_line_without_a_final_newline_still_counts() {
        assert_eq!(read_all(b"clear").len(), 1);
    }

    #[test]
    fn a_line_that_is_not_utf8_is_skipped_and_reading_goes_on() {
        let inputs = read_all(b"plug\n\xff\xfe\nstop\n");
        let commands: Vec<Command> = inputs.into_iter().map(|i| i.command).collect();
        assert_eq!(commands, [Command::Plug, Command::Stop]);
    }

    /// A reader whose every read fails.
    struct Broken;

    impl io::Read for Broken {
        fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
            Err(io::Error::other("broken"))
        }
    }

    impl BufRead for Broken {
        fn fill_buf(&mut self) -> io::Result<&[u8]> {
            Err(io::Error::other("broken"))
        }
        fn consume(&mut self, _: usize) {}
    }

    #[test]
    fn a_read_error_ends_the_reader() {
        let (tx, _rx) = mpsc::channel(1);
        // Returns instead of looping forever.
        read_inputs(Broken, &tx, || T0);
    }

    /// A reader that offers the line `plug` `left` times, then end of file.
    struct Plugs {
        left: usize,
    }

    impl io::Read for Plugs {
        fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
            unreachable!("read_line goes through fill_buf")
        }
    }

    impl BufRead for Plugs {
        fn fill_buf(&mut self) -> io::Result<&[u8]> {
            Ok(if self.left == 0 { b"" } else { b"plug\n" })
        }
        fn consume(&mut self, amount: usize) {
            if amount > 0 {
                self.left -= 1;
            }
        }
    }

    #[test]
    fn a_dropped_receiver_ends_the_reader_after_the_first_line() {
        let (tx, rx) = mpsc::channel(1);
        drop(rx);
        let mut plugs = Plugs { left: 5 };
        read_inputs(&mut plugs, &tx, || T0);
        // One line was read and could not be sent; the reader stopped there.
        assert_eq!(plugs.left, 4);
    }

    #[test]
    fn simple_commands() {
        assert_eq!(parse("plug"), Ok(Command::Plug));
        assert_eq!(
            parse("start"),
            Ok(Command::Present {
                id_tag: "DEMO-TAG".into()
            })
        );
        assert_eq!(parse("stop"), Ok(Command::Stop));
        assert_eq!(parse("unplug"), Ok(Command::Unplug));
        assert_eq!(parse("clear"), Ok(Command::Clear));
    }

    #[test]
    fn case_and_whitespace_do_not_matter() {
        assert_eq!(parse("  PLUG \n"), Ok(Command::Plug));
        assert_eq!(
            parse("Start"),
            Ok(Command::Present {
                id_tag: "DEMO-TAG".into()
            })
        );
        assert_eq!(
            parse("fault   OverCurrent\t"),
            Ok(Command::Fault(Fault::OverCurrent))
        );
    }

    #[test]
    fn start_takes_an_id_tag_and_keeps_its_case() {
        assert_eq!(
            parse("START  Card-42 "),
            Ok(Command::Present {
                id_tag: "Card-42".into()
            })
        );
        assert_eq!(
            parse("start Карта-7"),
            Ok(Command::Present {
                id_tag: "Карта-7".into()
            })
        );
        // The length is checked by `session` (`transaction16::IdTag`), which
        // knows the limit of the OCPP version; the keyboard passes it on.
        let long = "A".repeat(21);
        assert_eq!(
            parse(&format!("start {long}")),
            Ok(Command::Present { id_tag: long })
        );
        assert_eq!(
            parse("start a b"),
            Err(ParseError::Unknown("start a b".into()))
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
            assert_eq!(parse(&format!("fault {word}")), Ok(Command::Fault(fault)));
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
