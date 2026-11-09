//! Time values on the wire.
//!
//! OCPP 1.6 (section 3.15) asks for ISO 8601 and OCPP 2.0.1 (part 2,
//! section 3.1) for RFC 3339 with at most three decimal places and
//! recommends omitting fractions. Whole seconds in UTC with a `Z` satisfy
//! both, so nothing here depends on the OCPP version.
//!
//! The source of time is the system clock. On a Raspberry Pi without a
//! battery-backed RTC it can be far off until NTP has synchronised, and a
//! clock corrected backwards can make two timestamps run out of order. The
//! CSMS trusts a `timestamp` it receives, so this matters once the client
//! sends them (OCPP 1.6 section 4.2 recommends syncing with `currentTime`
//! of BootNotification.conf; see the roadmap backlog).

use serde::{Serializer, ser::Error};
use time::{UtcDateTime, format_description::well_known::Rfc3339};

/// `at` as RFC 3339 in UTC, without a fraction: `2026-11-05T12:00:00Z`.
///
/// The fraction is dropped, not rounded, so the text never shows a later
/// second than `at` is in. `None` if `at` cannot be written (the `Result`s
/// of the `time` API); today every `UtcDateTime` can, because years above
/// 9999 need the crate feature `large-dates`.
pub fn rfc3339(at: UtcDateTime) -> Option<String> {
    UtcDateTime::from_unix_timestamp(at.unix_timestamp())
        .ok()?
        .format(&Rfc3339)
        .ok()
}

/// `rfc3339` for a required field: `#[serde(serialize_with = "timestamp::serialize")]`.
/// An instant that cannot be written fails the serialization, which
/// `Client::call` reports as `CallFailure::Encode`, instead of sending a
/// message without its required time.
pub fn serialize<S: Serializer>(at: &UtcDateTime, serializer: S) -> Result<S::Ok, S::Error> {
    match rfc3339(*at) {
        Some(text) => serializer.serialize_str(&text),
        None => Err(S::Error::custom(format!(
            "{at} cannot be written as RFC 3339"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn epoch_is_formatted_with_z() {
        assert_eq!(
            rfc3339(UtcDateTime::UNIX_EPOCH).as_deref(),
            Some("1970-01-01T00:00:00Z")
        );
    }

    #[test]
    fn fraction_is_dropped_not_rounded() {
        let almost_next_second = UtcDateTime::UNIX_EPOCH
            .replace_nanosecond(999_999_999)
            .unwrap();
        assert_eq!(
            rfc3339(almost_next_second).as_deref(),
            Some("1970-01-01T00:00:00Z")
        );
    }

    #[test]
    fn a_known_instant() {
        let at = UtcDateTime::from_unix_timestamp(1_793_880_000).unwrap();
        assert_eq!(rfc3339(at).as_deref(), Some("2026-11-05T12:00:00Z"));
    }

    #[test]
    fn max_instant_formats_without_panic() {
        assert_eq!(
            rfc3339(UtcDateTime::MAX).as_deref(),
            Some("9999-12-31T23:59:59Z")
        );
    }
}
