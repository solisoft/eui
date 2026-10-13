//! The Rust engraver against the Soli one it was ported from.
//!
//! `tests/golden/<name>.json` holds what `eui_builders_score.sl` drew for
//! `tests/corpus/<name>.ly`: its paths on a screen 900 px wide at 8 px a
//! staff space, each A4 page's at 600 px a page, its MIDI at 100 beats a
//! minute, and the screen's bands — a canvas each, a staff each in a system
//! of several; `<name>.pdf` its PDF. The paths must agree to the hundredth of a
//! pixel (one rounding step either way), the MIDI and the PDF to the byte,
//! and every refusal in `refusals.json` word for word.

#![allow(clippy::indexing_slicing, clippy::arithmetic_side_effects)]

use std::fs;
use std::path::{Path as FsPath, PathBuf};

use eui_score::{bands, engrave, engrave_a4, midi, paths, pdf, Layout, MidiOptions, Paper, Path};
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

#[test]
fn the_corpus_draws_as_the_soli_engraver_drew_it() {
    let mut failures = Vec::new();
    let mut names: Vec<String> = fs::read_dir(dir("golden")).map(|rd| rd.filter_map(|e| e.ok()).map(|e| e.file_name().to_string_lossy().into_owned()).collect()).unwrap_or_default();
    names.retain(|n| n.ends_with(".json") && n != "refusals.json");
    names.sort();
    assert!(names.len() >= 19, "the golden files are missing");
    for file in names {
        let name = file.trim_end_matches(".json");
        let golden: Value = serde_json::from_str(&fs::read_to_string(dir("golden").join(&file)).unwrap_or_default()).unwrap_or(Value::Null);
        let src = fs::read_to_string(dir("corpus").join(format!("{name}.ly"))).unwrap_or_default();

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
            match pdf(&src, "") {
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
    }
    assert!(failures.is_empty(), "{} differences:\n{}", failures.len(), failures.join("\n"));
}

#[test]
fn what_the_soli_engraver_refuses_this_refuses_in_the_same_words() {
    let all: Value = serde_json::from_str(&fs::read_to_string(dir("golden").join("refusals.json")).unwrap_or_default()).unwrap_or(Value::Null);
    let cases = all.as_array().cloned().unwrap_or_default();
    assert!(cases.len() >= 30, "the refusals are missing");
    let mut failures = Vec::new();
    for case in cases {
        let src = case["src"].as_str().unwrap_or_default();
        let want = case["error"].as_str().unwrap_or_default();
        match engrave(src, 900.0 / 8.0, &Layout::default()) {
            Ok(_) if want == "ACCEPTED" => {}
            Ok(_) => failures.push(format!("{src:?}: accepted, the Soli engraver said {want:?}")),
            Err(e) if e.message == want => {}
            Err(e) => failures.push(format!("{src:?}:\n  ours   {}\n  theirs {want}", e.message)),
        }
    }
    assert!(failures.is_empty(), "{} differences:\n{}", failures.len(), failures.join("\n"));
}
