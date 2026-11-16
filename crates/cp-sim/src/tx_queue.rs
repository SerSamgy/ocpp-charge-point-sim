//! A queue of transaction-related messages and the rules for retrying them
//! (OCPP 1.6 sections 3.7 and 3.7.1).
//!
//! Messages leave in the order they were queued. When the CSMS fails to
//! process the message at the head (the caller decides what counts as a
//! failure), it is sent again after `retry_interval` times the number of
//! transmissions so far: with 3 attempts and 60 s, wait 60 s, then 120 s,
//! and after the third failure the message is discarded and the next one
//! goes. A message only waits at the head; everything behind it waits for
//! it, which keeps the order.
//!
//! Pure logic: no I/O, no clock (instants are passed in), generic over the
//! message, so OCPP 2.0.1 can use it for TransactionEvent as well.

use std::{collections::VecDeque, time::Duration};

use tokio::time::Instant;

#[derive(Debug)]
pub struct TxQueue<M> {
    entries: VecDeque<Entry<M>>,
    /// Transmissions of one message at most; at least 1.
    attempts: u32,
    retry_interval: Duration,
}

#[derive(Debug)]
struct Entry<M> {
    message: M,
    /// Transmissions that failed so far.
    transmissions: u32,
    /// When the message may go again after a failure; `None` until the
    /// first failure, so a new message goes at once.
    retry_at: Option<Instant>,
}

/// What became of the head after a failed transmission.
#[derive(Debug, PartialEq, Eq)]
pub enum Failure<M> {
    /// It stays at the head and goes again at this instant.
    RetryAt(Instant),
    /// It was the last attempt (or the wait would overflow the clock): the
    /// message left the queue after `transmissions` failures.
    Discarded { message: M, transmissions: u32 },
}

impl<M> TxQueue<M> {
    /// `attempts` = 0 is treated as 1: every message goes at least once.
    pub fn new(attempts: u32, retry_interval: Duration) -> Self {
        TxQueue {
            entries: VecDeque::new(),
            attempts: attempts.max(1),
            retry_interval,
        }
    }

    pub fn push(&mut self, message: M) {
        self.entries.push_back(Entry {
            message,
            transmissions: 0,
            retry_at: None,
        });
    }

    /// `push`, but keeps at most `limit` queued messages that `limited`
    /// selects (at least 1, so the newest one always waits): the oldest of
    /// them leave first. Returns the messages that were dropped. Messages
    /// that `limited` does not select are never dropped.
    pub fn push_limited(
        &mut self,
        message: M,
        limited: impl Fn(&M) -> bool,
        limit: usize,
    ) -> Vec<M> {
        let mut dropped = Vec::new();
        if limited(&message) {
            let limit = limit.max(1);
            while self.count(&limited) >= limit {
                match self.drop_first(&limited) {
                    Some(old) => dropped.push(old),
                    None => break,
                }
            }
        }
        self.push(message);
        dropped
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// How many queued messages `select` selects.
    pub fn count(&self, select: impl Fn(&M) -> bool) -> usize {
        self.entries.iter().filter(|e| select(&e.message)).count()
    }

    /// The head, if it may go at `now`.
    pub fn head_due(&self, now: Instant) -> Option<&M> {
        let head = self.entries.front()?;
        match head.retry_at {
            Some(retry_at) if retry_at > now => None,
            _ => Some(&head.message),
        }
    }

    /// When the head may go: `now` if it may go at once, its retry instant
    /// otherwise; `None` for an empty queue. `head_due` of this instant is
    /// always `Some`.
    pub fn next_due(&self, now: Instant) -> Option<Instant> {
        let head = self.entries.front()?;
        Some(head.retry_at.map_or(now, |retry_at| retry_at.max(now)))
    }

    /// The CSMS answered the head: it leaves the queue.
    pub fn delivered(&mut self) -> Option<M> {
        self.entries.pop_front().map(|e| e.message)
    }

    /// The head failed at `now`. `None` for an empty queue.
    pub fn failed(&mut self, now: Instant) -> Option<Failure<M>> {
        let head = self.entries.front_mut()?;
        head.transmissions = head.transmissions.saturating_add(1);
        let retry_at = if head.transmissions >= self.attempts {
            None
        } else {
            self.retry_interval
                .checked_mul(head.transmissions)
                .and_then(|wait| now.checked_add(wait))
        };
        match retry_at {
            Some(retry_at) => {
                head.retry_at = Some(retry_at);
                Some(Failure::RetryAt(retry_at))
            }
            None => {
                let entry = self.entries.pop_front()?;
                Some(Failure::Discarded {
                    message: entry.message,
                    transmissions: entry.transmissions,
                })
            }
        }
    }

    /// Removes the first queued message that `select` selects.
    fn drop_first(&mut self, select: impl Fn(&M) -> bool) -> Option<M> {
        let index = self.entries.iter().position(|e| select(&e.message))?;
        self.entries.remove(index).map(|e| e.message)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MINUTE: Duration = Duration::from_secs(60);

    fn queue(attempts: u32) -> TxQueue<&'static str> {
        TxQueue::new(attempts, MINUTE)
    }

    #[test]
    fn messages_leave_in_order() {
        let mut q = queue(3);
        let now = Instant::now();
        q.push("start");
        q.push("stop");
        assert_eq!(q.head_due(now), Some(&"start"));
        assert_eq!(q.delivered(), Some("start"));
        assert_eq!(q.head_due(now), Some(&"stop"));
        assert_eq!(q.delivered(), Some("stop"));
        assert!(q.is_empty());
        assert_eq!(q.next_due(now), None);
    }

    #[test]
    fn retries_wait_the_interval_times_the_transmissions_then_discard() {
        // The example of section 3.7.1: 3 attempts, 60 s.
        let mut q = queue(3);
        let t0 = Instant::now();
        q.push("stop");
        q.push("next");

        assert_eq!(q.failed(t0), Some(Failure::RetryAt(t0 + MINUTE)));
        assert_eq!(q.head_due(t0 + MINUTE - Duration::from_millis(1)), None);
        let t1 = t0 + MINUTE;
        assert_eq!(q.head_due(t1), Some(&"stop"));

        assert_eq!(q.failed(t1), Some(Failure::RetryAt(t1 + 2 * MINUTE)));
        let t2 = t1 + 2 * MINUTE;
        assert_eq!(q.head_due(t2), Some(&"stop"));

        assert_eq!(
            q.failed(t2),
            Some(Failure::Discarded {
                message: "stop",
                transmissions: 3
            })
        );
        // The next message goes at once: it has not failed yet.
        assert_eq!(q.head_due(t2), Some(&"next"));
    }

    #[test]
    fn a_message_behind_a_waiting_head_waits_too() {
        let mut q = queue(3);
        let t0 = Instant::now();
        q.push("stop");
        q.failed(t0);
        q.push("sample");
        assert_eq!(q.head_due(t0), None);
        assert_eq!(q.next_due(t0), Some(t0 + MINUTE));
    }

    #[test]
    fn zero_attempts_send_once() {
        let mut q = queue(0);
        let now = Instant::now();
        q.push("stop");
        assert_eq!(
            q.failed(now),
            Some(Failure::Discarded {
                message: "stop",
                transmissions: 1
            })
        );
    }

    #[test]
    fn a_wait_that_overflows_the_clock_discards() {
        let mut q = TxQueue::new(3, Duration::MAX);
        let now = Instant::now();
        q.push("stop");
        assert!(matches!(
            q.failed(now),
            Some(Failure::Discarded {
                message: "stop",
                ..
            })
        ));
    }

    #[test]
    fn the_head_is_always_due_at_next_due() {
        let mut q = queue(5);
        let t0 = Instant::now();
        assert_eq!(q.next_due(t0), None);
        q.push("stop");
        // A new head: due now.
        let due = q.next_due(t0).unwrap();
        assert_eq!(due, t0);
        assert!(q.head_due(due).is_some());
        // A failed head: due at its retry instant, and not a moment before.
        for n in 1..4u32 {
            let now = q.next_due(t0).unwrap();
            q.failed(now);
            let due = q.next_due(now).unwrap();
            assert_eq!(due, now + MINUTE * n);
            assert!(q.head_due(due).is_some());
        }
        // Asked later than the retry instant, it is due at once.
        let late = q.next_due(t0).unwrap() + MINUTE * 100;
        assert_eq!(q.next_due(late), Some(late));
    }

    #[test]
    fn failed_and_delivered_on_an_empty_queue_do_nothing() {
        let mut q = queue(3);
        assert_eq!(q.failed(Instant::now()), None);
        assert_eq!(q.delivered(), None);
    }

    fn is_sample(message: &&str) -> bool {
        message.starts_with("sample")
    }

    #[test]
    fn the_limit_drops_the_oldest_limited_messages_only() {
        let mut q = queue(3);
        q.push("stop");
        assert!(q.push_limited("sample 1", is_sample, 2).is_empty());
        assert!(q.push_limited("sample 2", is_sample, 2).is_empty());
        assert_eq!(q.push_limited("sample 3", is_sample, 2), ["sample 1"]);
        // A message the limit does not select is never dropped.
        assert!(q.push_limited("stop 2", is_sample, 2).is_empty());
        assert_eq!(q.len(), 4);
        let order: Vec<_> = std::iter::from_fn(|| q.delivered()).collect();
        assert_eq!(order, ["stop", "sample 2", "sample 3", "stop 2"]);
    }

    #[test]
    fn a_limit_of_zero_keeps_the_newest() {
        let mut q = queue(3);
        assert!(q.push_limited("sample 1", is_sample, 0).is_empty());
        assert_eq!(q.push_limited("sample 2", is_sample, 0), ["sample 1"]);
        assert_eq!(q.count(is_sample), 1);
        assert_eq!(q.head_due(Instant::now()), Some(&"sample 2"));
    }
}
