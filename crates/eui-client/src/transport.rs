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
    pub tx: mpsc::Sender<Vec<u8>>,
    /// Incoming messages; the receiver end belongs to the caller.
    pub rx: mpsc::Receiver<Incoming>,
    /// The HTTPS origin assets come from.
    pub origin: String,
    in_tx: mpsc::Sender<Incoming>,
    notify: std::sync::Arc<dyn Fn() + Send + Sync>,
}

impl std::fmt::Debug for Connection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Connection").field("origin", &self.origin).finish()
    }
}

impl Connection {
    /// Fetch an asset on a worker thread; the result arrives as
    /// [`Incoming::Asset`] and the notifier is called.
    pub fn request_asset(&self, hash: [u8; 32]) {
        let origin = self.origin.clone();
        let tx = self.in_tx.clone();
        let notify = std::sync::Arc::clone(&self.notify);
        let _ = thread::Builder::new().name("eui-asset".into()).spawn(move || {
            let result = crate::assets::fetch(&origin, &hash).map_err(|e| e.to_string());
            let _ = tx.send(Incoming::Asset(hash, result));
            notify();
        });
    }
}

/// Set by an embedding host — a desktop artifact whose server and client
/// are one process — to allow `ws://` on loopback in a release build.
static HOST_LOOPBACK: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
/// A cookie an embedding host asks the client to present on every request,
/// so the host's loopback gate lets the session through.
static SESSION_COOKIE: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

/// Spec 08 §1: `ws://` on loopback is acceptable when the client and the
/// server are the same trusted process. Only a host that embeds this crate
/// can say so; the `eui` binary never does.
pub fn allow_host_loopback() {
    HOST_LOOPBACK.store(true, std::sync::atomic::Ordering::SeqCst);
}

/// The cookie to present, `name=value`, if a host set one.
pub fn set_session_cookie(cookie: Option<String>) {
    if let Ok(mut c) = SESSION_COOKIE.lock() {
        *c = cookie;
    }
}

/// See [`set_session_cookie`].
pub fn session_cookie() -> Option<String> {
    SESSION_COOKIE.lock().ok().and_then(|c| c.clone())
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
pub fn check_url(url: &str) -> Result<(), TransportError> {
    if url.starts_with("wss://") {
        return Ok(());
    }
    let loopback = url.starts_with("ws://127.0.0.1") || url.starts_with("ws://localhost") || url.starts_with("ws://[::1]");
    // Loopback only, and only when asked by name: a developer running a
    // local Soli should get to use the fast build of the client too.
    let asked = std::env::var("EUI_ALLOW_INSECURE_LOOPBACK").as_deref() == Ok("1");
    let allowed = loopback && (HOST_LOOPBACK.load(std::sync::atomic::Ordering::SeqCst) || asked);
    if allowed && asked && !cfg!(debug_assertions) {
        eprintln!("eui: EUI_ALLOW_INSECURE_LOOPBACK=1 — plain ws:// on loopback, nothing is encrypted");
    }
    if allowed {
        Ok(())
    } else {
        Err(TransportError::Insecure(url.to_owned()))
    }
}

/// Connect, spawning the socket's runtime on a background thread. `first` is
/// sent as soon as the socket is open — the `Hello` frame.
pub fn connect(url: &str, first: Vec<u8>, notify: impl Fn() + Send + Sync + 'static) -> Result<Connection, TransportError> {
    check_url(url)?;
    let origin = crate::assets::origin_for(url).map_err(|e| TransportError::Connect(e.to_string()))?;
    let url = url.to_owned();
    let (out_tx, out_rx) = mpsc::channel::<Vec<u8>>();
    let (in_tx, in_rx) = mpsc::channel::<Incoming>();
    let notify: std::sync::Arc<dyn Fn() + Send + Sync> = std::sync::Arc::new(notify);
    let in_tx_for_assets = in_tx.clone();
    let notify_for_thread = std::sync::Arc::clone(&notify);
    let notify = notify_for_thread.clone();
    let notify_thread = move || notify_for_thread();
    let ws_origin = origin.clone();

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
                        if let Some(cookie) = session_cookie().and_then(|c| c.parse().ok()) {
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

    Ok(Connection { tx: out_tx, rx: in_rx, origin, in_tx: in_tx_for_assets, notify })
}
