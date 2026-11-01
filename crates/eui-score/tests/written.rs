//! What a source writes out (score #540): a few characters of LilyPond that
//! expand into more music than any piece has — a repeat in a repeat in a
//! repeat, variables in variables, a rest counted a million times, a long
//! markup on every note of a repeat — are refused while they are read,
//! before the music is made, and in moments; every piece the score app
//! carries is read with room to spare.
//!
//! `tests/written/bombs/*.ly` are the sources the reviews of #540 sent; each
//! is refused, most by the budget. `tests/written/pieces/*.ly` are the score
//! app's largest pieces and house sections, and the ones that use what
//! multiplies (cues, endings, verses, empty staves); with `SCORE_REPO`
//! naming a checkout of the score app, every one of its pieces is read too.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing, clippy::panic, clippy::arithmetic_side_effects)]

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use eui_score::written::{Written, MAX_WRITTEN_EVENTS, MAX_WRITTEN_TEXT};
use eui_score::{engrave, engrave_a4, engrave_paper, midi, pdf, pdf_outlines, written_out, Layout, MidiOptions};

const NOTES: &str = "writes out to more than 20,000 notes; the engraver stops there";
const WORDS: &str = "writes out to more than 40,000 characters of words; the engraver stops there";

/// Well under a second, in a debug build.
const FAST: Duration = Duration::from_millis(800);

fn dir(sub: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join("written").join(sub)
}

fn sources(dir: &Path) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = fs::read_dir(dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "ly"))
        .map(|p| (p.file_name().unwrap().to_string_lossy().into_owned(), fs::read_to_string(&p).unwrap()))
        .collect();
    out.sort();
    out
}

/// The refusal of `src`, and how long it took to come.
fn refused(src: &str) -> (String, Duration) {
    let t = Instant::now();
    let r = written_out(src);
    let took = t.elapsed();
    match r {
        Ok(w) => panic!("accepted, writing out {w:?}: {src:.200}"),
        Err(e) => (e.message, took),
    }
}

/// `src` refused by the notes' budget, fast.
fn too_many_notes(src: &str) {
    let (message, took) = refused(src);
    assert!(message.contains(NOTES), "{message}: {src:.200}");
    assert!(took < FAST, "{took:?} to refuse {src:.200}");
}

fn accepted(src: &str) -> Written {
    written_out(src).unwrap_or_else(|e| panic!("{}: {src:.200}", e.message))
}

/// The sources the reviews sent are refused, each well under a second;
/// those that expand, by the budget.
#[test]
fn every_bomb_from_the_reviews_is_refused_fast() {
    let by_budget = [
        "bound.ly",
        "comma6.ly",
        "cr_star.ly",
        "lyrA.ly",
        "lyrB.ly",
        "lyrD.ly",
        "lyrhide.ly",
        "mark5.ly",
        "mark6.ly",
        "markup_rep.ly",
        "nobrace.ly",
        "plain5.ly",
        "qchord.ly",
        "qchord2.ly",
        "simul5.ly",
        "tab_star.ly",
        "withblk.ly",
        "withblk2.ly",
    ];
    let bombs = sources(&dir("bombs"));
    assert!(bombs.len() >= 50, "the bombs are in tests/written/bombs");
    for (name, src) in &bombs {
        let (message, took) = refused(src);
        assert!(took < FAST, "{name}: {took:?} to refuse");
        // `x_*`: the second review's — markings stacked on notes, slurs,
        // voices, a long short name — all past the budget.
        if by_budget.contains(&name.as_str()) || name.starts_with("x_") {
            assert!(message.contains(NOTES) || message.contains(WORDS), "{name}: {message}");
        }
    }
}

/// The repeats, written however they are written.
#[test]
fn repeats_in_repeats_are_refused_before_they_are_written_out() {
    too_many_notes(r"{ \repeat unfold 64 { \repeat unfold 64 { \repeat unfold 16 { c4 d e f } } } }");
    too_many_notes(r"{ \repeat %{%} unfold 64 { \repeat %{%} unfold 64 { \repeat %{ x %} unfold 16 { c4 d e f } } } }");
    too_many_notes(r"{ \repeat percent 64 { \repeat percent 64 { c4 d e f g } } }");
    too_many_notes(r"{ \repeat unfold 64 \repeat unfold 64 \repeat unfold 64 c4 }");
    // An ending played on every pass.
    too_many_notes(r"{ \repeat unfold 64 { c4 } \alternative { { \repeat unfold 64 { \repeat unfold 64 { c4 d e f } } } } }");
    too_many_notes(r"{ \repeat unfold 64 { \repeat unfold 64 { c4 } \alternative { { \repeat unfold 64 { c4 d e f } } } } }");
    // Settings multiply as notes do.
    too_many_notes(r#"{ \repeat unfold 64 { \repeat unfold 64 { \repeat unfold 64 { \bar "|" } } } }"#);
}

#[test]
fn bars_of_rest_count_as_the_bars_they_span() {
    too_many_notes(r"{ R1 * 1000000 }");
    too_many_notes(r"{ R1*20001 }");
    too_many_notes(r"{ s1*1000000000000 }");
    too_many_notes(r"{ s1*9223372036854775807 }");
    // A bar of 1/64: R1 is 64 of them.
    too_many_notes(r"{ \time 1/64 R1*1500 }");
    too_many_notes(r"{ \time 1/64 \repeat unfold 64 { R1*300 } }");
    assert_eq!(accepted(r"{ R1*4 }").events, 4);
    assert_eq!(accepted(r"{ R1*19999 }").events, 19_999);
    assert_eq!(accepted(r"{ \time 2/4 c1 }").events, 1 + 2);
}

#[test]
fn variables_count_at_each_use() {
    let a = "a-x = { \\repeat unfold 64 { c4 d e f } }\n";
    let b = format!("b-x = {{ {} }}\n", ["\\a-x"; 16].join(" "));
    let c = format!("c-x = {{ {} }}\n", ["\\b-x"; 16].join(" "));
    let d = format!("d-x = {{ {} }}\n", ["\\c-x"; 16].join(" "));
    too_many_notes(&format!("{a}{b}{c}{d}{{ \\d-x }}\n"));
    // 4 × 16 × 16 × 16 = 16,384: under.
    let a = "a = { c4 d e f }\n";
    let b = format!("b = {{ {} }}\n", ["\\a"; 16].join(" "));
    let c = format!("c = {{ {} }}\n", ["\\b"; 16].join(" "));
    let d = format!("d = {{ {} }}\n", ["\\c"; 16].join(" "));
    assert_eq!(accepted(&format!("{a}{b}{c}{d}{{ \\d }}\n")).events, 16_384);
    // Used on staves side by side, a staff each time.
    let m = "m = { \\repeat unfold 64 { \\repeat unfold 64 { c'4 } } }\n";
    too_many_notes(&format!("{m}<< {} >>\n", ["\\new Staff \\m"; 5].join(" ")));
}

#[test]
fn chords_count_their_heads_and_q_the_chord_it_repeats() {
    assert_eq!(accepted(r"{ <c e g>4 q q }").events, 9);
    let chord = "<c d e f g a b c' d' e' f' g' a' b'>4";
    too_many_notes(&format!("{{ \\repeat unfold 64 {{ \\repeat unfold 32 {{ {chord} q }} }} }}"));
}

#[test]
fn grace_notes_lyrics_and_tuplets_count() {
    assert_eq!(accepted(r"{ \grace { c16 d } e4 }").events, 3);
    assert_eq!(accepted(r"{ \tuplet 3/2 { c8 d e } }").events, 3);
    assert_eq!(accepted("{ c'4 d' e' f' } \\addlyrics { la la la la }").events, 4 + 4);
    too_many_notes(r"{ \repeat unfold 64 { \repeat unfold 64 { \grace { c16 d e f } g4 } } }");
}

#[test]
fn staves_are_as_long_as_the_longest() {
    // Each staff is drawn across every bar: 61 staves of 500 bars.
    let staves = ["\\new Staff { c'4 }"; 60].join(" ");
    too_many_notes(&format!("<< \\new Staff {{ s1*500 }} {staves} >>"));
    let w = accepted(r"<< \new Staff { s1*100 } \new Staff { c'4 } >>");
    assert_eq!(w.events, 200);
}

#[test]
fn words_count_each_time_they_are_drawn() {
    let long = "x".repeat(200);
    // A 200-character markup on a note repeated 4,096 times.
    for src in [
        format!("{{ \\repeat unfold 64 {{ \\repeat unfold 64 {{ c'4^\"{long}\" }} }} }}"),
        format!("{{ \\repeat unfold 64 {{ \\repeat unfold 64 {{ c'4^\\markup {{ \"{long}\" }} }} }} }}"),
        format!("m = {{ c'4^\"{long}\" }}\n{{ \\repeat unfold 64 {{ \\repeat unfold 64 {{ \\m }} }} }}"),
        format!("{{ \\repeat unfold 64 {{ \\repeat unfold 64 {{ \\mark \"{long}\" c'4 }} }} }}"),
        format!("{{ \\repeat unfold 64 {{ \\repeat unfold 64 {{ \\tempo \"{long}\" c'4 }} }} }}"),
    ] {
        let (message, took) = refused(&src);
        assert!(message.contains(WORDS), "{message}");
        assert!(took < FAST, "{took:?}");
    }
    let w = accepted(r#"{ c'4^"Allegro" d'4_\markup { "dolce" } }"#);
    assert_eq!(w, Written { events: 2, text: 12 });
    // A string in a variable used in a variable used in a variable: the
    // tokens' copies stop before they are made.
    let mut src = format!("a = \"{}\"\n", "x".repeat(1000));
    let mut last = "a".to_owned();
    for name in ["b", "c", "d", "e"] {
        src.push_str(&format!("{name} = {{ {} }}\n", vec![format!("c'4^\\{last}"); 16].join(" ")));
        last = name.into();
    }
    src.push_str(&format!("{{ \\{last} }}\n"));
    let (message, took) = refused(&src);
    assert!(message.contains("more music than this engraves") || message.contains(WORDS) || message.contains(NOTES), "{message}");
    assert!(took < FAST, "{took:?}");
}

/// The refusal is the engraver's, at every door.
#[test]
fn every_entry_point_refuses_the_same() {
    let src = r"{ \repeat unfold 64 { \repeat unfold 64 { \repeat unfold 64 { c4 } } } }";
    let t = Instant::now();
    let refusals = [
        engrave(src, 100.0, &Layout::default()).err().unwrap().message,
        engrave_a4(src).err().unwrap().message,
        midi(src, &MidiOptions::default()).err().unwrap().message,
        pdf(src, "").err().unwrap().message,
        pdf_outlines(src, "").err().unwrap().message,
        engrave_paper(src).err().unwrap().message,
        eui_score::warnings(src).err().unwrap().message,
        eui_score::protocol(src).err().unwrap().message,
    ];
    assert!(t.elapsed() < FAST * 2, "{:?}", t.elapsed());
    for m in &refusals {
        assert!(m.starts_with("score: this music writes out to more than 20,000 notes"), "{m}");
    }
}

/// Every marking drawn on a note counts: a few characters each, stacked on
/// a note in a repeat, were gigabytes of PDF.
#[test]
fn markings_on_notes_count_each_time_they_are_drawn() {
    assert_eq!(accepted(r"{ c4-> d-. e\trill f\fermata }").events, 4 + 4 * 2);
    assert_eq!(accepted(r"{ c4\ff d\p }").events, 2 + 3 + 2);
    assert_eq!(accepted(r"{ c4\< d\! e~ e }").events, 4 + 2 + 1);
    assert_eq!(accepted(r"{ c4( d) e\( f\) }").events, 4 + 2 * 10);
    assert_eq!(accepted(r"{ <c e g>4\arpeggio c4:16 c4-1 c\glissando d }").events, 3 + 1 + 1 + 1 + 1 + 4);
    // The same weight where the note is read and where its copies are counted.
    let once = accepted(r"{ c4->\trill\ff( d) }").events;
    assert_eq!(accepted(r"{ \repeat unfold 8 { c4->\trill\ff( d) } }").events, once * 8);
    let trills = r"\trill".repeat(60);
    too_many_notes(&format!("{{ \\repeat unfold 64 {{ \\repeat unfold 64 {{ c4{trills} }} }} }}"));
    too_many_notes(&format!("{{ \\repeat unfold 64 {{ \\repeat unfold 64 {{ c4{} }} }} }}", "->".repeat(100)));
    too_many_notes(r"{ \repeat unfold 64 { \repeat unfold 64 { c4\ffff d\pppp e\sfz f\fp } } }");
    too_many_notes(r"{ \repeat unfold 64 { \repeat unfold 64 { c''4( c,4) c'''4\( c,,4\) } } }");
    // Written once, without a repeat: counted when the music is read whole.
    let plain = format!("{{ {} }}", vec![format!("c4{}", r"\trill".repeat(20)); 1000].join(" "));
    too_many_notes(&plain);
}

/// Slurs at the budget lay out fast.
#[test]
fn slurred_music_at_the_budget_lays_out() {
    let src = r"{ \repeat unfold 64 { \repeat unfold 26 { c''4( c,4) } } }";
    assert_eq!(accepted(src).events, 19_968);
    assert!(engrave_a4(src).is_ok());
}

/// Music nested ten thousand deep is refused before the stack runs out: a
/// stack overflow is no refusal, it aborts the whole process.
#[test]
fn music_nested_too_deep_is_refused_on_a_small_stack() {
    let n = 10_000;
    let sources = [
        format!("{{ {} c4 {} }}", r"\repeat unfold 1 { ".repeat(n), " }".repeat(n)),
        format!("{{ {} c4 }}", r"\repeat unfold 1 ".repeat(n)),
        format!("{} c4 {}", "{ ".repeat(n), "} ".repeat(n)),
        format!("{} c4 {}", "<< ".repeat(n), ">> ".repeat(n)),
        format!("{{ {} c4 {} }}", r"\tuplet 3/2 { ".repeat(n), "} ".repeat(n)),
        format!("{} {{ c4 }}", r"\new Staff ".repeat(n)),
        format!("{{ c4^\\markup {} x }}", r"\bold ".repeat(n)),
        format!("{{ c4^\\markup {} x {} }}", r"\bold { ".repeat(n), "} ".repeat(n)),
        format!("a = {} {{ c4 }}\n{{ \\a }}", r"\relative ".repeat(n)),
        format!("{{ c4 }} \\addlyrics {{ {} la }}", r"\once ".repeat(n)),
    ];
    // A quarter of a server's 2 MB worker in a release build; a debug
    // build's frames are some ten times larger.
    let stack = if cfg!(debug_assertions) { 6 << 20 } else { 512 << 10 };
    let run = std::thread::Builder::new()
        .stack_size(stack)
        .spawn(move || {
            let mut out = Vec::new();
            for src in &sources {
                let t = Instant::now();
                let r = written_out(src);
                out.push((src.chars().take(60).collect::<String>(), r.err().map(|e| e.message), t.elapsed()));
            }
            // As deep as each goes, on the same stack: the deepest accepted
            // is read and printed without running out of it, and a dozen
            // deep is accepted. (before, one level, inside, its close, after)
            let shapes = [
                ("{ ", r"\repeat unfold 1 { ", "c4", " }", " }"),
                ("{ ", r"\repeat unfold 1 ", "c4", "", " }"),
                ("", "{ ", "c4", " }", ""),
                ("", "<< ", "c4", " >>", ""),
                ("{ c4^\\markup ", r"\bold { ", "x", " }", " }"),
            ];
            for (before, level, inside, close, after) in shapes {
                let nested = |k: usize| format!("{before}{}{inside}{}{after}", level.repeat(k), close.repeat(k));
                let mut k = 1;
                while written_out(&nested(k)).is_ok() {
                    k += 1;
                }
                let printed = pdf(&nested(k - 1), "").is_ok();
                out.push((format!("{level}{} deep", k - 1), (k <= 12 || !printed).then(|| format!("only {}, printed: {printed}", k - 1)), Duration::ZERO));
            }
            out
        })
        .unwrap()
        .join()
        .expect("no stack overflow");
    let (rest, deepest) = run.split_at(run.len() - 5);
    for (what, short, _) in deepest {
        assert!(short.is_none(), "{what}: {short:?}");
    }
    for (k, (src, refusal, took)) in rest.iter().enumerate() {
        let refusal = refusal.as_deref().unwrap_or_else(|| panic!("accepted: {src}"));
        assert!(took < &FAST, "{took:?}: {src}");
        // The first four can only be refused for their depth.
        assert!(k >= 4 || refusal.contains("nests too deep"), "{src}: {refusal}");
    }
}

/// Up to the budget, the music is engraved: 19,968 notes.
#[test]
fn music_just_under_the_budget_is_engraved() {
    let src = r"{ \repeat unfold 64 { \repeat unfold 64 { c4 d e f } } \repeat unfold 56 { \repeat unfold 16 { c4 d e f } } }";
    let w = accepted(src);
    assert_eq!(w.events, 19_968);
    assert!(w.events <= MAX_WRITTEN_EVENTS);
    assert!(engrave_a4(src).is_ok());
    too_many_notes(r"{ \repeat unfold 64 { \repeat unfold 64 { c4 d e f } } \repeat unfold 57 { \repeat unfold 16 { c4 d e f } } }");
}

/// The pieces, each written out to a tenth of the budget at most.
fn roomy(name: &str, src: &str) -> Written {
    let w = accepted(src);
    assert!(w.events * 10 <= MAX_WRITTEN_EVENTS, "{name}: {w:?}");
    w
}

#[test]
fn the_score_apps_largest_pieces_are_read_with_room_to_spare() {
    let pieces = sources(&dir("pieces"));
    assert!(pieces.len() >= 15);
    let mut most = 0;
    for (name, src) in &pieces {
        let w = roomy(name, src);
        // The two long texts of the reviews, written once: under the budget.
        if name.starts_with("long-") {
            assert!(w.text >= 9000 && w.text * 4 <= MAX_WRITTEN_TEXT, "{name}: {w:?}");
        } else {
            assert!(w.text * 10 <= MAX_WRITTEN_TEXT, "{name}: {w:?}");
        }
        assert!(engrave_a4(src).is_ok(), "{name}");
        most = most.max(w.events);
    }
    assert_eq!(most, 765, "symphony-5 is the largest");
}

/// `SCORE_REPO=…/score cargo test -p eui-score --test written`: every
/// piece of the score app, and every house section, read with room.
#[test]
fn every_piece_of_the_score_app_when_its_checkout_is_named() {
    let Some(repo) = std::env::var_os("SCORE_REPO").map(PathBuf::from) else { return };
    let mut all = Vec::new();
    let mut dirs = vec![repo.join("scores")];
    while let Some(d) = dirs.pop() {
        for e in fs::read_dir(&d).unwrap().filter_map(|e| e.ok()) {
            let p = e.path();
            if p.is_dir() {
                dirs.push(p);
            } else if p.extension().is_some_and(|x| x == "ly") {
                all.push((p.display().to_string(), fs::read_to_string(&p).unwrap()));
            }
        }
    }
    let house: serde_json::Value = serde_json::from_str(&fs::read_to_string(repo.join("db/data/house_sections.json")).unwrap()).unwrap();
    for s in house.as_array().unwrap() {
        all.push((s["seed"].as_str().unwrap_or("").to_owned(), s["source"].as_str().unwrap().to_owned()));
    }
    assert!(all.len() >= 400, "{} sources", all.len());
    for (name, src) in &all {
        roomy(name, src);
    }
}
