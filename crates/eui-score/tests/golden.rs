//! The Rust engraver against the Soli one it was ported from.
//!
//! `tests/golden/<name>.json` holds what `eui_builders_score.sl` drew for
//! `tests/corpus/<name>.ly`: its paths on a screen 900 px wide at 8 px a
//! staff space, each A4 page's at 600 px a page, its MIDI at 100 beats a
//! minute, and the screen's bands — a canvas each, a staff each in a system
//! of several; `<name>.pdf` its PDF. The paths must agree to the hundredth of a
//! pixel (one rounding step either way), the MIDI and the PDF to the byte,
//! and every refusal in `refusals.json` word for word.
//!
//! Since score #406 the two part where this engraver lays music out as
//! LilyPond does and the Soli engraver did not — every line of a score of
//! several filled to the margin, the last too, the line breaks chosen over
//! the whole score, a stem up before a stem down given more room, systems
//! of several staves spaced from the lower staff's middle line, a last page
//! not left with one system: the pieces that moved were looked at, picture
//! by picture, and their files written anew from this engraver
//! (`EUI_SCORE_BLESS=1 cargo test -p eui-score --test golden` writes the
//! pieces that differ, and leaves the others as they are). Since score #409
//! the distances down a page are LilyPond's springs, the ink kept apart
//! column by column rather than by its box: the pieces that moved were
//! looked at again, and written anew.

#![allow(clippy::indexing_slicing, clippy::arithmetic_side_effects)]

use std::fs;
use std::path::{Path as FsPath, PathBuf};

use eui_score::{bands, engrave, engrave_a4, midi, paths, pdf_outlines, Layout, MidiOptions, Paper, Path};
use serde_json::Value;

fn dir(sub: &str) -> PathBuf {
    FsPath::new(env!("CARGO_MANIFEST_DIR")).join("tests").join(sub)
}

/// A path as the golden files write it: `[kind, ink, numbers…]`.
fn as_numbers(p: &Path) -> (i64, Vec<f64>) {
    match p {
        Path::Rect(r) => (1, vec![r[0], r[1], r[2], r[3], 0.0]),
        Path::Fill(c) => (5, c.clone()),
    }
}

fn compare(what: &str, ours: &[Path], theirs: &Value, out: &mut Vec<String>) {
    let Some(theirs) = theirs.as_array() else {
        out.push(format!("{what}: no golden paths"));
        return;
    };
    if ours.len() != theirs.len() {
        out.push(format!("{what}: {} paths, the Soli engraver drew {}", ours.len(), theirs.len()));
    }
    for (k, (a, b)) in ours.iter().zip(theirs).enumerate() {
        let (kind, nums) = as_numbers(a);
        let b = b.as_array().cloned().unwrap_or_default();
        let b_kind = b.first().and_then(Value::as_i64).unwrap_or(-1);
        let b_nums: Vec<f64> = b.iter().skip(2).filter_map(Value::as_f64).collect();
        let same = kind == b_kind && nums.len() == b_nums.len() && nums.iter().zip(&b_nums).all(|(x, y)| (x - y).abs() <= 0.011);
        if !same {
            out.push(format!("{what}: path {k} differs\n  ours   {kind} {nums:?}\n  theirs {b_kind} {b_nums:?}"));
            return;
        }
    }
}

/// Where this engraver and the Soli one part, LilyPond siding with this one.
/// Three sing two voices under `\relative`, and LilyPond reads a voice's
/// first note from the last pitch of the voice before it
/// (`music_list_to_relative`), where the Soli engraver read it from the
/// pitch before `<<`; `tests/lilypond.rs` pins the pitches LilyPond gives.
/// In the fourth a `\bar "|."` is written in one staff of two, and a bar
/// line is the system's in LilyPond, drawn on both, where the Soli engraver
/// drew it on the staff that wrote it.
const PARTED: [&str; 4] = ["lyrics_choir", "piano_voices", "voices", "voices_spacers"];

/// A path as the golden files write it, its ink the default.
fn as_json(p: &Path) -> Value {
    let (kind, nums) = as_numbers(p);
    let mut v = vec![Value::from(kind), Value::from("text.default")];
    v.extend(nums.into_iter().map(Value::from));
    Value::Array(v)
}

/// `EUI_SCORE_BLESS=1 cargo test -p eui-score --test golden`: write what
/// this engraver draws for `name` as its golden files — for a change that
/// moves the drawing on purpose, its pictures looked at first.
fn bless(name: &str, src: &str) {
    let paths_of = |list: &[Path]| Value::Array(list.iter().map(as_json).collect());
    let screen = engrave(src, 900.0 / 8.0, &Layout::default()).and_then(|e| paths(&e, 8.0, 0)).map(|(p, _)| paths_of(&p)).unwrap_or(Value::Null);
    let bands = engrave(src, 900.0 / 8.0, &Layout::default())
        .and_then(|e| {
            let total = (e.height * 8.0).ceil() as i64;
            bands(&e, 8.0, 0, total)
        })
        .map(|got| Value::Array(got.iter().map(|b| serde_json::json!({ "y": b.y, "h": b.h, "paths": paths_of(&b.paths) })).collect()))
        .unwrap_or(Value::Null);
    let sp = 600.0 / Paper::a4().w;
    let a4 = engrave_a4(src).map(|eng| Value::Array((0..eng.pages.len()).map(|k| paths(&eng, sp, k).map(|(p, _)| paths_of(&p)).unwrap_or(Value::Null)).collect())).unwrap_or(Value::Null);
    let midi = midi(src, &MidiOptions { bpm: 100.0, ..MidiOptions::default() }).map(|b| Value::Array(b.into_iter().map(Value::from).collect())).unwrap_or(Value::Null);
    let doc = serde_json::json!({ "name": name, "screen": screen, "a4": a4, "midi": midi, "bands": bands });
    fs::write(dir("golden").join(format!("{name}.json")), doc.to_string()).unwrap_or_default();
    if let Ok(pdf) = pdf_outlines(src, "") {
        fs::write(dir("golden").join(format!("{name}.pdf")), pdf).unwrap_or_default();
    }
}

#[test]
fn the_corpus_draws_as_the_soli_engraver_drew_it() {
    let mut failures = Vec::new();
    let mut names: Vec<String> = fs::read_dir(dir("golden")).map(|rd| rd.filter_map(|e| e.ok()).map(|e| e.file_name().to_string_lossy().into_owned()).collect()).unwrap_or_default();
    names.retain(|n| n.ends_with(".json") && n != "refusals.json");
    names.sort();
    assert!(names.len() >= 19, "the golden files are missing");
    assert!(PARTED.iter().all(|p| names.iter().any(|n| n == &format!("{p}.json"))), "a parted piece is missing");
    for file in names {
        let name = file.trim_end_matches(".json");
        if PARTED.contains(&name) {
            continue;
        }
        let src = fs::read_to_string(dir("corpus").join(format!("{name}.ly"))).unwrap_or_default();
        let before = failures.len();
        let golden: Value = serde_json::from_str(&fs::read_to_string(dir("golden").join(&file)).unwrap_or_default()).unwrap_or(Value::Null);

        match engrave(&src, 900.0 / 8.0, &Layout::default()).and_then(|e| paths(&e, 8.0, 0)) {
            Ok((p, _)) => compare(&format!("{name} screen"), &p, &golden["screen"], &mut failures),
            Err(e) => failures.push(format!("{name} screen: refused: {e}")),
        }

        let laid = engrave(&src, 900.0 / 8.0, &Layout::default()).and_then(|e| {
            let total = (e.height * 8.0).ceil() as i64;
            bands(&e, 8.0, 0, total)
        });
        match laid {
            Ok(got) => {
                let want = golden["bands"].as_array().cloned().unwrap_or_default();
                if got.len() != want.len() {
                    failures.push(format!("{name} bands: {} bands, the Soli engraver cut {}", got.len(), want.len()));
                }
                for (k, (a, b)) in got.iter().zip(&want).enumerate() {
                    let (by, bh) = (b["y"].as_i64(), b["h"].as_i64());
                    if Some(a.y) != by || Some(a.h) != bh {
                        failures.push(format!("{name} band {k}: at {} px, {} px tall; the Soli engraver's at {by:?}, {bh:?}", a.y, a.h));
                        continue;
                    }
                    compare(&format!("{name} band {k}"), &a.paths, &b["paths"], &mut failures);
                }
            }
            Err(e) => failures.push(format!("{name} bands: refused: {e}")),
        }

        let sp = 600.0 / Paper::a4().w;
        match engrave_a4(&src) {
            Ok(eng) => {
                let pages = golden["a4"].as_array().cloned().unwrap_or_default();
                if pages.len() != eng.pages.len() {
                    failures.push(format!("{name} a4: {} pages, the Soli engraver made {}", eng.pages.len(), pages.len()));
                }
                for (k, page) in pages.iter().enumerate().take(eng.pages.len()) {
                    match paths(&eng, sp, k) {
                        Ok((p, _)) => compare(&format!("{name} a4 page {}", k + 1), &p, page, &mut failures),
                        Err(e) => failures.push(format!("{name} a4 page {}: {e}", k + 1)),
                    }
                }
            }
            Err(e) => failures.push(format!("{name} a4: refused: {e}")),
        }

        match midi(&src, &MidiOptions { bpm: 100.0, ..MidiOptions::default() }) {
            Ok(bytes) => {
                let want: Vec<u8> = golden["midi"].as_array().map(|a| a.iter().filter_map(Value::as_u64).map(|b| b as u8).collect()).unwrap_or_default();
                if bytes != want {
                    let at = bytes.iter().zip(&want).position(|(a, b)| a != b).unwrap_or(bytes.len().min(want.len()));
                    failures.push(format!("{name} midi: {} bytes against {}, first difference at {at}", bytes.len(), want.len()));
                }
            }
            Err(e) => failures.push(format!("{name} midi: refused: {e}")),
        }

        if let Ok(want) = fs::read_to_string(dir("golden").join(format!("{name}.pdf"))) {
            match pdf_outlines(&src, "") {
                Ok(doc) if doc == want => {}
                Ok(doc) => {
                    let at = doc.bytes().zip(want.bytes()).position(|(a, b)| a != b).unwrap_or(doc.len().min(want.len()));
                    let from = at.saturating_sub(60);
                    failures.push(format!(
                        "{name} pdf: {} bytes against {}, first difference at {at}:\n  ours   {:?}\n  theirs {:?}",
                        doc.len(),
                        want.len(),
                        doc.get(from..(at + 60).min(doc.len())).unwrap_or(""),
                        want.get(from..(at + 60).min(want.len())).unwrap_or("")
                    ));
                }
                Err(e) => failures.push(format!("{name} pdf: refused: {e}")),
            }
        }
        // Blessed: a piece that differs is written anew, the others left as they are.
        if failures.len() > before && std::env::var_os("EUI_SCORE_BLESS").is_some() {
            bless(name, &src);
            failures.truncate(before);
        }
    }
    assert!(failures.is_empty(), "{} differences:\n{}", failures.len(), failures.join("\n"));
}

/// What this engraver reads and the Soli one refuses: the LilyPond protocol
/// 11 adds (03 §1.4). `tests/lilypond.rs` holds what each draws. Two run
/// past a bar line: cut into tied notes here, as LilyPond's
/// Completion_heads_engraver cuts them; then a second verse, a mode, an odd
/// metre, a tuplet in a tuplet and three voices; and a clef in the middle
/// of a bar, drawn smaller before its note as LilyPond draws it; a header
/// field no title prints, passed over with a warning.
const BEYOND: [&str; 14] = [
    "{ \\clef tenor c'4 }",
    "{ c'1 \\repeat unfold 2 { c'4 } }",
    "{ << { c'4 } >> }",
    "{ << { c4 } \\\\ { \\clef bass d4 } >> }",
    "{ c'4 s4 }",
    "{ c'4 d'4 e'4 f'4 g'4 a'4 b'4 c''4 d''2 e''2 f''1 g''1 a''1 b''1 c'''1 c'1 c'1 c'1 c'1 c'1 c'1 c'1 c'1 c'1 c'1 c'1 c'1 c'1 c'1 c'2 c'1 }",
    "{ c'4 c'4 c'4 c'4 c'2. c'2 }",
    "{ c'4 } \\addlyrics { la } \\addlyrics { lo }",
    // A mode, an odd metre, a tuplet in a tuplet.
    "{ \\key c \\dorian c'4 }",
    "{ \\time 3/5 c'4 }",
    "{ \\tuplet 3/2 { \\tuplet 3/2 { c'8 d' e' } } }",
    "{ << { c'4 } \\\\ { d'4 } \\\\ { e'4 } >> }",
    "{ c'2 \\clef bass c2 }",
    "\\header { colour = \"red\" } { c'4 }",
];

/// What both refuse, this one saying more of why.
const REWORDED: [(&str, &str); 3] = [
    ("{ c'4 \\new Voice }", "score: \\new Voice wants its music (at character 6)"),
    ("{ c'7 }", "score: the duration 7 is not one this draws (1, 2, 4, 8, 16, 32, 64, 128) (at character 4)"),
    (
        "\\new Foo { c'4 }",
        "score: \\new Foo is not drawn: a Staff, a ChordNames, a FiguredBass, a FretBoards, a DrumStaff, a TabStaff, a PianoStaff, a GrandStaff, a StaffGroup or a ChoirStaff is (at character 0)",
    ),
];

#[test]
fn what_the_soli_engraver_refuses_this_refuses_in_the_same_words() {
    let all: Value = serde_json::from_str(&fs::read_to_string(dir("golden").join("refusals.json")).unwrap_or_default()).unwrap_or(Value::Null);
    let cases = all.as_array().cloned().unwrap_or_default();
    assert!(cases.len() >= 30, "the refusals are missing");
    let mut failures = Vec::new();
    for case in cases {
        let src = case["src"].as_str().unwrap_or_default();
        let mut want = case["error"].as_str().unwrap_or_default();
        if BEYOND.contains(&src) {
            want = "ACCEPTED";
        }
        if let Some((_, now)) = REWORDED.iter().find(|(s, _)| *s == src) {
            want = now;
        }
        match engrave(src, 900.0 / 8.0, &Layout::default()) {
            Ok(_) if want == "ACCEPTED" => {}
            Ok(_) => failures.push(format!("{src:?}: accepted, the Soli engraver said {want:?}")),
            Err(e) if e.message == want => {}
            Err(e) => failures.push(format!("{src:?}:\n  ours   {}\n  theirs {want}", e.message)),
        }
    }
    assert!(failures.is_empty(), "{} differences:\n{}", failures.len(), failures.join("\n"));
}
