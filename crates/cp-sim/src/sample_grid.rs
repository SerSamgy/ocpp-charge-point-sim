//! The points in time at which a transaction samples the meter.
//!
//! OCPP 1.6 section 3.16.1 (errata 3.15): samples are taken periodically
//! "from the start of the charging transaction". The points therefore lie on
//! a grid in UTC, `start + k·period` for k = 1, 2, 3, ..., fixed by the start
//! of the transaction alone. A point does not depend on when the station gets
//! round to sampling it, so a station that was busy or offline can still
//! produce every sample it owes, each with the time and the meter reading of
//! its own point (section 3.7: when offline, queue what would have been sent).
//!
//! Pure logic: no I/O, no clock (instants are passed in). Every point is
//! handed out exactly once: `take_before` and `take_up_to` move a cursor
//! forward, so calling them one after the other with growing limits (the
//! replay of the inputs that waited offline, then the live sampler) leaves no
//! gap and repeats nothing.
//!
//! The same grid serves OCPP 2.0.1, where it paces TransactionEvent
//! (Updated).

use std::time::Duration;

use time::UtcDateTime;

#[cfg_attr(
    not(test),
    expect(dead_code, reason = "used by session in the next commit")
)]
const NANOS_PER_SECOND: u128 = 1_000_000_000;

/// The sample points of one transaction.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(
    not(test),
    expect(dead_code, reason = "used by session in the next commit")
)]
pub struct SampleGrid {
    start: UtcDateTime,
    /// Never zero.
    period: Duration,
    /// How many points have been handed out: points 1 to `taken`.
    taken: u64,
}

/// Points handed out by one call.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(
    not(test),
    expect(dead_code, reason = "used by session in the next commit")
)]
pub struct Taken {
    /// Due points that were not computed because more than `keep_last` were
    /// due. They were the oldest ones.
    pub skipped: u64,
    /// The newest due points, oldest first, at most `keep_last`.
    pub points: Vec<UtcDateTime>,
}

impl SampleGrid {
    /// A grid for a transaction that starts at `start`. `None` if sampling is
    /// off (a zero period) or if not even the first point exists (the date
    /// after `start + period` is out of range).
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "used by session in the next commit")
    )]
    pub fn new(start: UtcDateTime, period: Duration) -> Option<Self> {
        if period.is_zero() {
            return None;
        }
        let grid = SampleGrid {
            start,
            period,
            taken: 0,
        };
        grid.point(1)?;
        Some(grid)
    }

    /// The next point that has not been handed out; `None` once the dates
    /// run out.
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "used by session in the next commit")
    )]
    pub fn next_point(&self) -> Option<UtcDateTime> {
        self.point(self.taken.checked_add(1)?)
    }

    /// Hands out the points strictly earlier than `until` that were not
    /// handed out yet. A point exactly at `until` stays: an event at that
    /// instant is applied first.
    ///
    /// At most `keep_last` points are returned (0 counts as 1); when more are
    /// due, the oldest are only counted, without computing them, so a long
    /// gap costs no more than a short one.
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "used by session in the next commit")
    )]
    pub fn take_before(&mut self, until: UtcDateTime, keep_last: usize) -> Taken {
        self.take(until, false, keep_last)
    }

    /// Like `take_before`, but a point exactly at `until` is due as well.
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "used by session in the next commit")
    )]
    pub fn take_up_to(&mut self, until: UtcDateTime, keep_last: usize) -> Taken {
        self.take(until, true, keep_last)
    }

    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "used by session in the next commit")
    )]
    fn take(&mut self, until: UtcDateTime, inclusive: bool, keep_last: usize) -> Taken {
        let keep_last = u64::try_from(keep_last).unwrap_or(u64::MAX).max(1);
        let due = self.last_due(until, inclusive);
        let count = due.saturating_sub(self.taken);
        if count == 0 {
            return Taken {
                skipped: 0,
                points: Vec::new(),
            };
        }
        let skipped = count.saturating_sub(keep_last);
        let first = self.taken + skipped + 1;
        // Every `k` up to `due` lies before `until`, a valid date, so each
        // point exists; `map_while` just never panics if one did not.
        let points = (first..=due).map_while(|k| self.point(k)).collect();
        self.taken = due;
        Taken { skipped, points }
    }

    /// The number of the last point before `until` (or at it, if
    /// `inclusive`); 0 if there is none.
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "used by session in the next commit")
    )]
    fn last_due(&self, until: UtcDateTime, inclusive: bool) -> u64 {
        let elapsed = (until - self.start).whole_nanoseconds();
        let Ok(elapsed) = u128::try_from(elapsed) else {
            return 0;
        };
        // Points k ≥ 1 with k·period ≤ elapsed, or < elapsed.
        let reach = if inclusive {
            elapsed
        } else {
            match elapsed.checked_sub(1) {
                Some(reach) => reach,
                None => return 0,
            }
        };
        u64::try_from(reach / self.period.as_nanos()).unwrap_or(u64::MAX)
    }

    /// Point number `k`: `start + k·period`, computed by multiplication in
    /// nanoseconds, so nothing adds up over thousands of points and `k` may
    /// pass 2^32. `None` if the date is out of range.
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "used by session in the next commit")
    )]
    fn point(&self, k: u64) -> Option<UtcDateTime> {
        let nanos = self.period.as_nanos().checked_mul(u128::from(k))?;
        let seconds = i64::try_from(nanos / NANOS_PER_SECOND).ok()?;
        let sub = i32::try_from(nanos % NANOS_PER_SECOND).ok()?;
        self.start.checked_add(time::Duration::new(seconds, sub))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PERIOD: Duration = Duration::from_secs(10);

    fn at(seconds: i64) -> UtcDateTime {
        UtcDateTime::from_unix_timestamp(seconds).unwrap()
    }

    fn grid() -> SampleGrid {
        SampleGrid::new(at(1_000), PERIOD).unwrap()
    }

    fn unix(taken: &Taken) -> Vec<i64> {
        taken.points.iter().map(|p| p.unix_timestamp()).collect()
    }

    #[test]
    fn the_first_point_is_one_period_after_the_start() {
        assert_eq!(grid().next_point(), Some(at(1_010)));
    }

    #[test]
    fn no_grid_for_a_zero_period_or_one_that_leaves_the_calendar() {
        assert_eq!(SampleGrid::new(at(0), Duration::ZERO), None);
        assert_eq!(SampleGrid::new(at(0), Duration::MAX), None);
        // The first point itself must exist.
        assert_eq!(
            SampleGrid::new(UtcDateTime::MAX, Duration::from_secs(1)),
            None
        );
    }

    #[test]
    fn a_point_exactly_at_the_limit_belongs_to_take_up_to_only() {
        let mut before = grid();
        assert_eq!(unix(&before.take_before(at(1_030), 100)), [1_010, 1_020]);
        let mut up_to = grid();
        assert_eq!(
            unix(&up_to.take_up_to(at(1_030), 100)),
            [1_010, 1_020, 1_030]
        );
    }

    #[test]
    fn nothing_is_due_before_the_first_point_or_before_the_start() {
        let mut grid = grid();
        assert!(grid.take_up_to(at(1_009), 100).points.is_empty());
        assert!(grid.take_up_to(at(500), 100).points.is_empty());
        // The start itself is not a point.
        assert!(grid.take_up_to(at(1_000), 100).points.is_empty());
        assert_eq!(grid.next_point(), Some(at(1_010)));
    }

    #[test]
    fn every_point_is_handed_out_once_across_alternating_calls() {
        let mut grid = grid();
        let mut all = Vec::new();
        // The replay of waiting inputs (strict), then the live sampler,
        // again and again: no point twice and none missing.
        for (limit, strict) in [(1_025, true), (1_025, false), (1_040, true), (1_075, false)] {
            let taken = if strict {
                grid.take_before(at(limit), 100)
            } else {
                grid.take_up_to(at(limit), 100)
            };
            all.extend(unix(&taken));
        }
        assert_eq!(all, [1_010, 1_020, 1_030, 1_040, 1_050, 1_060, 1_070]);
        assert_eq!(grid.next_point(), Some(at(1_080)));
    }

    #[test]
    fn sub_second_periods_keep_their_fractions() {
        let mut grid = SampleGrid::new(at(0), Duration::from_millis(250)).unwrap();
        let taken = grid.take_up_to(at(1), 100);
        assert_eq!(taken.points.len(), 4);
        assert_eq!(taken.points[3], at(1));
        assert_eq!(taken.points[0], at(0) + time::Duration::milliseconds(250));
    }

    #[test]
    fn points_are_multiples_of_the_period_without_accumulated_error() {
        // A period that does not divide a second: 1/3 s. Adding it a million
        // times would drift; multiplying does not.
        let period = Duration::from_nanos(333_333_333);
        let mut grid = SampleGrid::new(at(0), period).unwrap();
        let taken = grid.take_up_to(at(1_000_000), 1);
        let n: i64 = 1_000_000_000_000_000 / 333_333_333;
        assert_eq!(taken.skipped, u64::try_from(n).unwrap() - 1);
        let expected = at(0) + time::Duration::nanoseconds(333_333_333 * n);
        assert_eq!(taken.points, [expected]);
    }

    #[test]
    fn the_point_number_may_pass_u32() {
        // 1 ms for 60 days is 5.2 billion points, more than u32 holds.
        let mut grid = SampleGrid::new(at(0), Duration::from_millis(1)).unwrap();
        let sixty_days = 60 * 24 * 3_600;
        let taken = grid.take_up_to(at(sixty_days), 2);
        let all = u64::try_from(sixty_days).unwrap() * 1_000;
        assert!(all > u64::from(u32::MAX));
        assert_eq!(taken.skipped, all - 2);
        assert_eq!(taken.points.len(), 2);
        assert_eq!(taken.points[1], at(sixty_days));
    }

    #[test]
    fn a_huge_gap_returns_only_the_newest_points_and_counts_the_rest() {
        let mut grid = grid();
        // 100 years at 10 s: 315 million points, computed in no time.
        let until = at(1_000 + 100 * 365 * 86_400);
        let taken = grid.take_up_to(until, 3);
        assert_eq!(taken.points.len(), 3);
        assert_eq!(taken.points[2], until);
        assert_eq!(taken.skipped, 100 * 365 * 86_400 / 10 - 3);
        // The cursor moved past all of them: nothing is due again.
        assert!(grid.take_up_to(until, 3).points.is_empty());
        assert_eq!(grid.next_point(), Some(until + time::Duration::seconds(10)));
    }

    #[test]
    fn keep_last_zero_counts_as_one() {
        let mut grid = grid();
        let taken = grid.take_up_to(at(1_050), 0);
        assert_eq!(unix(&taken), [1_050]);
        assert_eq!(taken.skipped, 4);
    }

    #[test]
    fn the_grid_ends_when_the_dates_run_out() {
        let start = UtcDateTime::MAX - time::Duration::seconds(25);
        let mut grid = SampleGrid::new(start, PERIOD).unwrap();
        assert_eq!(grid.take_up_to(UtcDateTime::MAX, 100).points.len(), 2);
        // Point 3 would lie after the last representable instant.
        assert_eq!(grid.next_point(), None);
        assert!(grid.take_up_to(UtcDateTime::MAX, 100).points.is_empty());
    }
}
