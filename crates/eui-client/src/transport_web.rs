//! The session socket, where the host already owns one: a browser's
//! `WebSocket`, binary frames only.
//!
//! The same types as [`crate::transport`], the same [`Connection`],
//! and `app.rs` cannot tell which one it has. What differs is everything
//! underneath: there is no runtime, no thread and no TLS here, because a
//! page is handed a socket that has already done its handshake, checked
//! its chain and chosen its cipher. That is not a smaller client — it is a
//! client that is not permitted to do those things, which is the honest
//! difference and the one `doc/docs/eui/security.md` has to state.
//!
//! The types are written out again rather than shared behind a trait. Two
//! of them are enums `app.rs` matches on exhaustively, so a variant added
//! on one side and not the other is a compile error at the match and not a
//! silent divergence — which is the property a trait would cost, and the
//! whole of what pays for writing them twice.
//!
//! It buys nothing for a `const fn`, where the two copies would simply
//! disagree and say nothing about it, so [`kind_needs_server`] is written
//! once in [`crate::dial`] and re-exported from both sides.

use std::cell::Cell;
use std::rc::Rc;
use std::sync::mpsc;

use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

/// Why a connection could not be made or was dropped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransportError {
    /// The URL is not `wss://`, and the debug-only loopback exception did not
    /// apply.
    Insecure(String),
    /// The URL could not be parsed or the handshake failed.
    Connect(String),
    /// The server answered the upgrade with an HTTP status rather than
    /// switching protocols.
    ///
    /// **A page never builds this one.** The `WebSocket` API does not
    /// expose the handshake response, so a `404` from an address that is
    /// not a session arrives here as an `error` event and nothing more —
    /// indistinguishable from a socket that broke. The variant exists
    /// because `app.rs` matches this enum exhaustively and the two
    /// transports are written out twice precisely so that a case present
    /// on one side and absent on the other is a compile error rather than
    /// a silent divergence. What it does not do is give a page the refusal
    /// that stops the retry ladder on a desktop; that gap is the browser's
    /// to carry until the address is checked before it is dialled.
    Refused(u16, String),
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
            Self::Refused(code, why) if why.is_empty() => write!(f, "the server answered {code} rather than opening a session"),
            Self::Refused(code, why) => write!(f, "the server answered {code}: {why}"),
            Self::TextFrame => f.write_str("server sent a text frame"),
            Self::Closed => f.write_str("connection closed"),
        }
    }
}

impl std::error::Error for TransportError {}

pub use crate::dial::kind_needs_server;

/// What arrives from the socket, or from an asset fetch.
#[derive(Debug)]
pub enum Incoming {
    /// A binary message.
    Message(Vec<u8>),
    /// The connection ended.
    Closed(TransportError),
    /// An asset fetch finished: verified bytes, or why not.
    Asset([u8; 32], Result<Vec<u8>, String>),
}

/// A live connection: send bytes in, receive [`Incoming`] out.
pub struct Connection {
    /// Outgoing messages.
    ///
    /// The same tokio channel as the native side, and for a reason that
    /// survives the change of platform: the far end is a task, not a
    /// thread, and a task has to *wait* on it. Keeping the type identical
    /// is also what leaves every caller above this line untouched.
    pub tx: tokio::sync::mpsc::UnboundedSender<Vec<u8>>,
    /// Incoming messages; the receiver end belongs to the caller.
    pub rx: mpsc::Receiver<Incoming>,
    /// The HTTPS origin assets come from.
    pub origin: String,
    in_tx: mpsc::Sender<Incoming>,
    /// `Rc`, not `Arc`, and `Fn()` without `Send`: winit's web
    /// `EventLoopProxy` holds an `std::rc::Weak`, so the waker this closure
    /// carries is `!Send` on this target and no bound here may ask for it.
    notify: Rc<dyn Fn()>,
    /// Kept alive for as long as the socket is. A `Closure` dropped while
    /// the socket still refers to it is a call into freed Rust on the next
    /// message, so these are fields and not temporaries.
    _open: Closure<dyn FnMut()>,
    _message: Closure<dyn FnMut(web_sys::MessageEvent)>,
    _error: Closure<dyn FnMut(web_sys::ErrorEvent)>,
    _close: Closure<dyn FnMut(web_sys::CloseEvent)>,
    ws: web_sys::WebSocket,
}

impl std::fmt::Debug for Connection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Connection").field("origin", &self.origin).finish()
    }
}

impl Drop for Connection {
    fn drop(&mut self) {
        // The handlers come off *before* the close, and that order is the
        // whole of this method.
        //
        // `close()` is a request, not an end: the socket stays alive long
        // enough to deliver a `close` event, and a `message` already queued
        // is still delivered. The `Closure` fields below are freed the
        // moment this returns, so a socket still holding them calls into
        // freed Rust and `wasm-bindgen` throws
        //
        //     Uncaught Error: closure invoked recursively or after being
        //     dropped                              ... at WebSocket.real
        //
        // which is a trap: the module stops, so the window stops drawing
        // and the page is left with whatever was on the canvas and a note
        // that still says "Connecting…". Clearing the handlers first means
        // nothing the browser has left to deliver has anywhere to land.
        self.ws.set_onopen(None);
        self.ws.set_onmessage(None);
        self.ws.set_onerror(None);
        self.ws.set_onclose(None);
        // The page outlives the session; a socket left open would go on
        // waking a loop that no longer has a tab for it.
        let _ = self.ws.close();
    }
}

/// Everything asset fetching needed, without the socket it used to hang off.
///
/// A page fetched over `GET /_eui/view/<component>` (01 §2.4) has no session
/// and may still name a picture, so the half of a `Connection` that goes and
/// gets bytes has to outlive the half that does not exist. Cheap to clone;
/// one per tab.
#[derive(Clone)]
pub struct Fetcher {
    origin: String,
    in_tx: mpsc::Sender<Incoming>,
    /// `Rc` and no `Send`, for the reason [`Connection::notify`] carries one:
    /// winit's web `EventLoopProxy` holds an `std::rc::Weak`.
    notify: Rc<dyn Fn()>,
}

impl std::fmt::Debug for Fetcher {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Fetcher").field("origin", &self.origin).finish()
    }
}

impl Fetcher {
    /// A fetcher with no socket behind it, and the channel its answers
    /// arrive on.
    ///
    /// `cookie` is taken and dropped, where a desktop keeps it. That is not
    /// an oversight and not a smaller client: `fetch_bytes` sends
    /// `credentials: omit` because 01 §1 forbids a client identifier, and
    /// the loopback cookie a desktop carries to reach its own `soli serve`
    /// has no analogue in a page. The argument stays so that the one caller
    /// reads the same on every target.
    pub fn alone(origin: String, cookie: Option<String>, notify: impl Fn() + 'static) -> (Self, mpsc::Receiver<Incoming>) {
        let _ = cookie;
        let (in_tx, rx) = mpsc::channel::<Incoming>();
        (Self { origin, in_tx, notify: Rc::new(notify) }, rx)
    }

    /// The HTTPS origin this fetches from.
    pub fn origin(&self) -> &str {
        &self.origin
    }

    /// Fetch an asset; the result arrives as [`Incoming::Asset`] and the
    /// notifier is called — the same contract as a desktop's, with the
    /// thread replaced by the task the browser was going to run anyway.
    pub fn request_asset(&self, hash: [u8; 32]) {
        let url = format!("{}/_eui/asset/{}", self.origin, crate::assets::hex(&hash));
        let tx = self.in_tx.clone();
        let notify = Rc::clone(&self.notify);
        wasm_bindgen_futures::spawn_local(async move {
            let result = match fetch_bytes(&url).await {
                // Before anything is decoded, exactly as on a desktop: a
                // substituted body is discarded, not displayed.
                Ok(bytes) if *blake3::hash(&bytes).as_bytes() == hash => Ok(bytes),
                Ok(_) => Err(crate::assets::AssetError::HashMismatch.to_string()),
                Err(e) => Err(e),
            };
            let _ = tx.send(Incoming::Asset(hash, result));
            notify();
        });
    }
}

impl Connection {
    /// This session's asset fetching, as a handle that outlives the socket.
    pub fn fetcher(&self) -> Fetcher {
        Fetcher { origin: self.origin.clone(), in_tx: self.in_tx.clone(), notify: Rc::clone(&self.notify) }
    }

    /// Fetch an asset. Unchanged for every caller.
    pub fn request_asset(&self, hash: [u8; 32]) {
        self.fetcher().request_asset(hash);
    }

    /// Queue `bytes` for the socket. `false` when it is gone.
    pub fn send(&self, bytes: Vec<u8>) -> bool {
        self.tx.send(bytes).is_ok()
    }

    /// Bytes sent and not yet written: the browser's own count.
    pub fn backlog(&self) -> usize {
        self.ws.buffered_amount() as usize
    }
}

/// The backlog under which a producer held back for the socket may go on.
pub const BACKLOG_LOW: usize = 4 * eui_proto::limits::MAX_TRANSFER_CHUNK_BYTES;

/// One `GET` of `url`, bounded by the same ceiling a desktop applies.
///
/// No cookie and no credential: 01 §1 forbids a client identifier, and the
/// loopback cookie a desktop carries has no analogue here. A redirect is
/// refused rather than followed — the browser will not say where it went,
/// and 01 §1 has a client refuse one that changes origin.
async fn fetch_bytes(url: &str) -> Result<Vec<u8>, String> {
    let opts = web_sys::RequestInit::new();
    opts.set_method("GET");
    opts.set_mode(web_sys::RequestMode::SameOrigin);
    opts.set_redirect(web_sys::RequestRedirect::Error);
    opts.set_credentials(web_sys::RequestCredentials::Omit);
    let request = web_sys::Request::new_with_str_and_init(url, &opts).map_err(js_msg)?;
    let window = web_sys::window().ok_or_else(|| "no window".to_owned())?;
    let response = wasm_bindgen_futures::JsFuture::from(window.fetch_with_request(&request)).await.map_err(js_msg)?;
    let response: web_sys::Response = response.dyn_into().map_err(|_| "not a Response".to_owned())?;
    if response.status() != 200 {
        return Err(format!("status {}", response.status()));
    }
    let buffer = wasm_bindgen_futures::JsFuture::from(response.array_buffer().map_err(js_msg)?).await.map_err(js_msg)?;
    let array = js_sys::Uint8Array::new(&buffer);
    if array.length() as usize > crate::assets::MAX_ASSET_BYTES {
        return Err(crate::assets::AssetError::TooLarge.to_string());
    }
    Ok(array.to_vec())
}

/// What a `JsValue` had to say, for a log. Never for the server: a host's
/// diagnostic names the host (08 §8).
fn js_msg(v: JsValue) -> String {
    v.as_string().or_else(|| v.dyn_ref::<js_sys::Error>().map(|e| String::from(e.message()))).unwrap_or_else(|| "the browser did not say".to_owned())
}

/// Enforce `spec/01-transport.md` §1: TLS only, with the loopback
/// exception of 08 §1 taken from the page rather than from a flag.
///
/// `host_loopback` is always false here — a page embeds no server — and
/// there is no environment, so `EUI_ALLOW_INSECURE_LOOPBACK` cannot be set.
/// Something has to stand in for it or a developer cannot point this at a
/// `soli serve --dev` on their own machine, and what stands in is a better
/// question than the flag was: **is the page itself on loopback?**
///
/// A flag can be set by whoever runs the client. This cannot: a document
/// served from anywhere but `localhost` may not ask for `ws://` at all, no
/// matter what it says, because the answer comes from the browser's idea of
/// where the document came from and not from anything the document wrote.
/// So the exception is available exactly where it is meant to be — a
/// developer's own machine — and is unreachable from the web, which is
/// more than the environment variable manages.
pub fn check_url(url: &str, _host_loopback: bool) -> Result<(), TransportError> {
    if url.starts_with("wss://") {
        return Ok(());
    }
    let loopback = crate::dial::is_loopback_url(url);
    if loopback && page_is_loopback() {
        return Ok(());
    }
    Err(TransportError::Insecure(url.to_owned()))
}

/// Whether the document this module is running in came from loopback.
///
/// Conservative in both directions: a hostname that cannot be read at all
/// is not loopback, and neither is one that merely looks like it.
fn page_is_loopback() -> bool {
    let Some(host) = web_sys::window().and_then(|w| w.location().hostname().ok()) else { return false };
    matches!(host.as_str(), "127.0.0.1" | "localhost" | "::1" | "[::1]")
}

/// Connect. `first` is sent as soon as the socket opens — the `Hello`,
/// which the caller has already encoded — and `notify` is called whenever
/// something is waiting on `rx`.
///
/// `cookie` is accepted and ignored: a page may not set a `Cookie` header
/// on an upgrade, and the browser sends whatever it holds for the origin on
/// its own. Taken by value anyway so the two transports have one signature.
pub fn connect(url: &str, first: Vec<u8>, _cookie: Option<String>, host_loopback: bool, notify: impl Fn() + 'static) -> Result<Connection, TransportError> {
    check_url(url, host_loopback)?;
    let origin = crate::assets::origin_for(url).map_err(|e| TransportError::Connect(e.to_string()))?;

    let (out_tx, out_rx) = tokio::sync::mpsc::unbounded_channel::<Vec<u8>>();
    let (in_tx, in_rx) = mpsc::channel::<Incoming>();
    let notify: Rc<dyn Fn()> = Rc::new(notify);

    let ws = web_sys::WebSocket::new(url).map_err(|e| TransportError::Connect(js_msg(e)))?;
    ws.set_binary_type(web_sys::BinaryType::Arraybuffer);

    // `onerror` and `onclose` both fire on a failed connection, and the
    // window backs off on the first `Closed` it sees. A second would read
    // as an instant re-failure and cost a step of the ladder.
    let ended = Rc::new(Cell::new(false));

    let open = {
        let ws = ws.clone();
        // `onopen` fires once, but its type does not say so, and both of
        // these are consumed: the `Hello` is sent and the receiver is moved
        // into the task that drains it. Taken out of an `Option` rather
        // than cloned, so a second open — which would be a socket we did
        // not make — sends nothing and starts no second drain.
        let mut once = Some((first, out_rx));
        Closure::<dyn FnMut()>::new(move || {
            let Some((first, out_rx)) = once.take() else { return };
            if ws.send_with_u8_array(&first).is_err() {
                return;
            }
            // The drain, which is the whole of the sender task: it parks on
            // `recv` and costs nothing until something is sent.
            let ws = ws.clone();
            let mut out_rx = out_rx;
            wasm_bindgen_futures::spawn_local(async move {
                while let Some(bytes) = out_rx.recv().await {
                    if ws.ready_state() != web_sys::WebSocket::OPEN || ws.send_with_u8_array(&bytes).is_err() {
                        return;
                    }
                }
                // Every sender is gone: the session is over, and the far
                // end should hear so rather than time out.
                let _ = ws.close();
            });
        })
    };
    ws.set_onopen(Some(open.as_ref().unchecked_ref()));

    let message = {
        let tx = in_tx.clone();
        let notify = Rc::clone(&notify);
        let ended = Rc::clone(&ended);
        let ws = ws.clone();
        Closure::<dyn FnMut(web_sys::MessageEvent)>::new(move |e: web_sys::MessageEvent| {
            let data = e.data();
            if let Some(buffer) = data.dyn_ref::<js_sys::ArrayBuffer>() {
                let _ = tx.send(Incoming::Message(js_sys::Uint8Array::new(buffer).to_vec()));
            } else {
                // 01 §3: the protocol is binary, and a text frame closes
                // the session rather than being skipped.
                if !ended.replace(true) {
                    let _ = tx.send(Incoming::Closed(TransportError::TextFrame));
                }
                let _ = ws.close();
            }
            notify();
        })
    };
    ws.set_onmessage(Some(message.as_ref().unchecked_ref()));

    let error = {
        let tx = in_tx.clone();
        let notify = Rc::clone(&notify);
        let ended = Rc::clone(&ended);
        Closure::<dyn FnMut(web_sys::ErrorEvent)>::new(move |_e: web_sys::ErrorEvent| {
            // Deliberately no detail: a browser tells a page that its
            // socket failed and never why, so saying so beats inventing a
            // reason that reads like one we looked up.
            if !ended.replace(true) {
                let _ = tx.send(Incoming::Closed(TransportError::Connect("the browser did not say why".to_owned())));
                notify();
            }
        })
    };
    ws.set_onerror(Some(error.as_ref().unchecked_ref()));

    let close = {
        let tx = in_tx.clone();
        let notify = Rc::clone(&notify);
        let ended = Rc::clone(&ended);
        Closure::<dyn FnMut(web_sys::CloseEvent)>::new(move |_e: web_sys::CloseEvent| {
            if !ended.replace(true) {
                let _ = tx.send(Incoming::Closed(TransportError::Closed));
                notify();
            }
        })
    };
    ws.set_onclose(Some(close.as_ref().unchecked_ref()));

    Ok(Connection { tx: out_tx, rx: in_rx, origin, in_tx, notify, _open: open, _message: message, _error: error, _close: close, ws })
}
