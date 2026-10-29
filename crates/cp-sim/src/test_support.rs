//! Test helpers shared by the modules of this crate.

use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use tokio::{net::TcpListener, sync::mpsc, time::timeout};
use tokio_tungstenite::{
    accept_hdr_async,
    tungstenite::{
        Message,
        handshake::server::{ErrorResponse, Request as HsRequest, Response},
        http::HeaderValue,
    },
};

use crate::frame::Frame;

// --- Fake CSMS: a WebSocket server driven by the test ---

#[expect(
    clippy::result_large_err,
    reason = "the signature is dictated by tungstenite's handshake `Callback` impl for closures, so the large `Err` variant cannot be boxed"
)]
fn echo_ocpp16(_req: &HsRequest, mut resp: Response) -> Result<Response, ErrorResponse> {
    resp.headers_mut().insert(
        "Sec-WebSocket-Protocol",
        HeaderValue::from_static("ocpp1.6"),
    );
    Ok(resp)
}

/// What the fake server saw, in order.
#[derive(Debug)]
pub enum Event {
    /// A client finished the WebSocket handshake.
    Connected,
    /// A text frame from the client.
    Frame(Frame),
    /// The current connection is gone (closed by either side or broken).
    Closed,
}

enum Command {
    Send(String),
    Close,
}

/// Accepts connections one after another, so a test can watch the client
/// disconnect and come back.
pub struct FakeCsms {
    pub url: String,
    events: mpsc::UnboundedReceiver<Event>,
    commands: mpsc::UnboundedSender<Command>,
}

impl FakeCsms {
    pub async fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("ws://{}/CP001", listener.local_addr().unwrap());
        let (event_tx, events) = mpsc::unbounded_channel();
        let (commands, mut command_rx) = mpsc::unbounded_channel();

        tokio::spawn(async move {
            while let Ok((tcp, _)) = listener.accept().await {
                let Ok(ws) = accept_hdr_async(tcp, echo_ocpp16).await else {
                    continue;
                };
                // Send errors only mean the test is gone; the command channel
                // tells us when to stop.
                let _ = event_tx.send(Event::Connected);
                let (mut sink, mut stream) = ws.split();
                loop {
                    tokio::select! {
                        msg = stream.next() => match msg {
                            Some(Ok(Message::Text(text))) => {
                                let frame: Frame = serde_json::from_str(&text).unwrap();
                                let _ = event_tx.send(Event::Frame(frame));
                            }
                            Some(Ok(_)) => {}
                            _ => break,
                        },
                        command = command_rx.recv() => match command {
                            Some(Command::Send(text)) => {
                                if sink.send(Message::text(text)).await.is_err() {
                                    break;
                                }
                            }
                            Some(Command::Close) => {
                                let _ = sink.close().await;
                                break;
                            }
                            // The test dropped the FakeCsms: close and stop serving.
                            None => {
                                let _ = sink.close().await;
                                return;
                            }
                        },
                    }
                }
                let _ = event_tx.send(Event::Closed);
            }
        });

        FakeCsms {
            url,
            events,
            commands,
        }
    }

    /// Next event; panics if nothing happens within a second.
    pub async fn recv_event(&mut self) -> Event {
        self.recv_event_within(Duration::from_secs(1)).await
    }

    /// Next event; panics if nothing happens within `wait`.
    pub async fn recv_event_within(&mut self, wait: Duration) -> Event {
        timeout(wait, self.events.recv())
            .await
            .expect("fake CSMS saw nothing")
            .expect("fake CSMS task is gone")
    }

    /// Next frame from the client, skipping new connections;
    /// panics if nothing arrives within a second or the connection closes.
    pub async fn recv(&mut self) -> Frame {
        self.recv_within(Duration::from_secs(1))
            .await
            .expect("client sent nothing")
    }

    /// Next frame from the client, skipping new connections, or None if
    /// nothing arrives in time. Panics if the connection closes.
    pub async fn recv_within(&mut self, wait: Duration) -> Option<Frame> {
        let deadline = tokio::time::Instant::now() + wait;
        loop {
            match tokio::time::timeout_at(deadline, self.events.recv()).await {
                Ok(Some(Event::Frame(frame))) => return Some(frame),
                Ok(Some(Event::Connected)) => continue,
                Ok(Some(Event::Closed)) => panic!("connection closed while waiting for a frame"),
                Ok(None) | Err(_) => return None,
            }
        }
    }

    pub fn send(&self, frame: Frame) {
        self.send_raw(&serde_json::to_string(&frame).unwrap());
    }

    pub fn send_raw(&self, text: &str) {
        self.command(Command::Send(text.to_owned()));
    }

    /// Closes the current connection; the server keeps accepting new ones.
    pub fn close_connection(&self) {
        self.command(Command::Close);
    }

    fn command(&self, command: Command) {
        if self.commands.send(command).is_err() {
            panic!("fake CSMS task is gone");
        }
    }
}
