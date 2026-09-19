//! Render the shell's chrome off-screen, so it can be looked at without a
//! display — the same tree, layout, text and paint path a window uses.
//!
//! `chrome_shot <out_dir> <w> <h> <scale>`; writes `chrome-light.rgba` and
//! `chrome-dark.rgba`.

// A development tool: a panic here is a failed render, which is what one
// wants to see. The same allowance the snapshot binary beside it takes.
#![allow(clippy::expect_used, clippy::arithmetic_side_effects, clippy::indexing_slicing)]

use eui_client::chrome::{Chrome, TabView, Trust};
use eui_render::Renderer;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let dir = args.first().cloned().unwrap_or_else(|| ".".into());
    let w: f32 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(900.0);
    let h: f32 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(560.0);
    let scale: f32 = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(2.0);
    let blank = std::env::var("CHROME_BLANK").is_ok();

    let mut renderer = Renderer::new_headless().expect("a GPU adapter");
    let mut textures = renderer.session();

    let tabs = [
        ("Vitrine", "wss://vitrine.solisoft.net", "/_eui/session/gallery", Trust::Pinned),
        ("Needle", "wss://needle.solisoft.net", "/_eui/session/music", Trust::Pinned),
        ("Feedx", "ws://127.0.0.1:5090", "/_eui/session/feed", Trust::Local),
        ("Tracker", "wss://tracker.solisoft.net", "/_eui/session/tracker", Trust::Pinned),
    ];

    for (name, mode) in [("light", eui_proto::ThemeMode::Light), ("dark", eui_proto::ThemeMode::Dark)] {
        let mut chrome = Chrome::new(w, h, scale);
        chrome.set_desktop_theme(Some(mode), Vec::new());
        if blank {
            chrome.set_recents(
                [
                    ("demo-app", "wss://eui-data.solisoft.test/_eui/session/gallery"),
                    ("demo-app", "wss://eui-data.solisoft.test/_eui/session/music"),
                    ("demo-app", "wss://eui-data.solisoft.test/_eui/session/tracker"),
                ]
                .iter()
                .map(|(n, u)| eui_client::recent::Recent { url: (*u).to_owned(), name: (*n).to_owned() })
                .collect(),
            );
            // CHROME_PICK=<n> — the keyboard standing on the nth recent, as
            // ArrowDown leaves it.
            let pick: usize = std::env::var("CHROME_PICK").ok().and_then(|v| v.parse().ok()).unwrap_or(0);
            chrome.rebuild(&[TabView { title: "New tab", origin: "", path: "", trust: None, link: None, can_back: false, can_forward: false, grants: None, installed: None }], 0);
            for _ in 0..pick {
                let _ = chrome.input(eui_client::Input::Key { key: "ArrowDown".into(), modifiers: 0, down: true });
                chrome.rebuild(&[TabView { title: "New tab", origin: "", path: "", trust: None, link: None, can_back: false, can_forward: false, grants: None, installed: None }], 0);
            }
        } else {
            let views: Vec<TabView<'_>> = tabs
                .iter()
                .map(|(t, o, p, tr)| TabView {
                    title: t,
                    origin: o,
                    path: p,
                    trust: Some(*tr),
                    link: None,
                    can_back: std::env::var("CHROME_BACK").is_ok(),
                    can_forward: std::env::var("CHROME_FWD").is_ok(),
                    // CHROME_GRANTS — the padlock, for a plate that wants it.
                    grants: std::env::var("CHROME_GRANTS").is_ok().then_some(eui_proto::caps::CAMERA),
                    // CHROME_INSTALL=in|out — the launcher control, which
                    // is absent for an application that publishes no icon.
                    installed: match std::env::var("CHROME_INSTALL").as_deref() {
                        Ok("in") => Some(true),
                        Ok("out") => Some(false),
                        _ => None,
                    },
                })
                .collect();
            if std::env::var("CHROME_EDIT").is_ok() {
                chrome.edit_address();
            }
            chrome.rebuild(&views, 0);
        }
        let (dw, dh) = ((w * scale) as u32, (h * scale) as u32);
        let list = chrome.paint(dw, dh);
        let target = renderer.offscreen(dw, dh);
        let (atlas, images) = chrome.atlases_mut();
        renderer.render_offscreen(&mut textures, &target, 0.0, &list, atlas, images);
        let px = renderer.read_back(&target).expect("read back");
        let path = format!("{dir}/chrome-{mode:?}.rgba", mode = if mode == eui_proto::ThemeMode::Dark { "dark" } else { "light" });
        let _ = name;
        std::fs::write(&path, &px).expect("write");
        println!("{path} {dw}x{dh} {} bytes, {} quads", px.len(), list.quads.len());
    }
}
