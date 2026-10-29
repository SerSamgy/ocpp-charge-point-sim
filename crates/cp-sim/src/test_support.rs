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

// --- Fake CSMS: a one-connection WebSocket server driven by the test ---

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

pub struct FakeCsms {
    pub url: String,
    from_client: mpsc::UnboundedReceiver<Frame>,
    to_client: mpsc::UnboundedSender<String>,
}

impl FakeCsms {
    pub async fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("ws://{}/CP001", listener.local_addr().unwrap());
        let (in_tx, from_client) = mpsc::unbounded_channel();
        let (to_client, mut out_rx) = mpsc::unbounded_channel::<String>();

        tokio::spawn(async move {
            let (tcp, _) = listener.accept().await.unwrap();
            let ws = accept_hdr_async(tcp, echo_ocpp16).await.unwrap();
            let (mut sink, mut stream) = ws.split();
            loop {
                tokio::select! {
                    msg = stream.next() => match msg {
                        Some(Ok(Message::Text(text))) => {
                            let frame: Frame = serde_json::from_str(&text).unwrap();
                            if in_tx.send(frame).is_err() {
                                break;
                            }
                        }
                        Some(Ok(_)) => {}
                        _ => break,
                    },
                    text = out_rx.recv() => match text {
                        Some(text) => {
                            if sink.send(Message::text(text)).await.is_err() {
                                break;
                            }
                        }
                        // The test dropped the FakeCsms: close the connection.
                        None => {
                            let _ = sink.close().await;
                            break;
                        }
                    },
                }
            }
        });

        FakeCsms {
            url,
            from_client,
            to_client,
        }
    }

    /// Next frame from the client; panics if nothing arrives within a second.
    pub async fn recv(&mut self) -> Frame {
        self.recv_within(Duration::from_secs(1))
            .await
            .expect("client sent nothing")
    }

    /// Next frame from the client, or None if nothing arrives in time.
    pub async fn recv_within(&mut self, wait: Duration) -> Option<Frame> {
        timeout(wait, self.from_client.recv()).await.ok().flatten()
    }

    pub fn send(&self, frame: Frame) {
        self.send_raw(&serde_json::to_string(&frame).unwrap());
    }

    pub fn send_raw(&self, text: &str) {
        self.to_client.send(text.to_owned()).unwrap();
    }
}
