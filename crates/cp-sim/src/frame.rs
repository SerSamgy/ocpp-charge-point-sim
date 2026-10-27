//! OCPP-J RPC framing: the outer JSON array around every message.
//! Identical for OCPP 1.6 and 2.0.1.

use serde::{Deserialize, Deserializer, Serialize, Serializer, de};
use serde_json::Value;

const CALL: u64 = 2;
const CALL_RESULT: u64 = 3;
const CALL_ERROR: u64 = 4;

#[derive(Debug, Clone, PartialEq)]
pub enum Frame {
    /// [2, "<id>", "<action>", {payload}]
    Call {
        id: String,
        action: String,
        payload: Value,
    },
    /// [3, "<id>", {payload}]
    CallResult { id: String, payload: Value },
    /// [4, "<id>", "<errorCode>", "<errorDescription>", {errorDetails}]
    CallError {
        id: String,
        code: String,
        description: String,
        details: Value,
    },
}

impl Frame {
    /// Message ID, used to match a response to its request.
    pub fn id(&self) -> &str {
        match self {
            Frame::Call { id, .. } => id,
            Frame::CallResult { id, .. } => id,
            Frame::CallError { id, .. } => id,
        }
    }
}

impl Serialize for Frame {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            // A tuple serializes as a JSON array, which is exactly what we need.
            Frame::Call {
                id,
                action,
                payload,
            } => (CALL, id, action, payload).serialize(serializer),
            Frame::CallResult { id, payload } => (CALL_RESULT, id, payload).serialize(serializer),
            Frame::CallError {
                id,
                code,
                description,
                details,
            } => (CALL_ERROR, id, code, description, details).serialize(serializer),
        }
    }
}

impl<'de> Deserialize<'de> for Frame {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        // Step 1: read the array as a list of untyped JSON values.
        let items = Vec::<Value>::deserialize(deserializer)?;

        // Step 2: match on items.as_slice() and build the right variant.
        match items.as_slice() {
            [
                Value::Number(n),
                Value::String(id),
                Value::String(action),
                payload @ Value::Object(_),
            ] if n.as_u64() == Some(CALL) => Ok(Self::Call {
                id: id.clone(),
                action: action.clone(),
                payload: payload.clone(),
            }),
            [
                Value::Number(n),
                Value::String(id),
                payload @ Value::Object(_),
            ] if n.as_u64() == Some(CALL_RESULT) => Ok(Self::CallResult {
                id: id.clone(),
                payload: payload.clone(),
            }),
            [
                Value::Number(n),
                Value::String(id),
                Value::String(code),
                Value::String(description),
                details @ Value::Object(_),
            ] if n.as_u64() == Some(CALL_ERROR) => Ok(Self::CallError {
                id: id.clone(),
                code: code.clone(),
                description: description.clone(),
                details: details.clone(),
            }),
            _ => Err(de::Error::custom(format!(
                "invalid OCPP-J frame: unexpected shape ({} elements)",
                items.len()
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn rejects_non_array() {
        let raw = r#"{"id": "1"}"#;
        let err = serde_json::from_str::<Frame>(raw).unwrap_err();
        assert!(err.is_data(), "unexpected error kind: {err}");
    }

    #[test]
    fn parses_call_result() {
        let raw = r#"[3, "abc", {"status": "Accepted"}]"#;
        let frame: Frame = serde_json::from_str(raw).unwrap();
        assert_eq!(
            frame,
            Frame::CallResult {
                id: "abc".into(),
                payload: json!({"status": "Accepted"})
            }
        );
    }

    #[test]
    fn frame_number_is_not_string() {
        let raw = r#"["3", "abc", {"status": "Accepted"}]"#;
        let result = serde_json::from_str::<Frame>(raw);
        assert!(result.is_err(), "expected an error, got {result:?}");
    }

    #[test]
    fn call_result_roundtrip() {
        let frame = Frame::CallResult {
            id: "abc".into(),
            payload: json!({"status": "Accepted"}),
        };
        let text = serde_json::to_string(&frame).unwrap();
        assert_eq!(text, r#"[3,"abc",{"status":"Accepted"}]"#);
        assert_eq!(serde_json::from_str::<Frame>(&text).unwrap(), frame);
    }

    #[test]
    fn parses_call() {
        let raw = r#"[2, "abc", "Heartbeat", {}]"#;
        let frame: Frame = serde_json::from_str(raw).unwrap();
        assert_eq!(
            frame,
            Frame::Call {
                id: "abc".into(),
                action: "Heartbeat".into(),
                payload: json!({}),
            }
        );
    }

    #[test]
    fn call_roundtrip() {
        let frame = Frame::Call {
            id: "abc".into(),
            action: "Heartbeat".into(),
            payload: json!({}),
        };
        let text = serde_json::to_string(&frame).unwrap();
        assert_eq!(text, r#"[2,"abc","Heartbeat",{}]"#);
        assert_eq!(serde_json::from_str::<Frame>(&text).unwrap(), frame);
    }

    #[test]
    fn parses_call_error() {
        let raw =
            r#"[4, "19223201", "NotImplemented", "Requested Action is not known by receiver", {}]"#;
        let frame: Frame = serde_json::from_str(raw).unwrap();
        assert_eq!(
            frame,
            Frame::CallError {
                id: "19223201".into(),
                code: "NotImplemented".into(),
                description: "Requested Action is not known by receiver".into(),
                details: json!({})
            }
        );
    }

    #[test]
    fn call_error_roundtrip() {
        let frame = Frame::CallError {
            id: "19223201".into(),
            code: "NotImplemented".into(),
            description: "Requested Action is not known by receiver".into(),
            details: json!({}),
        };
        let text = serde_json::to_string(&frame).unwrap();
        assert_eq!(
            text,
            r#"[4,"19223201","NotImplemented","Requested Action is not known by receiver",{}]"#
        );
        assert_eq!(serde_json::from_str::<Frame>(&text).unwrap(), frame);
    }

    #[test]
    fn unknown_message_type() {
        let raw = r#"[5, "id", {}]"#;
        let err = serde_json::from_str::<Frame>(raw).unwrap_err();
        assert!(err.is_data(), "unexpected error kind: {err}");
    }

    #[test]
    fn id_is_not_string() {
        let raw = r#"[3, 123, {}]"#;
        let err = serde_json::from_str::<Frame>(raw).unwrap_err();
        assert!(err.is_data(), "unexpected error kind: {err}");
    }

    #[test]
    fn nested_is_not_an_object() {
        let cases = [
            ("payload call", r#"[2, "1", "Heartbeat", 42]"#),
            ("payload call_result", r#"[3, "1", 42]"#),
            (
                "details call_error",
                r#"[4, "1", "NotImplemented", "Requested Action is not known by receiver", 42]"#,
            ),
        ];
        for (name, raw) in cases {
            let result = serde_json::from_str::<Frame>(raw);
            assert!(
                result.is_err(),
                "case `{name}`: expected an error, got {result:?}"
            );
        }
    }

    #[test]
    fn rejects_wrong_arity() {
        let cases = [
            ("call too short", r#"[2, "1", "Heartbeat"]"#),
            ("call too long", r#"[2, "1", "Heartbeat", {}, {}]"#),
            ("call_result too short", r#"[3, "abc"]"#),
            (
                "call_result too long",
                r#"[3, "abc", {"status": "Accepted"}, {}]"#,
            ),
            (
                "call_error too short",
                r#"[4, "19223201", "NotImplemented", "Requested Action is not known by receiver"]"#,
            ),
            (
                "call_error too long",
                r#"[4, "19223201", "NotImplemented", "Requested Action is not known by receiver", {}, {}]"#,
            ),
        ];

        for (name, raw) in cases {
            let result = serde_json::from_str::<Frame>(raw);
            assert!(
                result.is_err(),
                "case `{name}`: expected an error, got {result:?}"
            );
        }
    }
}
