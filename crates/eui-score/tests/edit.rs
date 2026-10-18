//! 03 §1.4, protocol 10: the pitch of a note moved in the source itself.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

use eui_score::edit::{self, Move, NotEditable};
use eui_score::parse::Clef;

fn item(src: &str, n: usize) -> edit::Item {
    edit::item(src, n).expect("editable").expect("a note")
}

#[test]
fn a_note_is_found_where_it_is_written_even_after_accents() {
    // The title's accents are two bytes each: a character position used as
    // a byte would land inside the wrong note.
    let src = "\\header { title = \"Frère Jacques à l'été\" }\n{ \\clef treble c'4 <e' g'>4 r4 fis''8 }";
    let c = item(src, 0);
    assert_eq!(&src[c.spans[0].clone()], "c'");
    let chord = item(src, 1);
    assert_eq!(chord.heads.len(), 2);
    assert_eq!(&src[chord.spans[0].clone()], "e'");
    assert_eq!(&src[chord.spans[1].clone()], "g'");
    assert_eq!(edit::item(src, 2), Ok(None), "a rest is not moved");
    let fis = item(src, 3);
    assert_eq!(&src[fis.spans[0].clone()], "fis''");
    assert_eq!(fis.heads, vec![(17, 1)]);
}

#[test]
fn a_pitch_is_spelled_as_the_editor_writes_it() {
    assert_eq!(edit::spell(7, 0), "c'");
    assert_eq!(edit::spell(0, 0), "c");
    assert_eq!(edit::spell(-5, 1), "eis,");
    assert_eq!(edit::spell(9, -1), "es'");
    assert_eq!(edit::spell(12, -2), "ases'");
    assert_eq!(edit::spell(13, -1), "bes'");
    assert_eq!(edit::spell(10, 2), "fisis'");
    assert_eq!(edit::midi(7, 0), 60);
    assert_eq!(edit::octave(7), 4);
}

#[test]
fn the_arrows_move_in_the_key() {
    // ↑ from E in G major is F sharp.
    let e = item("{ \\key g \\major e'4 }", 0);
    assert_eq!(edit::moved(&e, Move::Step(1)), Some(vec![(10, 1)]));
    // Shift+↑ from A in F major is B flat; from F in C major, F sharp;
    // Shift+↓ from G in C major, G flat.
    let a = item("{ \\key f \\major a'4 }", 0);
    assert_eq!(edit::moved(&a, Move::Semitone(1)), Some(vec![(13, -1)]));
    let f = item("{ f'4 }", 0);
    assert_eq!(edit::moved(&f, Move::Semitone(1)), Some(vec![(10, 1)]));
    let g = item("{ g'4 }", 0);
    assert_eq!(edit::moved(&g, Move::Semitone(-1)), Some(vec![(11, -1)]));
    // Ctrl+↑ is the octave, alteration kept.
    let fis = item("{ fis'4 }", 0);
    assert_eq!(edit::moved(&fis, Move::Octave(1)), Some(vec![(17, 1)]));
}

#[test]
fn a_chord_moves_all_its_heads_or_none() {
    // Treble's middle line is b' (13): 14 half spaces up is 27.
    let top = item("{ <a''' c''''>4 }", 0);
    assert_eq!(top.heads, vec![(26, 0), (28, 0)]);
    assert_eq!(edit::moved(&top, Move::Step(1)), None, "c'''' is already past the reach");
    let chord = item("{ <c'' e''>4 }", 0);
    assert_eq!(edit::moved(&chord, Move::Step(1)), Some(vec![(15, 0), (17, 0)]));
}

#[test]
fn a_drag_puts_the_nearest_head_on_the_line() {
    // Treble: pos 0 is the top line, F5; pos 8 the bottom one, E4.
    assert_eq!(edit::at_position(Clef::TREBLE, 0, 0), (17, 0));
    assert_eq!(edit::at_position(Clef::TREBLE, 0, 8), (9, 0));
    assert_eq!(edit::at_position(Clef::TREBLE, 2, 0), (17, 1), "F is sharp in D major");
    let chord = item("{ <c'' e''>4 }", 0);
    // g'' is nearer e'' than c'': the top head goes, the order is kept.
    assert_eq!(edit::placed(&chord, 18, 0), Some(vec![(14, 0), (18, 0)]));
    // a' is below c'': the bottom head goes, and the chord stays ascending.
    assert_eq!(edit::placed(&chord, 12, 0), Some(vec![(12, 0), (16, 0)]));
    assert_eq!(edit::placed(&chord, 40, 0), None);
}

#[test]
fn a_rewrite_changes_the_pitch_and_nothing_else() {
    let src = "\\header { title = \"Été\" }\n{ c'4. ~ <e' g'>8( fis''16) }";
    let chord = item(src, 1);
    let out = edit::rewrite(src, &chord, &[(9, -1), (11, 0)]);
    assert_eq!(out, "\\header { title = \"Été\" }\n{ c'4. ~ <es' g'>8( fis''16) }");
    let fis = item(&out, 2);
    assert_eq!(edit::rewrite(&out, &fis, &[(4, 0)]), "\\header { title = \"Été\" }\n{ c'4. ~ <es' g'>8( g16) }");
}

#[test]
fn a_relative_source_is_not_edited() {
    assert_eq!(edit::item("\\relative c' { c4 d e }", 0), Err(NotEditable::Relative));
    assert_eq!(edit::item("{ c'4 \\nonsense }", 0), Err(NotEditable::Refused));
}

#[test]
fn an_arrow_on_a_chord_moves_one_head() {
    let chord = item("{ <c'' e'' g''>4 }", 0);
    assert_eq!(edit::moved_head(&chord, 1, Move::Step(1)), Some(vec![(14, 0), (17, 0), (18, 0)]), "e'' up to f''");
    assert_eq!(edit::moved_head(&chord, 0, Move::Step(-1)), Some(vec![(13, 0), (16, 0), (18, 0)]), "c'' down to b'");
    // Past another head: the chord stays ascending.
    assert_eq!(edit::moved_head(&chord, 0, Move::Octave(1)), Some(vec![(16, 0), (18, 0), (21, 0)]));
    assert_eq!(edit::nearest_head(&chord, 17), 1);
    assert_eq!(edit::nearest_head(&chord, 40), 2);
}

#[test]
fn a_drag_with_shift_moves_by_semitones() {
    let e = item("{ \\key f \\major a'4 }", 0);
    // Up a semitone from A in F major is B flat; two, B natural; down one, A flat.
    assert_eq!(edit::dragged_chromatic(&e, 0, (12, 0), 1), Some(vec![(13, -1)]));
    assert_eq!(edit::dragged_chromatic(&e, 0, (12, 0), 2), Some(vec![(13, 0)]));
    assert_eq!(edit::dragged_chromatic(&e, 0, (12, 0), -1), Some(vec![(12, -1)]));
    assert_eq!(edit::dragged_chromatic(&e, 0, (12, 0), 0), Some(vec![(12, 0)]));
}
