//! Shape notes (score #1026): `\aikenHeads`, `\sacredHarpHeads`,
//! `\southernHarmonyHeads`, `\funkHeads`, `\walkerHeads`, their thin and
//! minor kin and `\set shapeNoteStyles`, each head's shape by its degree in
//! the key, as LilyPond's `shapeNoteStyles` gives it, in Emmentaler's
//! shape-note glyphs.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing, clippy::panic, clippy::arithmetic_side_effects)]

use eui_score::look::shape_head;
use eui_score::parse::{Ev, Kind};
use eui_score::{engrave, engrave_paper, paths, Engraved, Layout};

fn eng(src: &str) -> Engraved {
    engrave(src, 900.0 / 8.0, &Layout::default()).unwrap_or_else(|e| panic!("{src}: {e}"))
}

fn notes(eng: &Engraved) -> Vec<Ev> {
    eng.staves[0].iter().flat_map(|b| b.items.iter()).filter(|e| e.k == Kind::Note).cloned().collect()
}

/// Each note's heads' glyphs, low to high; "" for the usual head.
fn shapes(src: &str) -> Vec<Vec<&'static str>> {
    notes(&eng(src)).iter().map(|ev| ev.heads.iter().map(|h| shape_head(ev, h.d, ev.up).unwrap_or("")).collect()).collect()
}

fn one(src: &str) -> Vec<&'static str> {
    shapes(src).into_iter().map(|h| h[0]).collect()
}

#[test]
fn aiken_heads_shape_each_degree_of_the_major_scale() {
    let got = one("{ \\key c \\major \\aikenHeads c'4 d' e' f' g' a' b' c'' }");
    let fa = got[3];
    assert_eq!(got, vec!["noteheads.s2do", "noteheads.s2re", "noteheads.s2miMirror", fa, "noteheads.s2sol", "noteheads.s2la", "noteheads.s2ti", "noteheads.s2do"]);
    // Fa's triangle leans as its stem goes: up on f', down on f''.
    assert_eq!(fa, "noteheads.u2fa");
    assert_eq!(one("{ \\aikenHeads f''4 }"), vec!["noteheads.d2fa"]);
    // Halves and wholes have theirs.
    assert_eq!(one("{ \\aikenHeads c'2 e' d'1 }"), vec!["noteheads.s1do", "noteheads.s1miMirror", "noteheads.s0re"]);
    // Counted from the key's tonic: in D, d is do and f sharp mi.
    assert_eq!(one("{ \\key d \\major \\aikenHeads d'4 fis' }"), vec!["noteheads.s2do", "noteheads.s2miMirror"]);
    // Before the switch, and without it, the usual heads.
    assert_eq!(one("{ c'4 \\aikenHeads c'4 }"), vec!["", "noteheads.s2do"]);
}

#[test]
fn a_minor_key_counts_from_its_tonic_as_lilypond_does() {
    // \aikenHeads in A minor: A is do, as LilyPond has it ...
    assert_eq!(one("{ \\key a \\minor \\aikenHeads a'4 c'' }"), vec!["noteheads.s2do", "noteheads.s2miMirror"]);
    // ... and the minor set makes it la.
    assert_eq!(one("{ \\key a \\minor \\aikenHeadsMinor a'4 c'' e'' }"), vec!["noteheads.s2la", "noteheads.s2do", "noteheads.s2miMirror"]);
    assert_eq!(one("{ \\key a \\minor \\sacredHarpHeadsMinor a'4 b' c'' }"), vec!["noteheads.s2la", "noteheads.s2mi", "noteheads.d2fa"]);
}

#[test]
fn the_four_shape_systems_and_the_other_families() {
    // Sacred Harp in G: fa sol la fa sol la mi.
    let harp = one("{ \\key g \\major \\sacredHarpHeads g'4 a' b' c'' d'' e'' fis'' }");
    assert_eq!(harp, vec!["noteheads.u2fa", "noteheads.s2sol", "noteheads.s2la", "noteheads.d2fa", "noteheads.s2sol", "noteheads.s2la", "noteheads.s2mi"]);
    let southern = one("{ \\southernHarmonyHeads c'4 d' e' f' g' a' b' }");
    assert_eq!(southern, vec!["noteheads.u2faThin", "noteheads.s2sol", "noteheads.s2laThin", "noteheads.u2faThin", "noteheads.s2sol", "noteheads.s2laThin", "noteheads.s2miThin"]);
    let thin = one("{ \\aikenThinHeads c'4 d' e' f' g' a' b' }");
    assert_eq!(thin, vec!["noteheads.s2doThin", "noteheads.s2reThin", "noteheads.s2miThin", "noteheads.u2faThin", "noteheads.s2sol", "noteheads.s2laThin", "noteheads.s2tiThin"]);
    // b', on the middle line, stems down.
    let funk = one("{ \\funkHeads c'4 d' e' f' g' a' b' }");
    assert_eq!(funk, vec!["noteheads.u2doFunk", "noteheads.u2reFunk", "noteheads.s2miFunk", "noteheads.u2faFunk", "noteheads.s2solFunk", "noteheads.s2laFunk", "noteheads.d2tiFunk"]);
    let walker = one("{ \\walkerHeads c'4 d' e' f' g' a' b' }");
    assert_eq!(walker, vec!["noteheads.u2doWalker", "noteheads.u2reWalker", "noteheads.s2miWalker", "noteheads.u2faWalker", "noteheads.s2solFunk", "noteheads.s2laWalker", "noteheads.d2tiWalker"]);
    for minor in ["aikenThinHeadsMinor", "funkHeadsMinor", "walkerHeadsMinor", "southernHarmonyHeadsMinor"] {
        let got = one(&format!("{{ \\key a \\minor \\{minor} a'4 }}"));
        assert!(got[0].contains("la"), "{minor}: {got:?}");
    }
}

#[test]
fn each_head_of_a_chord_has_its_own_shape_and_set_shape_note_styles_is_read() {
    assert_eq!(shapes("{ \\aikenHeads <c' e' g'>4 }"), vec![vec!["noteheads.s2do", "noteheads.s2miMirror", "noteheads.s2sol"]]);
    // A list of its own; #f, the usual head.
    let own = one("{ \\set shapeNoteStyles = ##(do re mi fa #f la ti) c'4 e' g' }");
    assert_eq!(own, vec!["noteheads.s2do", "noteheads.s2mi", ""]);
    assert_eq!(one("{ \\aikenHeads c'4 \\unset shapeNoteStyles c'4 }"), vec!["noteheads.s2do", ""]);
    // Drawn: other paths than the usual heads; on paper too.
    let drawn = |src: &str| paths(&eng(src), 8.0, 0).unwrap().0;
    assert!(drawn("{ \\aikenHeads c'4 d' e' f' }") != drawn("{ c'4 d' e' f' }"));
    engrave_paper("{ \\sacredHarpHeads c'4 d' e' f' }").unwrap();
}
