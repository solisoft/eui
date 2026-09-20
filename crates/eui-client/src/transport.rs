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
    /// The URL could not be parsed, or the socket could not be reached.
    Connect(String),
    /// The server answered the upgrade with an HTTP status rather than
    /// switching protocols.
    ///
    /// Separate from [`Self::Connect`] because it is **not** a network fault
    /// and must not be retried: a `404` here means this address is not a
    /// session and will not become one, and a client that treats it as a
    /// dropped connection climbs a backoff ladder for ever against a server
    /// that is answering perfectly well.
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
    /// A tokio channel rather than a `std` one, because the far end of it
    /// is inside the socket's runtime and has to *wait* on it. A `std`
    /// receiver cannot be awaited, and the only way to read one from an
    /// async task is to ask whether it has anything and go back to sleep —
    /// which is a poll, at whatever interval is chosen, for as long as the
    /// session lasts. `send` is not async on either, so nothing above this
    /// line changes.
    pub tx: tokio::sync::mpsc::UnboundedSender<Vec<u8>>,
    /// Incoming messages; the receiver end belongs to the caller.
    pub rx: mpsc::Receiver<Incoming>,
    /// The HTTPS origin assets come from.
    pub origin: String,
    in_tx: mpsc::Sender<Incoming>,
    notify: std::sync::Arc<dyn Fn() + Send + Sync>,
    /// The cookie this session presents, `name=value`. Per connection, not
    /// per process: with two sessions in one process a global would send
    /// one application's loopback cookie to the other's origin.
    cookie: Option<String>,
}

impl std::fmt::Debug for Connection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Connection").field("origin", &self.origin).finish()
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
    notify: std::sync::Arc<dyn Fn() + Send + Sync>,
    cookie: Option<String>,
}

impl std::fmt::Debug for Fetcher {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Fetcher").field("origin", &self.origin).finish()
    }
}

impl Fetcher {
    /// A fetcher with no socket behind it, and the channel its answers
    /// arrive on.
    pub fn alone(origin: String, cookie: Option<String>, notify: impl Fn() + Send + Sync + 'static) -> (Self, mpsc::Receiver<Incoming>) {
        let (in_tx, rx) = mpsc::channel::<Incoming>();
        (Self { origin, in_tx, notify: std::sync::Arc::new(notify), cookie }, rx)
    }

    /// The HTTPS origin this fetches from.
    pub fn origin(&self) -> &str {
        &self.origin
    }

    /// Fetch an asset on a worker thread; the result arrives as
    /// [`Incoming::Asset`] and the notifier is called.
    pub fn request_asset(&self, hash: [u8; 32]) {
        let origin = self.origin.clone();
        let tx = self.in_tx.clone();
        let notify = std::sync::Arc::clone(&self.notify);
        let cookie = self.cookie.clone();
        let _ = thread::Builder::new().name("eui-asset".into()).spawn(move || {
            // `EUI_TRACE=1` times this. There was no trace on the asset path
            // at all, and it is the one part of opening a picture that is
            // neither the server's nor the frame's -- a viewer that took six
            // seconds to show a photograph could be measured everywhere
            // except where the time was going.
            let t0 = crate::time::Instant::now();
            let result = crate::assets::fetch(&origin, &hash, cookie.as_deref()).map_err(|e| e.to_string());
            crate::driver::trace(|| match &result {
                Ok(bytes) => format!("asset {} fetched {} bytes in {} ms", crate::assets::hex(&hash).get(..8).unwrap_or(""), bytes.len(), t0.elapsed().as_millis()),
                Err(e) => format!("asset {} failed in {} ms: {e}", crate::assets::hex(&hash).get(..8).unwrap_or(""), t0.elapsed().as_millis()),
            });
            let _ = tx.send(Incoming::Asset(hash, result));
            notify();
        });
    }
}

impl Connection {
    /// This session's asset fetching, as a handle that outlives the socket.
    pub fn fetcher(&self) -> Fetcher {
        Fetcher { origin: self.origin.clone(), in_tx: self.in_tx.clone(), notify: std::sync::Arc::clone(&self.notify), cookie: self.cookie.clone() }
    }

    /// Fetch an asset on a worker thread. Unchanged for every caller.
    pub fn request_asset(&self, hash: [u8; 32]) {
        self.fetcher().request_asset(hash);
    }
}

/// The TLS the client speaks, built once: **TLS 1.3 only** (spec 01 §1),
/// verified against three sets of roots.
///
/// 1. The **public web's**, from `webpki-roots`: a published EUI application
///    is served from an ordinary origin behind an ordinary certificate.
/// 2. The **machine's own**, from the platform trust store. An application on
///    a private network, a staging box, or a development proxy under a
///    `.test` name is signed by a root no public list carries — but one the
///    browser on the same desktop already trusts, because an administrator
///    or `mkcert -install` put it there. A window that refused what the
///    browser beside it accepts reads as broken, not as careful.
///    `EUI_CA_SYSTEM=0` leaves the client on the public roots alone.
/// 3. Whatever **`EUI_CA_FILE`** names — one PEM bundle, or several
///    separated by `:` — for a root that is in neither, such as a CA carried
///    with a deployment rather than installed on the machine.
///
/// Roots are added, never removed, and there is no flag anywhere that turns
/// verification off: a client that would accept any certificate on request
/// is a client whose TLS means nothing.
pub fn tls_config() -> std::sync::Arc<rustls::ClientConfig> {
    static CONFIG: std::sync::OnceLock<std::sync::Arc<rustls::ClientConfig>> = std::sync::OnceLock::new();
    std::sync::Arc::clone(CONFIG.get_or_init(|| {
        let mut roots = rustls::RootCertStore::empty();
        roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
        // Then the machine's own store, unless told not to. This is what
        // makes the client agree with everything else on the desktop: a
        // development CA from `mkcert -install`, a corporate root, a
        // private CA an administrator installed — all of them are already
        // trusted by the browser beside it, and a window that refused what
        // the browser accepts would be read as broken, not as careful.
        // `EUI_CA_SYSTEM=0` leaves the client on the public roots alone.
        //
        // A desktop only. Neither phone keeps its authorities in a
        // directory of PEMs — iOS has a Keychain and Android has them in
        // the framework — so `rustls-native-certs` there is not a store
        // that comes up empty, it is a question that cannot be asked. The
        // crate is left out for those targets rather than called and
        // quietly believed, and `EUI_CA_FILE` is how a private CA reaches
        // a phone. A session refused for want of one says so on the glass
        // and names the variable (see `refusal` in `app.rs`), because a
        // phone has no stderr to say it on.
        #[cfg(not(any(target_os = "android", target_os = "ios")))]
        if std::env::var("EUI_CA_SYSTEM").as_deref() != Ok("0") {
            let found = rustls_native_certs::load_native_certs();
            for cert in found.certs {
                let _ = roots.add(cert);
            }
            for e in found.errors {
                eprintln!("eui: platform trust store: {e}");
            }
        }
        for path in std::env::var("EUI_CA_FILE").unwrap_or_default().split(':').filter(|p| !p.is_empty()) {
            use rustls::pki_types::pem::PemObject;
            match rustls::pki_types::CertificateDer::pem_file_iter(path) {
                Ok(certs) => {
                    let mut added = 0usize;
                    for cert in certs.flatten() {
                        if roots.add(cert).is_ok() {
                            added += 1;
                        }
                    }
                    eprintln!("eui: EUI_CA_FILE {path}: {added} additional root(s)");
                }
                Err(e) => eprintln!("eui: EUI_CA_FILE {path}: {e}; ignored"),
            }
        }
        // Spec 01 §1: TLS 1.3 is REQUIRED. Saying so here rather than
        // leaving it to the library's default is what makes "no downgrade"
        // a property of this client and not of its dependency tree.
        let config = rustls::ClientConfig::builder_with_protocol_versions(&[&rustls::version::TLS13]).with_root_certificates(roots).with_no_client_auth();
        std::sync::Arc::new(config)
    }))
}

/// Enforce `spec/01-transport.md` §1: TLS only. A release build refuses
/// `ws://` unconditionally, except on loopback when
/// `EUI_ALLOW_INSECURE_LOOPBACK=1` is set — how the examples run, and how a
/// developer uses the release client against a local Soli — or when an
/// embedding host trusts its own process.
/// `host_loopback` says this particular session's server is embedded in
/// this process (spec 08 §1). It is a parameter rather than a process flag
/// because one such session must not vouch for the others: with several
/// sessions in one process a sticky global would turn `ws://` on for every
/// session opened after the first embedded one.
pub fn check_url(url: &str, host_loopback: bool) -> Result<(), TransportError> {
    if url.starts_with("wss://") {
        return Ok(());
    }
    let loopback = crate::dial::is_loopback_url(url);
    // Loopback only, and only when asked by name: a developer running a
    // local Soli should get to use the fast build of the client too.
    let asked = std::env::var("EUI_ALLOW_INSECURE_LOOPBACK").as_deref() == Ok("1");
    let allowed = loopback && (host_loopback || asked);
    if allowed && asked && !cfg!(debug_assertions) {
        eprintln!("eui: EUI_ALLOW_INSECURE_LOOPBACK=1 — plain ws:// on loopback, nothing is encrypted");
    }
    if allowed {
        Ok(())
    } else {
        Err(TransportError::Insecure(url.to_owned()))
    }
}

/// A first render fetched over HTTPS, with no session behind it (01 §2.4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct View {
    /// BLAKE3 of the `Batch` frames — the identity of the *tree*, computed
    /// here and never taken from a header. Deliberately not the body's hash:
    /// the `Welcome` differs between the two roads a tree can arrive by, so
    /// hashing the whole body would name something the socket can never
    /// agree to.
    pub tree: [u8; 32],
    /// The `ETag` verbatim, for a later `If-None-Match`.
    pub etag: Option<String>,
    /// The body split on frame boundaries, in order, still encoded — the
    /// window hands encoded bytes to the worker and never reads a frame
    /// itself.
    pub frames: Vec<Vec<u8>>,
}

/// Why a view could not be had.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ViewError {
    /// A `404`: this component is not served that way. Not an error — the
    /// ordinary answer for most components, and the caller opens a socket.
    NotOffered,
    /// Anything else, with the reason, for the log.
    Refused(String),
}

impl std::fmt::Display for ViewError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotOffered => f.write_str("this component is not served as a one-shot render"),
            Self::Refused(e) => write!(f, "{e}"),
        }
    }
}

/// Fetch `GET /_eui/view/<component>?v=<version>` and walk it into frames.
///
/// Blocking, and called from where the manifest fetch already blocks. The
/// whole body is validated before a byte of it is handed on: a body that
/// half-applies cannot be recovered by opening a socket afterwards, because
/// the driver would already have mounted a piece of a tree.
#[cfg(has_native_net)]
pub fn fetch_view(url: &str, component: &str, version: u32, width: u32, cookie: Option<&str>) -> Result<View, ViewError> {
    use eui_proto::limits::{MAX_VIEW_BYTES, MAX_VIEW_FRAMES};

    // Normalised here rather than left to the caller: this is reachable from
    // a tool and from a typed address, and `https://host/...` is how a person
    // writes the thing `wss://host/...` names.
    let url = crate::assets::normalise_url(url);
    let origin = crate::assets::origin_for(&url).map_err(|e| ViewError::Refused(e.to_string()))?;
    // The width joins the cache key, because a view that derives its
    // measurements from the viewport renders differently at each one. A
    // server that declared breakpoints snaps this to one of them, so the
    // cache holds a handful of entries rather than one per reader; a server
    // that declared none ignores it and renders at its nominal width.
    let path = format!("/_eui/view/{component}?v={version}&w={width}");
    let got = match crate::assets::get_full(&origin, &path, VIEW_MEDIA_TYPE, cookie) {
        Ok(got) => got,
        Err(crate::assets::AssetError::Status(404)) => return Err(ViewError::NotOffered),
        Err(e) => return Err(ViewError::Refused(e.to_string())),
    };
    // A body that is not what was asked for is not a body to walk. An origin
    // that answers this path with a login page is the case worth refusing.
    if got.content_type.as_deref() != Some(VIEW_MEDIA_TYPE) {
        return Err(ViewError::Refused(format!("answered {} rather than {VIEW_MEDIA_TYPE}", got.content_type.unwrap_or_else(|| "nothing".into()))));
    }
    if got.body.len() > MAX_VIEW_BYTES {
        return Err(ViewError::Refused(format!("{} bytes is past the ceiling for one render", got.body.len())));
    }

    let mut frames = Vec::new();
    let mut at = 0usize;
    while at < got.body.len() {
        let rest = got.body.get(at..).unwrap_or(&[]);
        let used = eui_proto::Frame::framed_len(rest).map_err(|e| ViewError::Refused(format!("frame {}: {e:?}", frames.len())))?;
        let frame = rest.get(..used).ok_or_else(|| ViewError::Refused("the body ends inside a frame".into()))?;
        if frames.len() >= MAX_VIEW_FRAMES {
            return Err(ViewError::Refused(format!("more than {MAX_VIEW_FRAMES} frames")));
        }
        frames.push(frame.to_vec());
        at = at.saturating_add(used);
    }
    // The shape 01 §2.4 promises: a `Welcome`, then batches, and nothing
    // else. Checked by kind byte alone — the window does not read frames.
    match frames.split_first() {
        Some((welcome, batches)) if welcome.first() == Some(&0x02) && batches.iter().all(|b| b.first() == Some(&0x03)) && !batches.is_empty() => {
            let mut hasher = blake3::Hasher::new();
            for batch in batches {
                hasher.update(batch);
            }
            Ok(View { tree: *hasher.finalize().as_bytes(), etag: got.etag, frames })
        }
        _ => Err(ViewError::Refused("not a Welcome followed by batches".into())),
    }
}

/// The media type this endpoint speaks, sent as `Accept` and required back.
pub const VIEW_MEDIA_TYPE: &str = "application/vnd.eui.frames";

pub use crate::dial::kind_needs_server;

/// Connect, spawning the socket's runtime on a background thread. `first` is
/// sent as soon as the socket is open — the `Hello` frame.
pub fn connect(url: &str, first: Vec<u8>, cookie: Option<String>, host_loopback: bool, notify: impl Fn() + Send + Sync + 'static) -> Result<Connection, TransportError> {
    check_url(url, host_loopback)?;
    let origin = crate::assets::origin_for(url).map_err(|e| TransportError::Connect(e.to_string()))?;
    let url = url.to_owned();
    let (out_tx, mut out_rx) = tokio::sync::mpsc::unbounded_channel::<Vec<u8>>();
    let (in_tx, in_rx) = mpsc::channel::<Incoming>();
    let notify: std::sync::Arc<dyn Fn() + Send + Sync> = std::sync::Arc::new(notify);
    let in_tx_for_assets = in_tx.clone();
    let notify_for_thread = std::sync::Arc::clone(&notify);
    let notify = notify_for_thread.clone();
    let notify_thread = move || notify_for_thread();
    let ws_origin = origin.clone();
    let upgrade_cookie = cookie.clone();

    thread::Builder::new()
        .name("eui-transport".into())
        .spawn(move || {
            let notify = notify_thread;
            let rt = match tokio::runtime::Builder::new_current_thread().enable_all().build() {
                Ok(rt) => rt,
                Err(e) => {
                    let _ = in_tx.send(Incoming::Closed(TransportError::Connect(e.to_string())));
                    notify();
                    return;
                }
            };
            rt.block_on(async move {
                use tokio_tungstenite::tungstenite::client::IntoClientRequest;
                let request = match url.as_str().into_client_request() {
                    Ok(mut r) => {
                        // A host's cookie rides with an Origin that names the
                        // host itself: a cookie-bearing upgrade without one
                        // is what a cross-site pivot looks like, and the
                        // server refuses it (Soli SEC-046).
                        if let Some(cookie) = upgrade_cookie.as_ref().and_then(|c| c.parse().ok()) {
                            r.headers_mut().insert("Cookie", cookie);
                            if let Ok(o) = ws_origin.parse() {
                                r.headers_mut().insert("Origin", o);
                            }
                        }
                        r
                    }
                    Err(e) => {
                        let _ = in_tx.send(Incoming::Closed(TransportError::Connect(e.to_string())));
                        notify();
                        return;
                    }
                };
                let connector = tokio_tungstenite::Connector::Rustls(crate::transport::tls_config());
                let (ws, _) = match tokio_tungstenite::connect_async_tls_with_config(request, None, false, Some(connector)).await {
                    Ok(ok) => ok,
                    // An HTTP status is an answer, not a failure to reach
                    // anyone: the server understood the request and declined
                    // it. Carried through as itself so the window can stop
                    // rather than retry.
                    Err(tokio_tungstenite::tungstenite::Error::Http(response)) => {
                        let code = response.status().as_u16();
                        let why = response.into_body().and_then(|b| String::from_utf8(b).ok()).unwrap_or_default();
                        let _ = in_tx.send(Incoming::Closed(TransportError::Refused(code, why.trim().to_owned())));
                        notify();
                        return;
                    }
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
                // Outgoing: wait on the channel. This task sleeps until
                // there is something to send and costs nothing until then,
                // which is what an idle session should cost.
                //
                // It used to ask the channel whether it had anything and
                // sleep 2 ms when it had not. That is five hundred wake-ups
                // a second, per tab, for as long as a window is open —
                // measured as the *only* thread with any cost at all in an
                // idle client, 0.5 % of a core on Linux and far worse on a
                // platform with coarser timers. An idle window should be
                // asleep, not nearly asleep.
                let sender = tokio::spawn(async move {
                    while let Some(bytes) = out_rx.recv().await {
                        if sink.send(Message::Binary(bytes)).await.is_err() {
                            return;
                        }
                    }
                    // Every sender is gone: the session is over, and the
                    // server is told rather than left to notice.
                    let _ = sink.send(Message::Close(None)).await;
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

    Ok(Connection { tx: out_tx, rx: in_rx, origin, in_tx: in_tx_for_assets, notify, cookie })
}
