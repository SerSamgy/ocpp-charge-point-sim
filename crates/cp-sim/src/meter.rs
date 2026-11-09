//! A simulated energy meter: what the station would read from a real
//! electricity meter between the grid and the connector.
//!
//! Pure logic: no I/O, no clock. The meter is a register that only grows,
//! like the counter of a real meter, and it keeps growing across
//! transactions: a transaction reports the register at its start
//! (`meterStart`) and at its end (`meterStop`), and the CSMS bills the
//! difference. Energy flows only while the connector is Charging; `session`
//! calls `start` when the connector enters Charging and `stop` when it
//! leaves it (to Finishing, Available or Faulted).
//!
//! The power is a constant, `POWER_W`: a typical home AC station, one phase
//! at 230 V and 32 A, which is about 7.4 kW. A real meter (or a current and
//! voltage sensor on the Pi) would replace the constant later.
//!
//! The register counts watt-seconds (joules), not watt-hours, so pausing
//! and resuming does not lose the fraction of a watt-hour each time;
//! `reading` rounds down to whole watt-hours, the unit OCPP 1.6 reports by
//! default (section 4.7: a value without a unit is Wh).

use time::UtcDateTime;

/// The constant charging power of the simulation, in watts.
pub const POWER_W: u64 = 7400;

const SECONDS_PER_HOUR: u64 = 3600;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Meter {
    /// Energy of the finished charging periods, in watt-seconds.
    register_ws: u64,
    /// Start of the charging period in progress, if any.
    charging_since: Option<UtcDateTime>,
}

impl Meter {
    /// A meter that reads 0 Wh and is not charging.
    pub fn new() -> Self {
        Meter::default()
    }

    /// Energy starts to flow at `at`. Does nothing if it already flows, so
    /// the period that is running is not restarted.
    pub fn start(&mut self, at: UtcDateTime) {
        if self.charging_since.is_none() {
            self.charging_since = Some(at);
        }
    }

    /// Energy stops flowing at `at`; the period is added to the register.
    /// Does nothing if no energy flows.
    pub fn stop(&mut self, at: UtcDateTime) {
        if let Some(since) = self.charging_since.take() {
            self.register_ws = self.register_ws.saturating_add(energy_ws(since, at));
        }
    }

    /// The register at `at`, in whole watt-hours, rounded down.
    pub fn reading(&self, at: UtcDateTime) -> u64 {
        let running = self.charging_since.map_or(0, |since| energy_ws(since, at));
        self.register_ws.saturating_add(running) / SECONDS_PER_HOUR
    }
}

/// Energy at `POWER_W` from `since` to `at`, in watt-seconds. A period that
/// runs backwards (the system clock was set back) counts as zero: a meter
/// never runs backwards.
fn energy_ws(since: UtcDateTime, at: UtcDateTime) -> u64 {
    let seconds = (at - since).whole_seconds();
    let seconds = u64::try_from(seconds).unwrap_or(0);
    POWER_W.saturating_mul(seconds)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(seconds: i64) -> UtcDateTime {
        UtcDateTime::from_unix_timestamp(seconds).unwrap()
    }

    #[test]
    fn reads_zero_before_any_charging() {
        let meter = Meter::new();
        assert_eq!(meter.reading(at(0)), 0);
        assert_eq!(meter.reading(at(1_000_000)), 0);
    }

    #[test]
    fn grows_linearly_while_charging() {
        let mut meter = Meter::new();
        meter.start(at(0));
        // 7400 W for one hour is 7400 Wh.
        assert_eq!(meter.reading(at(3600)), 7400);
        // 7400 W · 120 s / 3600 = 246.67 Wh, rounded down.
        assert_eq!(meter.reading(at(120)), 246);
    }

    #[test]
    fn stop_freezes_the_register() {
        let mut meter = Meter::new();
        meter.start(at(0));
        meter.stop(at(120));
        assert_eq!(meter.reading(at(120)), 246);
        assert_eq!(meter.reading(at(10_000)), 246);
    }

    #[test]
    fn a_pause_adds_nothing_and_fractions_are_kept() {
        // Four periods of 30 s with long pauses between them: 120 s of
        // charging, 246.67 Wh, so 246. Each period is 61.67 Wh; a register
        // in whole Wh would round each one down and end at 4 · 61 = 244.
        let mut meter = Meter::new();
        for period in 0..4 {
            let begin = period * 1000;
            meter.start(at(begin));
            meter.stop(at(begin + 30));
        }
        assert_eq!(meter.reading(at(5000)), 246);
    }

    #[test]
    fn the_register_keeps_growing_across_transactions() {
        let mut meter = Meter::new();
        meter.start(at(0));
        meter.stop(at(3600));
        let second_start = meter.reading(at(4000));
        meter.start(at(4000));
        meter.stop(at(7600));
        assert_eq!(second_start, 7400);
        assert_eq!(meter.reading(at(8000)), 14_800);
    }

    #[test]
    fn start_while_charging_does_not_restart_the_period() {
        let mut meter = Meter::new();
        meter.start(at(0));
        meter.start(at(3000));
        assert_eq!(meter.reading(at(3600)), 7400);
    }

    #[test]
    fn stop_without_charging_does_nothing() {
        let mut meter = Meter::new();
        meter.stop(at(100));
        assert_eq!(meter, Meter::new());
    }

    #[test]
    fn a_clock_set_back_counts_as_zero() {
        let mut meter = Meter::new();
        meter.start(at(1000));
        assert_eq!(meter.reading(at(0)), 0);
        meter.stop(at(0));
        assert_eq!(meter.reading(at(0)), 0);
    }

    #[test]
    fn huge_periods_saturate_instead_of_panicking() {
        // The longest period, year -9999 to 9999, is about 4.7e15 Ws; a few
        // thousand of them overflow a u64 (1.8e19).
        let mut meter = Meter::new();
        for _ in 0..5000 {
            meter.start(UtcDateTime::MIN);
            meter.stop(UtcDateTime::MAX);
        }
        let full = u64::MAX / SECONDS_PER_HOUR;
        assert_eq!(meter.reading(UtcDateTime::MAX), full);
        // A running period on top of a full register saturates as well.
        meter.start(UtcDateTime::MIN);
        assert_eq!(meter.reading(UtcDateTime::MAX), full);
    }
}
