//! How long to wait before the next connection attempt.
//!
//! The algorithm is taken from OCPP-J 2.0.1 (part 4, section 5.4). OCPP 1.6
//! does not require it: OCPP-J 1.6 only asks for "an appropriate back-off
//! mechanism" (errata sheet, 3.4). It belongs to the transport, not to a
//! message set, so both versions use it.

use std::time::Duration;

use rand::{Rng, RngExt};

/// The three OCPPCommCtrlr variables of OCPP-J 2.0.1, section 8.
/// The spec gives no defaults; the caller picks them.
#[derive(Debug, Clone, Copy)]
pub struct BackoffConfig {
    /// `RetryBackOffWaitMinimum`: the wait before the first attempt.
    pub wait_minimum: Duration,
    /// `RetryBackOffRandomRange`: upper bound of the random part.
    pub random_range: Duration,
    /// `RetryBackOffRepeatTimes`: how many times the wait doubles.
    pub repeat_times: u32,
}

/// Waits grow as `wait_minimum · 2^k` with `k` capped at `repeat_times`,
/// plus a fresh random value in `0..=random_range` each time.
///
/// Section 5.4 says to "double the previous back-off time". We double only
/// the fixed part: doubling the random part as well would let the
/// randomness pile up with every attempt.
#[derive(Debug)]
pub struct Backoff {
    config: BackoffConfig,
    /// Waits handed out since the last `reset`.
    failures: u32,
}

impl Backoff {
    pub fn new(config: BackoffConfig) -> Self {
        Backoff {
            config,
            failures: 0,
        }
    }

    /// The wait before the next attempt; every call counts as one more
    /// failed attempt.
    pub fn next_delay<R: Rng + ?Sized>(&mut self, rng: &mut R) -> Duration {
        let delay = self.base_delay().saturating_add(self.jitter(rng));
        self.failures = self.failures.saturating_add(1);
        delay
    }

    /// Back to `wait_minimum`: the connection works again.
    pub fn reset(&mut self) {
        self.failures = 0;
    }

    fn base_delay(&self) -> Duration {
        let doublings = self.failures.min(self.config.repeat_times);
        // 2^32 and more does not fit into u32; such waits are absurd anyway.
        let factor = 2u32.checked_pow(doublings).unwrap_or(u32::MAX);
        self.config.wait_minimum.saturating_mul(factor)
    }

    fn jitter<R: Rng + ?Sized>(&self, rng: &mut R) -> Duration {
        let max_ms = u64::try_from(self.config.random_range.as_millis()).unwrap_or(u64::MAX);
        Duration::from_millis(rng.random_range(0..=max_ms))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::{SeedableRng, rngs::StdRng};

    const MIN: Duration = Duration::from_secs(3);

    fn backoff(random_range: Duration, repeat_times: u32) -> Backoff {
        Backoff::new(BackoffConfig {
            wait_minimum: MIN,
            random_range,
            repeat_times,
        })
    }

    fn delays(backoff: &mut Backoff, n: usize) -> Vec<Duration> {
        let mut rng = StdRng::seed_from_u64(1);
        (0..n).map(|_| backoff.next_delay(&mut rng)).collect()
    }

    fn secs(values: &[u64]) -> Vec<Duration> {
        values.iter().map(|&s| Duration::from_secs(s)).collect()
    }

    #[test]
    fn doubles_up_to_repeat_times_then_stays() {
        let mut backoff = backoff(Duration::ZERO, 3);
        assert_eq!(delays(&mut backoff, 6), secs(&[3, 6, 12, 24, 24, 24]));
    }

    #[test]
    fn zero_repeat_times_always_waits_the_minimum() {
        let mut backoff = backoff(Duration::ZERO, 0);
        assert_eq!(delays(&mut backoff, 3), secs(&[3, 3, 3]));
    }

    #[test]
    fn reset_starts_again_from_the_minimum() {
        let mut backoff = backoff(Duration::ZERO, 3);
        delays(&mut backoff, 4);
        backoff.reset();
        assert_eq!(delays(&mut backoff, 2), secs(&[3, 6]));
    }

    #[test]
    fn jitter_stays_within_random_range() {
        let range = Duration::from_secs(2);
        let mut backoff = backoff(range, 0);
        let samples = delays(&mut backoff, 200);

        for delay in &samples {
            assert!(
                (MIN..=MIN + range).contains(delay),
                "{delay:?} outside {MIN:?}..={:?}",
                MIN + range
            );
        }
        assert!(
            samples.iter().any(|d| *d != samples[0]),
            "jitter never changed the delay"
        );
    }

    #[test]
    fn huge_settings_saturate_instead_of_panicking() {
        let mut backoff = Backoff::new(BackoffConfig {
            wait_minimum: Duration::MAX,
            random_range: Duration::MAX,
            repeat_times: u32::MAX,
        });
        let mut rng = StdRng::seed_from_u64(1);
        for _ in 0..40 {
            backoff.next_delay(&mut rng);
        }
    }
}
