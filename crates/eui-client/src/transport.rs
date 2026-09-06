//! The session socket: WebSocket over TLS, binary frames only.
//!
//! Runs on its own tokio runtime thread and talks to the window through two
//! channels, so the window's event loop never blocks on the network and the
//! network never touches the GPU.

use std::sync::mpsc;
use std::thread;

use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::tungstenite::Message;

/// Why a connection could not be made or was dropped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransportError {
    /// The URL is not `wss://`, and the debug-only loopback exception did not
    /// apply.
    Insecure(String),
    /// The URL could not be parsed or the handshake failed.
    Connect(String),
    /// The server sent a text frame; the protocol is binary only.
    TextFrame,
    /// The socket closed.
    Closed,
}

impl std::fmt::Display for TransportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Insecure(u) => write!(f, "refusing insecure session URL {u}"),
            Self::Connect(e) => write!(f, "connect failed: {e}"),
            Self::TextFrame => f.write_str("server sent a text frame"),
            Self::Closed => f.write_str("connection closed"),
        }
    }
}

impl std::error::Error for TransportError {}

/// What arrives from the socket.
#[derive(Debug)]
pub enum Incoming {
    /// A binary message.
    Message(Vec<u8>),
    /// The connection ended.
    Closed(TransportError),
}

/// A live connection: send bytes in, receive [`Incoming`] out.
#[derive(Debug)]
pub struct Connection {
    /// Outgoing messages.
    pub tx: mpsc::Sender<Vec<u8>>,
    /// Incoming messages; the receiver end belongs to the caller.
    pub rx: mpsc::Receiver<Incoming>,
}

/// Enforce `spec/01-transport.md` §1: TLS only. A release build refuses
/// `ws://` unconditionally; a debug build allows it for loopback when
/// `EUI_ALLOW_INSECURE_LOOPBACK=1`, which is how the examples run.
pub fn check_url(url: &str) -> Result<(), TransportError> {
    if url.starts_with("wss://") {
        return Ok(());
    }
    let loopback = url.starts_with("ws://127.0.0.1") || url.starts_with("ws://localhost") || url.starts_with("ws://[::1]");
    let allowed = cfg!(debug_assertions) && loopback && std::env::var("EUI_ALLOW_INSECURE_LOOPBACK").as_deref() == Ok("1");
    if allowed {
        Ok(())
    } else {
        Err(TransportError::Insecure(url.to_owned()))
    }
}

/// Connect, spawning the socket's runtime on a background thread. `first` is
/// sent as soon as the socket is open — the `Hello` frame.
pub fn connect(url: &str, first: Vec<u8>, notify: impl Fn() + Send + 'static) -> Result<Connection, TransportError> {
    check_url(url)?;
    let url = url.to_owned();
    let (out_tx, out_rx) = mpsc::channel::<Vec<u8>>();
    let (in_tx, in_rx) = mpsc::channel::<Incoming>();

    thread::Builder::new()
        .name("eui-transport".into())
        .spawn(move || {
            let rt = match tokio::runtime::Builder::new_current_thread().enable_all().build() {
                Ok(rt) => rt,
                Err(e) => {
                    let _ = in_tx.send(Incoming::Closed(TransportError::Connect(e.to_string())));
                    notify();
                    return;
                }
            };
            rt.block_on(async move {
                let (ws, _) = match tokio_tungstenite::connect_async(&url).await {
                    Ok(ok) => ok,
                    Err(e) => {
                        let _ = in_tx.send(Incoming::Closed(TransportError::Connect(e.to_string())));
                        notify();
                        return;
                    }
                };
                let (mut sink, mut stream) = ws.split();
                if sink.send(Message::Binary(first)).await.is_err() {
                    let _ = in_tx.send(Incoming::Closed(TransportError::Closed));
                    notify();
                    return;
                }
                // Outgoing: poll the std channel without blocking the runtime.
                let sender = tokio::spawn(async move {
                    loop {
                        match out_rx.try_recv() {
                            Ok(bytes) => {
                                if sink.send(Message::Binary(bytes)).await.is_err() {
                                    return;
                                }
                            }
                            Err(mpsc::TryRecvError::Empty) => tokio::time::sleep(std::time::Duration::from_millis(2)).await,
                            Err(mpsc::TryRecvError::Disconnected) => {
                                let _ = sink.send(Message::Close(None)).await;
                                return;
                            }
                        }
                    }
                });
                while let Some(msg) = stream.next().await {
                    let event = match msg {
                        Ok(Message::Binary(b)) => Incoming::Message(b),
                        Ok(Message::Text(_)) => Incoming::Closed(TransportError::TextFrame),
                        Ok(Message::Ping(_) | Message::Pong(_) | Message::Frame(_)) => continue,
                        Ok(Message::Close(_)) | Err(_) => Incoming::Closed(TransportError::Closed),
                    };
                    let fatal = matches!(event, Incoming::Closed(_));
                    if in_tx.send(event).is_err() {
                        break;
                    }
                    notify();
                    if fatal {
                        break;
                    }
                }
                sender.abort();
            });
        })
        .map_err(|e| TransportError::Connect(e.to_string()))?;

    Ok(Connection { tx: out_tx, rx: in_rx })
}
