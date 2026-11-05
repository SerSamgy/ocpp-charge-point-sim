//! Log setup: one `tracing` subscriber that writes events to stderr.
//!
//! The filter comes from `RUST_LOG` (e.g. `RUST_LOG=cp_sim=info` or
//! `RUST_LOG=warn`). Without it the program logs everything of its own, down
//! to the frames on the wire, as it did before `tracing`. Events of other
//! crates are off by default; `tokio-tungstenite` logs through the `log`
//! crate, which is not bridged.

use std::{
    env,
    io::{self, IsTerminal},
};

use tracing_subscriber::{EnvFilter, filter::ParseError};

use crate::client::BoxError;

/// Filter used when `RUST_LOG` is not set.
const DEFAULT_FILTER: &str = "cp_sim=debug";

/// Builds the filter from the value of `RUST_LOG`. Unset and blank mean the
/// default; a value that does not parse is an error, so that a typo does not
/// silently turn the log off.
fn env_filter(rust_log: Option<&str>) -> Result<EnvFilter, ParseError> {
    match rust_log.map(str::trim) {
        None | Some("") => EnvFilter::try_new(DEFAULT_FILTER),
        Some(directives) => EnvFilter::try_new(directives),
    }
}

/// Installs the global subscriber. Call it first in `main`: events emitted
/// before there is a subscriber are dropped without a trace. Colours only
/// when stderr is a terminal, so a file or journald gets plain text.
pub fn init() -> Result<(), BoxError> {
    let rust_log = match env::var("RUST_LOG") {
        Ok(value) => Some(value),
        Err(env::VarError::NotPresent) => None,
        Err(err) => return Err(format!("RUST_LOG: {err}").into()),
    };
    let filter = env_filter(rust_log.as_deref())?;
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(io::stderr)
        .with_ansi(io::stderr().is_terminal())
        .try_init()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::LogCapture;

    fn logged_with(rust_log: Option<&str>) -> String {
        let capture = LogCapture::start(env_filter(rust_log).unwrap());
        tracing::debug!(direction = "out", "frame");
        tracing::warn!("trouble");
        capture.text()
    }

    #[test]
    fn default_filter_logs_debug_of_this_crate() {
        let text = logged_with(None);
        assert!(text.contains("frame"), "{text}");
        assert!(text.contains("direction=\"out\""), "{text}");
        assert!(text.contains("trouble"), "{text}");
    }

    #[test]
    fn blank_value_means_the_default() {
        assert!(logged_with(Some("  ")).contains("frame"));
    }

    #[test]
    fn rust_log_overrides_the_default() {
        let text = logged_with(Some("warn"));
        assert!(!text.contains("frame"), "{text}");
        assert!(text.contains("trouble"), "{text}");
    }

    #[test]
    fn malformed_directives_are_an_error() {
        assert!(env_filter(Some("cp_sim=loud")).is_err());
    }
}
