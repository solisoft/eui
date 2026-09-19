//! The EUI client as a page: one canvas, one chromeless session.
//!
//! This crate is the whole of what JavaScript can see, and it is
//! deliberately three functions wide. Everything it does is in
//! `eui-client`; what is here is the argument checking that a page should
//! get an exception for rather than a canvas that never draws.
//!
//! It exists as its own crate for the reason `eui-android` and `eui-ios`
//! do: the entry point a platform loads is a different *kind* of artefact
//! from the binary the client builds, and `eui-client` has a `[[bin]]`.
//! Making that crate a `cdylib` to serve one target would change its link
//! kind on all five.

#![cfg(target_arch = "wasm32")]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

/// Open `url` in the `<canvas>` with id `canvas`, granting `allow`.
///
/// `allow` is the comma-separated capability list of spec 01 §2.1 — the
/// same grammar as the `eui` binary's `--allow` and `snapshot`'s
/// `SNAPSHOT_ALLOW`, parsed by the same `eui_proto::caps::from_name`, so a
/// page and a command line cannot drift. `""` grants nothing, which is what
/// a demo on a documentation page should ask for. A name that is not a
/// capability is an error rather than a warning: a page that asked for
/// `clipbord` and got silence would read as a client that ignores its
/// embed.
///
/// Returns as soon as the loop is the browser's, so `Ok` means *started*.
/// Everything that can be refused synchronously is refused here — a missing
/// canvas, a URL that is not `wss://`, a capability nobody has heard of —
/// so the page gets a thrown error it can show instead of a rectangle that
/// stays empty. What cannot be refused here is the GPU: `navigator.gpu` and
/// WebGL2 are asked asynchronously, and a browser that answers neither says
/// so on the console one turn of the loop later.
#[wasm_bindgen]
pub fn start(canvas: &str, url: &str, allow: &str) -> Result<(), JsValue> {
    // Before anything else, so that a panic during argument checking is
    // also a message rather than `unreachable executed`.
    console_error_panic_hook::set_once();

    let element = web_sys::window().and_then(|w| w.document()).and_then(|d| d.get_element_by_id(canvas)).ok_or_else(|| JsValue::from_str(&format!("eui: no element with id {canvas:?}")))?;
    let element: web_sys::HtmlCanvasElement = element.dyn_into().map_err(|_| JsValue::from_str(&format!("eui: {canvas:?} is not a <canvas>")))?;

    let mut allowed = 0u32;
    for name in allow.split(',').map(str::trim).filter(|n| !n.is_empty()) {
        allowed |= eui_proto::caps::from_name(name).ok_or_else(|| JsValue::from_str(&format!("eui: no capability called {name:?}")))?;
    }

    // 01 §1, before a socket is opened. There is no loopback exception in
    // a page — `EUI_ALLOW_INSECURE_LOOPBACK` cannot be set where there is
    // no environment — so this refuses everything but `wss://` on its own.
    // The page may hand this an `https://` address — it is the one the page
    // itself was served from — and that names the same origin as `wss://`.
    let url = eui_client::normalise_url(url);
    let url = url.as_str();
    eui_client::check_url(url, false).map_err(|e| JsValue::from_str(&format!("eui: {e}")))?;

    eui_client::web::start(element);
    // One chromeless session: no tab strip, no address to type into. A
    // page embeds an application, and the page is the shell.
    eui_client::app::launch(eui_client::app::Launch::new(url.to_owned(), allowed)).map_err(|e| JsValue::from_str(&format!("eui: {e}")))
}

/// The capability names [`start`] will accept, comma-separated.
///
/// From `caps::NAMES` rather than written out here, for the reason the
/// binary's usage line is: a list that omits the one somebody needs is
/// worse than none, because it reads as everything there is.
#[wasm_bindgen]
pub fn capabilities() -> String {
    eui_proto::caps::NAMES.iter().map(|(n, _)| *n).collect::<Vec<_>>().join(",")
}

/// Build a text engine and say so. A bisecting probe, called from the
/// page: it is the first thing on the session path that does real work —
/// four embedded faces through `fontdb` — and the first thing to suspect
/// when a page stops answering.
#[wasm_bindgen]
pub fn probe_text() -> String {
    let mut engine = eui_text::TextEngine::new();
    let spec = eui_layout::FontSpec { family: eui_proto::FontFamily::Sans, weight: eui_proto::FontWeight::Regular, size: 16.0, line_height: 22.0 };
    let shaped = engine.shape("probe", spec, None, 0);
    format!("text engine up; {} glyph(s) for \"probe\"", shaped.glyphs.len())
}

/// Build a driver and say so. Everything a session decodes, lays out and
/// paints hangs off this, and none of it touches a GPU or a socket — so a
/// page that gets past here has a problem in the window, not the client.
#[wasm_bindgen]
pub fn probe_driver() -> String {
    let driver = eui_client::Driver::new(800.0, 600.0, 2.0, 0);
    format!("driver up; {driver:?}").chars().take(80).collect()
}

/// Which build this is: the short commit, as the window title says it on a
/// desktop. A demo from the wrong run looks exactly like the right one.
#[wasm_bindgen]
pub fn version() -> String {
    eui_client::BUILD.to_owned()
}
