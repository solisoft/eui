//! Spec 03 §1.4 and 06 §1.5: music the client engraves (09 §13).
//!
//! The server sends LilyPond source; the client lays it out with the engraver
//! the catalogue's `score()` uses, draws exactly what that engraver draws,
//! and reports back the two facts only it now has — where in the music a
//! press landed, and what the engraving came to.
#![allow(clippy::indexing_slicing, clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::arithmetic_side_effects)]

use eui_client::{Driver, Input};
use eui_proto::*;

const A_SRC: u32 = 1;
const A_SP: u32 = 2;
const A_PICK: u32 = 3;
const A_INFO: u32 = 4;
const A_MARKS: u32 = 5;
const A_PATHS: u32 = 6;
const A_PAGE: u32 = 7;
const A_PAGE_H: u32 = 8;
const A_PAGE_I: u32 = 9;

const W: f32 = 600.0;
const FRERE: &str = include_str!("../../eui-score/tests/corpus/frere_jacques.ly");
const BEETHOVEN: &str = include_str!("../../eui-score/tests/corpus/beethoven24.ly");

/// `src` as the wire carries it: one string, or chunks of at most 4 KiB
/// read as their concatenation (03 §1.4, 02 §4.4).
fn src_value(src: &str) -> Value {
    if src.len() <= 4096 {
        return Value::Str(src.to_owned());
    }
    let mut parts = Vec::new();
    let mut rest = src;
    while !rest.is_empty() {
        let mut cut = rest.len().min(4096);
        while !rest.is_char_boundary(cut) {
            cut -= 1;
        }
        parts.push(Value::Str(rest[..cut].to_owned()));
        rest = &rest[cut..];
    }
    Value::List(parts)
}

/// A page: a column `W` wide holding one `score` (node 2) with `props`, and
/// handlers for both of its events.
fn page(props: Vec<(u32, Value)>) -> Batch {
    let mut t = Subtree::default();
    t.nodes.push(FlatNode { kind: NodeKind::Box, id: 1, style: 1, key: 0, text: None, props: (0, 0), handlers: (0, 0), child_count: 1 });
    let n = props.len() as u32;
    t.nodes.push(FlatNode { kind: NodeKind::Score, id: 2, style: 0, key: 0, text: None, props: (0, n), handlers: (0, 2), child_count: 0 });
    t.props = props;
    t.handlers.push((EventKind::ScorePick, Handler::Server(A_PICK)));
    t.handlers.push((EventKind::ScoreInfo, Handler::Server(A_INFO)));
    Batch {
        seq: 1,
        ops: vec![
            Op::DefAtom { id: A_SRC, value: "src".into() },
            Op::DefAtom { id: A_SP, value: "staff_space".into() },
            Op::DefAtom { id: A_PICK, value: "pick".into() },
            Op::DefAtom { id: A_INFO, value: "info".into() },
            Op::DefAtom { id: A_MARKS, value: "marks".into() },
            Op::DefAtom { id: A_PATHS, value: "paths".into() },
            Op::DefAtom { id: A_PAGE, value: "page".into() },
            Op::DefAtom { id: A_PAGE_H, value: "page_height".into() },
            Op::DefAtom { id: A_PAGE_I, value: "page_index".into() },
            Op::DefStyle { id: 1, record: StyleRecord { display: Display::Column, width: Dim::Px(W as u16), ..Default::default() } },
            Op::Mount(t),
        ],
    }
}

fn driver(props: Vec<(u32, Value)>) -> Driver {
    let mut d = Driver::new(W, 900.0, 1.0, 0);
    d.handle_frame(Frame::Welcome(Welcome { version: PROTOCOL_VERSION, session: [0u8; 16], start: Start::Fresh }));
    assert_eq!(d.handle_frame(Frame::Batch(page(props))), vec![Frame::Ack { seq: 1 }]);
    d
}

fn set(d: &mut Driver, seq: u64, atom: u32, value: Value) {
    let out = d.handle_frame(Frame::Batch(Batch { seq, ops: vec![Op::SetProp { node: 2, prop: atom, value }] }));
    assert_eq!(out, vec![Frame::Ack { seq }]);
}

/// The events of `kind` that went out, from input or from a paint.
fn sent(frames: &[Frame], kind: EventKind) -> Vec<Value> {
    frames
        .iter()
        .filter_map(|f| match f {
            Frame::Event(e) if e.event == kind => Some(e.payload.clone()),
            _ => None,
        })
        .collect()
}

fn painted(d: &mut Driver) -> Vec<Frame> {
    d.paint(W as u32, 900);
    std::mem::take(d.pending_mut())
}

fn score_rect(d: &Driver) -> eui_layout::Rect {
    let ix = d.session().lookup(2).unwrap();
    d.layout().rect(ix).unwrap()
}

/// The catalogue's paths for a source, as lang's `eui_score_bands` writes
/// them: the reference a client's drawing is held to.
fn catalogue_paths(src: &str, sp: f64, width: f32) -> Vec<Value> {
    let eng = eui_score::engrave(src, f64::from(width) / sp, &eui_score::Layout::default()).unwrap();
    let (paths, _) = eui_score::paths(&eng, sp, 0).unwrap();
    let ink = Value::Str("text.default".into());
    paths
        .iter()
        .map(|p| match p {
            eui_score::Path::Rect([x, y, w, h]) => Value::List(vec![Value::Int(1), ink.clone(), Value::Float(*x), Value::Float(*y), Value::Float(*w), Value::Float(*h), Value::Int(0)]),
            eui_score::Path::Fill(cmds) => {
                let mut out = vec![Value::Int(5), ink.clone()];
                let mut k = 0;
                while k < cmds.len() {
                    let c = cmds[k];
                    out.push(Value::Int(c as i64));
                    let args = match c as i64 {
                        0 | 1 => 2,
                        2 => 4,
                        3 => 6,
                        _ => 0,
                    };
                    out.extend(cmds.iter().skip(k + 1).take(args).map(|v| Value::Float(*v)));
                    k += 1 + args;
                }
                Value::List(out)
            }
        })
        .collect()
}

#[test]
fn the_score_kind_and_its_events_round_trip() {
    assert_eq!(NodeKind::from_u8(0x12).unwrap(), NodeKind::Score);
    assert_eq!(NodeKind::Score.to_u8(), 0x12);
    assert!(NodeKind::Score.is_leaf());
    assert_eq!(EventKind::from_u8(0x22).unwrap(), EventKind::ScorePick);
    assert_eq!(EventKind::from_u8(0x23).unwrap(), EventKind::ScoreInfo);
    let pick = Value::List(vec![
        Value::Int(0),
        Value::Int(0),
        Value::Int(0),
        Value::Int(0),
        Value::Int(1),
        Value::Int(-3),
        Value::Int(4),
        Value::Float(0.2),
        Value::Int(4),
        Value::Int(5),
        Value::Float(0.5),
        Value::Float(10.0),
        Value::Int(0),
    ]);
    assert!(EventKind::ScorePick.payload_fits(&pick));
    let mut bad = pick.clone();
    if let Value::List(v) = &mut bad {
        v[0] = Value::Int(3);
    }
    assert!(!EventKind::ScorePick.payload_fits(&bad), "a phase 06 §1.5 does not name");
    let starts = Value::List(vec![Value::List(vec![Value::Int(0), Value::Int(-1)]), Value::List(vec![Value::Int(12), Value::Int(30)])]);
    assert!(EventKind::ScoreInfo.payload_fits(&Value::List(vec![Value::Int(2), Value::Str(String::new()), starts.clone()])));
    assert!(!EventKind::ScoreInfo.payload_fits(&Value::List(vec![Value::Int(0), Value::Str(String::new()), starts])), "no music takes no pages");
}

#[test]
fn a_score_is_drawn_from_the_catalogues_paths() {
    // The same paths on a `canvas` the size the score laid itself out at,
    // and the two pictures must be one.
    let mut d = driver(vec![(A_SRC, src_value(FRERE)), (A_SP, Value::Int(8))]);
    let mine = d.paint(W as u32, 900);
    let r = score_rect(&d);
    assert!(r.h > 0.0, "the score took no height");

    let mut t = Subtree::default();
    t.nodes.push(FlatNode { kind: NodeKind::Box, id: 1, style: 1, key: 0, text: None, props: (0, 0), handlers: (0, 0), child_count: 1 });
    t.nodes.push(FlatNode { kind: NodeKind::Canvas, id: 2, style: 2, key: 0, text: None, props: (0, 1), handlers: (0, 0), child_count: 0 });
    t.props.push((A_PATHS, Value::List(catalogue_paths(FRERE, 8.0, W))));
    let mut c = Driver::new(W, 900.0, 1.0, 0);
    c.handle_frame(Frame::Welcome(Welcome { version: PROTOCOL_VERSION, session: [0u8; 16], start: Start::Fresh }));
    c.handle_frame(Frame::Batch(Batch {
        seq: 1,
        ops: vec![
            Op::DefAtom { id: A_PATHS, value: "paths".into() },
            Op::DefStyle { id: 1, record: StyleRecord { display: Display::Column, width: Dim::Px(W as u16), ..Default::default() } },
            Op::DefStyle { id: 2, record: StyleRecord { width: Dim::Px(r.w as u16), height: Dim::Px(r.h as u16), ..Default::default() } },
            Op::Mount(t),
        ],
    }));
    let theirs = c.paint(W as u32, 900);
    assert!(!mine.quads.is_empty());
    assert_eq!(mine.quads, theirs.quads, "a client-engraved score draws what the catalogue draws");
}

#[test]
fn a_scores_height_is_the_page_it_draws() {
    let mut d = driver(vec![(A_SRC, src_value(FRERE)), (A_SP, Value::Int(8))]);
    d.paint(W as u32, 900);
    let eng = eui_score::engrave(FRERE, f64::from(W) / 8.0, &eui_score::Layout::default()).unwrap();
    assert_eq!(score_rect(&d).h, (eng.height * 8.0).ceil() as f32, "a screen: the whole engraving");
    assert_eq!(score_rect(&d).w, W, "and the width it is given");

    set(&mut d, 2, A_PAGE, Value::Str("a4".into()));
    d.paint(W as u32, 900);
    let sp = f64::from(W) / eui_score::Paper::a4().w;
    assert_eq!(score_rect(&d).h, (eui_score::Paper::a4().h * sp).floor() as f32, "A4: the sheet, width × 297 / 210");
}

#[test]
fn only_what_moves_the_music_engraves_it_again() {
    let mut d = driver(vec![(A_SRC, src_value(FRERE)), (A_SP, Value::Int(8))]);
    painted(&mut d);
    let once = d.scores_engraved();
    assert_eq!(once, 1);
    // What only redraws.
    set(&mut d, 2, A_MARKS, Value::List(vec![Value::List(vec![Value::Int(0), Value::Str("accent.base".into())])]));
    painted(&mut d);
    set(&mut d, 3, A_PAGE_I, Value::Int(0));
    painted(&mut d);
    assert_eq!(d.scores_engraved(), once, "marks and page_index redraw from the engraving kept");
    // What moves the music.
    set(&mut d, 4, A_SRC, src_value("\\relative c' { c4 d e f | g1 }"));
    painted(&mut d);
    assert_eq!(d.scores_engraved(), once + 1, "a new source engraves once");
    set(&mut d, 5, A_SP, Value::Int(10));
    painted(&mut d);
    assert_eq!(d.scores_engraved(), once + 2, "a new staff space engraves once");
}

#[test]
fn a_refused_score_draws_nothing_and_says_why() {
    let mut d = driver(vec![(A_SRC, src_value("\\relative c' { c4 d %%% }} ")), (A_SP, Value::Int(8))]);
    let frames = painted(&mut d);
    let infos = sent(&frames, EventKind::ScoreInfo);
    assert_eq!(infos.len(), 1);
    let Value::List(info) = &infos[0] else { panic!() };
    let Value::Str(why) = &info[1] else { panic!() };
    assert!(!why.is_empty(), "the refusal is said");
    assert_eq!(score_rect(&d).h, 0.0, "and nothing is drawn");

    let huge = format!("\\relative c' {{ {} }}", "c4 ".repeat(200_000));
    let mut d = driver(vec![(A_SRC, src_value(&huge[..huge.len().min(4096 * 1024)]))]);
    let frames = painted(&mut d);
    let Value::List(info) = &sent(&frames, EventKind::ScoreInfo)[0] else { panic!() };
    assert!(matches!(&info[1], Value::Str(s) if s.contains("512 KiB")), "past 512 KiB, refused unread: {info:?}");
}

#[test]
fn score_info_is_sent_once_an_engraving() {
    let mut d = driver(vec![(A_SRC, src_value(BEETHOVEN)), (A_SP, Value::Int(6)), (A_PAGE_H, Value::Int(400))]);
    let first = sent(&painted(&mut d), EventKind::ScoreInfo);
    assert_eq!(first.len(), 1);
    let Value::List(info) = &first[0] else { panic!() };
    assert!(matches!(info[0], Value::Int(n) if n > 1), "a 13-staff piece paged at 400 px takes pages: {info:?}");
    assert_eq!(info[1], Value::Str(String::new()));
    // A list a page, a number a staff: page 0 starts every staff at its
    // first item, and each later page starts later.
    let Value::List(starts) = &info[2] else { panic!("starts: {:?}", info[2]) };
    let Value::Int(pages) = info[0] else { panic!() };
    assert_eq!(starts.len() as i64, pages);
    let staff0: Vec<i64> = starts
        .iter()
        .map(|p| match p {
            Value::List(s) => match s.first() {
                Some(Value::Int(n)) => *n,
                _ => -9,
            },
            _ => -9,
        })
        .collect();
    assert_eq!(staff0[0], 0, "the first page starts at the first note");
    assert!(staff0.windows(2).all(|w| w[1] > w[0]), "and the pages go on through the music: {staff0:?}");
    assert!(sent(&painted(&mut d), EventKind::ScoreInfo).is_empty(), "the same engraving is not reported twice");
    set(&mut d, 2, A_PAGE_I, Value::Int(1));
    assert!(sent(&painted(&mut d), EventKind::ScoreInfo).is_empty(), "a page turned is a redraw, not an engraving");
}

#[test]
fn score_pick_names_the_music_under_the_pointer() {
    let mut d = driver(vec![(A_SRC, src_value(FRERE)), (A_SP, Value::Int(8))]);
    painted(&mut d);
    let r = score_rect(&d);
    // The first note's head, from the engraving the client made.
    let eng = eui_score::engrave(FRERE, f64::from(W) / 8.0, &eui_score::Layout::default()).unwrap();
    let sys = &eng.systems[0];
    let first = eng.staves[0][sys.lines[0].bars.clone()].iter().flat_map(|b| b.items.iter()).next().unwrap();
    let mid = first.x + first.head_w.unwrap_or(1.18) / 2.0;
    let head_pos = first.heads.first().map_or(8, |h| h.pos);
    let (x, y) = (r.x + (mid * 8.0) as f32, r.y + ((sys.top + head_pos as f64 / 2.0) * 8.0) as f32);
    d.input(Input::PointerMove(x, y));
    let down = sent(&d.input(Input::PointerDown(0)), EventKind::ScorePick);
    assert_eq!(down.len(), 1, "a press on the music is a score_pick");
    let Value::List(p) = &down[0] else { panic!() };
    assert_eq!(p[0], Value::Int(0), "phase: down");
    assert_eq!(p[6], Value::Int(first.i as i64), "near: the note under the pointer");
    assert!(matches!(p[7], Value::Float(g) if (0.0..0.5).contains(&g)), "gap: under half a space, {:?}", p[7]);
    assert_eq!(p[5], Value::Int(head_pos), "pos: its line or space");
    assert_eq!(p[4], Value::Int(0), "staff: the only one");

    // Between the first two notes: before, after and how far between.
    let second = eng.staves[0][sys.lines[0].bars.clone()].iter().flat_map(|b| b.items.iter()).nth(1).unwrap();
    let mid2 = second.x + second.head_w.unwrap_or(1.18) / 2.0;
    let between = r.x + (((mid + mid2) / 2.0) * 8.0) as f32;
    d.input(Input::PointerMove(between, y));
    let moved = sent(&painted(&mut d), EventKind::ScorePick);
    assert_eq!(moved.len(), 1, "a move while pressed is one score_pick a frame");
    let Value::List(p) = &moved[0] else { panic!() };
    assert_eq!(p[0], Value::Int(1));
    assert_eq!((p[8].clone(), p[9].clone()), (Value::Int(first.i as i64), Value::Int(second.i as i64)));
    assert!(matches!(p[10], Value::Float(f) if (f - 0.5).abs() < 0.05), "half way: {:?}", p[10]);
    let up = sent(&d.input(Input::PointerUp(0)), EventKind::ScorePick);
    assert!(matches!(&up[..], [Value::List(p)] if p[0] == Value::Int(2)), "and the release");

    // A score with no music answers nothing.
    let mut refused = driver(vec![(A_SRC, src_value("}}}"))]);
    painted(&mut refused);
    refused.input(Input::PointerMove(5.0, 5.0));
    assert!(sent(&refused.input(Input::PointerDown(0)), EventKind::ScorePick).is_empty());
}

/// 10 §1 "Scores": the largest piece the catalogue ships — the score
/// editor's Beethoven exposition — engraved and painted by the client inside
/// a frame at 30 Hz. Timed in a release build; a debug build only reports.
#[test]
fn the_largest_piece_is_engraved_inside_a_frame() {
    let src = include_str!("../../../examples/demo-app/public/scores/beethoven-symphony-5-1.ly");
    // As the editor shows it: on a screen paged at 700 px, and on A4. One
    // endless page holding all 37 systems is reported and not held to the
    // frame: it is 14 371 paths, most of them below the window.
    for (what, props, held) in [
        ("screen, paged at 700 px", vec![(A_SRC, src_value(src)), (A_SP, Value::Int(12)), (A_PAGE_H, Value::Int(700))], true),
        ("a4", vec![(A_SRC, src_value(src)), (A_PAGE, Value::Str("a4".into()))], true),
        ("screen, one endless page", vec![(A_SRC, src_value(src)), (A_SP, Value::Int(12))], false),
    ] {
        let mut best = f64::MAX;
        for _ in 0..3 {
            let mut d = driver(props.clone());
            let t = std::time::Instant::now();
            d.paint(W as u32, 900);
            best = best.min(t.elapsed().as_secs_f64() * 1e3);
            assert_eq!(d.scores_engraved(), 1);
        }
        eprintln!("score timing: {what}: {} bytes engraved and painted in {best:.1} ms", src.len());
        if held && !cfg!(debug_assertions) {
            assert!(best < 33.0, "{what}: {best:.1} ms, past a frame at 30 Hz");
        }
    }
}
