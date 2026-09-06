//! Random trees through layout at random viewports: no panic, finite rects,
//! and a second pass equal to the first.
#![no_main]
use libfuzzer_sys::fuzz_target;
use eui_layout::{Env, Layout, Monospace, Size};
use eui_proto::*;
use eui_theme::{Theme, Viewer};
use eui_tree::{Limits, Session};

fuzz_target!(|data: &[u8]| {
    let mut r = Reader::new(data);
    let Ok(w) = r.u16() else { return };
    let Ok(h) = r.u16() else { return };
    let mut session = Session::with_limits(Limits { max_nodes: 512, max_depth: 24, ..Default::default() });
    let mut seq = 0u64;
    while !r.is_empty() {
        let Ok(op) = Op::decode(&mut r) else { break };
        seq += 1;
        let _ = session.apply(&Batch { seq, ops: vec![op] });
    }
    let Some(root) = session.root() else { return };
    let theme = Theme::default().resolve(Viewer::default());
    let mut mono = Monospace::default();
    let mut layout = Layout::new();
    let size = Size::new(f32::from(w) / 4.0, f32::from(h) / 4.0);
    layout.compute(&mut Env { session: &session, theme: &theme, text: &mut mono }, size);
    let first: Vec<_> = session.preorder(root).map(|ix| layout.rect(ix)).collect();
    for rect in first.iter().flatten() {
        assert!(rect.x.is_finite() && rect.y.is_finite() && rect.w.is_finite() && rect.h.is_finite());
    }
    layout.compute(&mut Env { session: &session, theme: &theme, text: &mut mono }, size);
    let second: Vec<_> = session.preorder(root).map(|ix| layout.rect(ix)).collect();
    assert_eq!(first, second, "layout is deterministic");
});
