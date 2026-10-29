//! OCPP-J client: one WebSocket connection, request/response matching, server CALLs.

use std::{error::Error, fmt, sync::Arc, time::Duration};

use futures_util::{
    SinkExt, StreamExt,
    stream::{SplitSink, SplitStream},
};
use serde_json::json;
use tokio::{net::TcpStream, sync::Mutex, task::JoinHandle, time::timeout};
use tokio_tungstenite::{
    MaybeTlsStream, WebSocketStream, connect_async,
    tungstenite::{self, Message, client::IntoClientRequest, http::HeaderValue},
};
use uuid::Uuid;

use crate::{frame::Frame, ocpp16::Request, pending::PendingCalls};

pub type BoxError = Box<dyn Error + Send + Sync>;

type WsStream = WebSocketStream<MaybeTlsStream<TcpStream>>;
type WsSink = SplitSink<WsStream, Message>;
type WsSource = SplitStream<WsStream>;

/// Why a CALL did not produce a typed response.
#[derive(Debug)]
pub enum CallFailure {
    /// The server answered with CALLERROR.
    Rejected { code: String, description: String },
    /// No answer within the call timeout.
    Timeout,
    /// The connection is gone.
    Disconnected,
    /// Our request payload could not be serialized.
    Encode(serde_json::Error),
    /// The answer's payload doesn't match the expected response type.
    Decode(serde_json::Error),
}

impl fmt::Display for CallFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CallFailure::Rejected { code, description } => {
                write!(f, "rejected by server: {code}: {description}")
            }
            CallFailure::Timeout => write!(f, "no response within timeout"),
            CallFailure::Disconnected => write!(f, "connection lost"),
            CallFailure::Encode(e) => write!(f, "cannot encode request: {e}"),
            CallFailure::Decode(e) => write!(f, "unexpected response payload: {e}"),
        }
    }
}

impl Error for CallFailure {}

pub struct Client {
    /// Write half of the socket. Used by callers and by the reader task
    /// (replies to server CALLs).
    sink: Mutex<WsSink>,
    /// Held for the whole duration of a call: OCPP allows one outstanding CALL at a time.
    call_lock: Mutex<()>,
    pending: PendingCalls,
    call_timeout: Duration,
}

impl Client {
    /// Connects with sub-protocol ocpp1.6 and starts the reader task.
    /// The returned handle finishes when the connection is closed.
    pub async fn connect(
        url: &str,
        call_timeout: Duration,
    ) -> Result<(Arc<Client>, JoinHandle<()>), BoxError> {
        let mut request = url.into_client_request()?;
        request.headers_mut().insert(
            "Sec-WebSocket-Protocol",
            HeaderValue::from_static("ocpp1.6"),
        );
        let (ws, _response) = connect_async(request).await?;
        let (sink, source) = ws.split();

        let client = Arc::new(Client {
            sink: Mutex::new(sink),
            call_lock: Mutex::new(()),
            pending: PendingCalls::default(),
            call_timeout,
        });
        let reader = tokio::spawn(read_loop(Arc::clone(&client), source));
        Ok((client, reader))
    }

    /// Sends a request and waits for its typed response.
    pub async fn call<R: Request>(&self, req: &R) -> Result<R::Response, CallFailure> {
        let _guard = self.call_lock.lock().await;
        let id = Uuid::new_v4().to_string();
        let frame = Frame::Call {
            id: id.clone(),
            action: R::ACTION.into(),
            payload: serde_json::to_value(req).map_err(CallFailure::Encode)?,
        };
        let rx = self.pending.register(id.clone());
        if let Err(err) = self.send_frame(&frame).await {
            self.pending.cancel(&id);
            eprintln!("failed to send {}: {err}", R::ACTION);
            return Err(CallFailure::Disconnected);
        }
        let frame = match timeout(self.call_timeout, rx).await {
            // In time, and the pager delivered a frame
            Ok(Ok(frame)) => frame,
            // In time, but the pager was destroyed without an answer
            Ok(Err(_)) => return Err(CallFailure::Disconnected),
            // Time is up: cross our ID out so the table doesn't keep a dead entry
            Err(_) => {
                self.pending.cancel(&id);
                return Err(CallFailure::Timeout);
            }
        };
        match frame {
            Frame::CallResult { payload, .. } => {
                serde_json::from_value(payload).map_err(CallFailure::Decode)
            }
            Frame::CallError {
                code, description, ..
            } => Err(CallFailure::Rejected { code, description }),
            Frame::Call { .. } => unreachable!("PendingCalls never delivers a CALL"),
        }
    }

    /// Serializes a frame, logs it as `>> ...` and writes it to the socket.
    async fn send_frame(&self, frame: &Frame) -> Result<(), tungstenite::Error> {
        let text = serde_json::to_string(frame).expect("Frame always serializes");
        println!(">> {text}");
        let mut sink = self.sink.lock().await;
        sink.send(Message::text(text)).await
    }
}

/// Reads the socket until the connection closes. Logs every text message as `<< ...`.
/// - responses go to `pending`
/// - server CALLs get a CALLERROR NotImplemented with empty details
/// - malformed or unexpected frames are logged and skipped
/// - Close, end of stream or a transport error end the loop
async fn read_loop(client: Arc<Client>, mut source: WsSource) {
    while let Some(msg) = source.next().await {
        match msg {
            Ok(Message::Text(text)) => {
                println!("<< {text}");
                let frame: Frame = match serde_json::from_str(&text) {
                    Ok(frame) => frame,
                    Err(err) => {
                        eprintln!("ignoring malformed frame: {err}: {text}");
                        continue;
                    }
                };
                if let Err(unclaimed) = client.pending.complete(frame) {
                    match unclaimed {
                        Frame::Call { id, action, .. } => {
                            let reply = Frame::CallError {
                                id,
                                code: "NotImplemented".into(),
                                description: format!("{action} is not implemented"),
                                details: json!({}),
                            };
                            if let Err(err) = client.send_frame(&reply).await {
                                eprintln!("failed to answer {action}: {err}");
                            }
                        }
                        other => eprintln!("nobody waits for this frame: {other:?}"),
                    }
                }
            }
            Ok(Message::Close(close)) => {
                println!("server closed connection: {close:?}");
                break;
            }
            Ok(_) => {} // ping/pong/binary
            Err(e) => {
                eprintln!("connection error: {e}");
                break;
            }
        }
    }
    println!("connection closed")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ocpp16::{HeartbeatRequest, HeartbeatResponse},
        test_support::{Event, FakeCsms},
    };
    use tokio::time::timeout;

    const LONG: Duration = Duration::from_secs(5);
    const SHORT: Duration = Duration::from_millis(300);

    // --- Helpers ---

    fn heartbeat_result(id: &str) -> Frame {
        Frame::CallResult {
            id: id.into(),
            payload: json!({"currentTime": "2026-10-15T12:00:00Z"}),
        }
    }

    /// Runs a Heartbeat call in the background so the test can play the server meanwhile.
    fn spawn_heartbeat(client: &Arc<Client>) -> JoinHandle<Result<HeartbeatResponse, CallFailure>> {
        let client = Arc::clone(client);
        tokio::spawn(async move { client.call(&HeartbeatRequest {}).await })
    }

    // --- Tests ---

    #[tokio::test]
    async fn call_returns_typed_response() {
        let mut csms = FakeCsms::start().await;
        let (client, _reader) = Client::connect(&csms.url, LONG).await.unwrap();

        let call = spawn_heartbeat(&client);
        let Frame::Call {
            id,
            action,
            payload,
        } = csms.recv().await
        else {
            panic!("expected a CALL");
        };
        assert_eq!(action, "Heartbeat");
        assert_eq!(payload, json!({}));

        csms.send(heartbeat_result(&id));
        let resp = call.await.unwrap().unwrap();
        assert_eq!(resp.current_time, "2026-10-15T12:00:00Z");
    }

    #[tokio::test]
    async fn call_error_is_reported_as_rejected() {
        let mut csms = FakeCsms::start().await;
        let (client, _reader) = Client::connect(&csms.url, LONG).await.unwrap();

        let call = spawn_heartbeat(&client);
        let id = csms.recv().await.id().to_owned();
        csms.send(Frame::CallError {
            id,
            code: "NotSupported".into(),
            description: "nope".into(),
            details: json!({}),
        });

        let err = call.await.unwrap().unwrap_err();
        assert!(
            matches!(&err, CallFailure::Rejected { code, .. } if code == "NotSupported"),
            "got {err:?}"
        );
    }

    #[tokio::test]
    async fn invalid_response_payload_is_reported() {
        let mut csms = FakeCsms::start().await;
        let (client, _reader) = Client::connect(&csms.url, LONG).await.unwrap();

        let call = spawn_heartbeat(&client);
        let id = csms.recv().await.id().to_owned();
        csms.send(Frame::CallResult {
            id,
            payload: json!({"unexpected": true}),
        });

        let err = call.await.unwrap().unwrap_err();
        assert!(matches!(err, CallFailure::Decode(_)), "got {err:?}");
    }

    #[tokio::test]
    async fn call_times_out_and_late_response_is_ignored() {
        let mut csms = FakeCsms::start().await;
        let (client, _reader) = Client::connect(&csms.url, SHORT).await.unwrap();

        let first = spawn_heartbeat(&client);
        let first_id = csms.recv().await.id().to_owned();
        let err = first.await.unwrap().unwrap_err();
        assert!(matches!(err, CallFailure::Timeout), "got {err:?}");

        // The answer to the first call arrives too late and must not be taken
        // for the answer to the second one.
        let second = spawn_heartbeat(&client);
        let second_id = csms.recv().await.id().to_owned();
        csms.send(Frame::CallResult {
            id: first_id,
            payload: json!({"currentTime": "late"}),
        });
        csms.send(heartbeat_result(&second_id));

        let resp = second.await.unwrap().unwrap();
        assert_eq!(resp.current_time, "2026-10-15T12:00:00Z");
    }

    #[tokio::test]
    async fn ignores_malformed_and_unrelated_frames() {
        let mut csms = FakeCsms::start().await;
        let (client, _reader) = Client::connect(&csms.url, LONG).await.unwrap();

        let call = spawn_heartbeat(&client);
        let id = csms.recv().await.id().to_owned();
        csms.send_raw("this is not json");
        csms.send_raw(r#"[3, "x"]"#);
        csms.send(heartbeat_result("someone-else"));
        csms.send(heartbeat_result(&id));

        assert!(call.await.unwrap().is_ok());
    }

    #[tokio::test]
    async fn answers_server_call_with_not_implemented() {
        let mut csms = FakeCsms::start().await;
        let (_client, _reader) = Client::connect(&csms.url, LONG).await.unwrap();

        csms.send(Frame::Call {
            id: "srv-1".into(),
            action: "Reset".into(),
            payload: json!({"type": "Soft"}),
        });

        match csms.recv().await {
            Frame::CallError {
                id, code, details, ..
            } => {
                assert_eq!(id, "srv-1");
                assert_eq!(code, "NotImplemented");
                assert_eq!(details, json!({}));
            }
            other => panic!("expected CALLERROR, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn only_one_call_in_flight() {
        let mut csms = FakeCsms::start().await;
        let (client, _reader) = Client::connect(&csms.url, LONG).await.unwrap();

        let a = spawn_heartbeat(&client);
        let b = spawn_heartbeat(&client);

        let first_id = csms.recv().await.id().to_owned();
        assert!(
            csms.recv_within(Duration::from_millis(200)).await.is_none(),
            "second CALL was sent before the first one was answered"
        );

        csms.send(heartbeat_result(&first_id));
        let second_id = csms.recv().await.id().to_owned();
        csms.send(heartbeat_result(&second_id));

        assert!(a.await.unwrap().is_ok());
        assert!(b.await.unwrap().is_ok());
    }

    #[tokio::test]
    async fn reader_stops_when_server_closes() {
        let csms = FakeCsms::start().await;
        let (_client, reader) = Client::connect(&csms.url, LONG).await.unwrap();

        drop(csms); // the fake server closes the connection
        timeout(Duration::from_secs(1), reader)
            .await
            .expect("reader task did not stop")
            .expect("reader task panicked");
    }

    #[tokio::test]
    async fn reader_stops_when_server_closes_connection_and_client_can_reconnect() {
        let mut csms = FakeCsms::start().await;
        let (_client, reader) = Client::connect(&csms.url, LONG).await.unwrap();
        assert!(matches!(csms.recv_event().await, Event::Connected));

        csms.close_connection();
        timeout(Duration::from_secs(1), reader)
            .await
            .expect("reader task did not stop")
            .expect("reader task panicked");
        assert!(matches!(csms.recv_event().await, Event::Closed));

        let (_client, _reader) = Client::connect(&csms.url, LONG).await.unwrap();
        assert!(matches!(csms.recv_event().await, Event::Connected));
    }
}
