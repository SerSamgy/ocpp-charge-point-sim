//! Outstanding CALLs waiting for their CALLRESULT / CALLERROR, keyed by message ID.

use std::collections::HashMap;
use std::sync::Mutex;

use tokio::sync::oneshot;

use crate::frame::Frame;

#[derive(Default)]
pub struct PendingCalls {
    inner: Mutex<HashMap<String, oneshot::Sender<Frame>>>,
}

impl PendingCalls {
    /// Registers a CALL we are about to send.
    /// The returned receiver resolves with the response frame.
    pub fn register(&self, id: String) -> oneshot::Receiver<Frame> {
        let (tx, rx) = oneshot::channel();
        if self.inner.lock().unwrap().insert(id.clone(), tx).is_some() {
            eprintln!("duplicate pending CALL id={id}; previous waiter dropped")
        }
        rx
    }

    /// Hands a CALLRESULT / CALLERROR to whoever is waiting for its ID.
    /// Returns the frame back if nobody takes it: it's a CALL,
    /// the ID is unknown, or the caller has already given up.
    pub fn complete(&self, frame: Frame) -> Result<(), Frame> {
        if !matches!(frame, Frame::CallResult { .. } | Frame::CallError { .. }) {
            return Err(frame);
        }

        let Some(tx) = self.inner.lock().unwrap().remove(frame.id()) else {
            return Err(frame);
        };
        tx.send(frame)
    }

    /// Forgets a CALL that will never be answered (timeout, send failure).
    pub fn cancel(&self, id: &str) {
        self.inner.lock().unwrap().remove(id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn result(id: &str) -> Frame {
        Frame::CallResult {
            id: id.into(),
            payload: json!({}),
        }
    }

    fn error(id: &str) -> Frame {
        Frame::CallError {
            id: id.into(),
            code: "NotImplemented".into(),
            description: "Requested Action is not known by receiver".into(),
            details: json!({}),
        }
    }

    #[tokio::test]
    async fn delivers_result_to_waiting_caller() {
        let pending = PendingCalls::default();
        let rx = pending.register("1".into());

        assert!(pending.complete(result("1")).is_ok());
        assert_eq!(rx.await.unwrap(), result("1"));
    }

    #[tokio::test]
    async fn delivers_error_to_waiting_caller() {
        let pending = PendingCalls::default();
        let rx = pending.register("1".into());

        assert!(pending.complete(error("1")).is_ok());
        assert_eq!(rx.await.unwrap(), error("1"));
    }

    #[tokio::test]
    async fn unknown_id_returns_frame_unchanged() {
        let pending = PendingCalls::default();
        let _rx = pending.register("1".into());

        assert_eq!(pending.complete(result("2")), Err(result("2")));
    }

    #[tokio::test]
    async fn call_frame_is_not_a_response() {
        let pending = PendingCalls::default();
        let _rx = pending.register("1".into());
        let call = Frame::Call {
            id: "1".into(),
            action: "Reset".into(),
            payload: json!({"type": "Soft"}),
        };

        assert_eq!(pending.complete(call.clone()), Err(call));
        // The CALL must not consume the entry: the real response still gets through.
        assert!(pending.complete(result("1")).is_ok());
    }

    #[tokio::test]
    async fn cancelled_call_is_not_completed() {
        let pending = PendingCalls::default();
        let rx = pending.register("1".into());
        pending.cancel("1");

        assert_eq!(pending.complete(result("1")), Err(result("1")));
        // cancel() dropped the sender, so the waiter learns there will be no answer.
        assert!(rx.await.is_err());
    }

    #[tokio::test]
    async fn late_response_after_caller_gave_up() {
        let pending = PendingCalls::default();
        let rx = pending.register("1".into());
        drop(rx);

        assert_eq!(pending.complete(result("1")), Err(result("1")));
    }

    #[tokio::test]
    async fn second_response_with_same_id_is_rejected() {
        let pending = PendingCalls::default();
        let _rx = pending.register("1".into());

        assert!(pending.complete(result("1")).is_ok());
        assert_eq!(pending.complete(result("1")), Err(result("1")));
    }

    #[tokio::test]
    async fn duplicate_registration_drops_previous_waiter() {
        let pending = PendingCalls::default();
        let first = pending.register("1".into());
        let second = pending.register("1".into());

        assert!(first.await.is_err());
        assert!(pending.complete(result("1")).is_ok());
        assert_eq!(second.await.unwrap(), result("1"));
    }
}
