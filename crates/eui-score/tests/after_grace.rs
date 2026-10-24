//! `\afterGrace` (score #389): grace notes after a note, read, drawn small
//! before the next note, and played in the end of their note — from its
//! 3/4 point, or the fraction written — as LilyPond plays them.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing, clippy::panic, clippy::arithmetic_side_effects)]

use eui_score::measure::measures;
use eui_score::parse::{parse, Kind, AFTER_GRACE};
use eui_score::{engrave, midi, paths, protocol, Layout, MidiOptions};

/// Every note of a Standard MIDI File: (key, on tick, off tick), in time order.
fn notes(bytes: &[u8]) -> (u16, Vec<(u8, u32, u32)>) {
    let division = u16::from_be_bytes([bytes[12], bytes[13]]);
    let mut out = Vec::new();
    let mut i = 14;
    while i + 8 <= bytes.len() {
        let len = u32::from_be_bytes([bytes[i + 4], bytes[i + 5], bytes[i + 6], bytes[i + 7]]) as usize;
        let track = &bytes[i + 8..i + 8 + len];
        i += 8 + len;
        let (mut at, mut k, mut status) = (0u32, 0usize, 0u8);
        let mut open: Vec<(u8, u32)> = Vec::new();
        while k < track.len() {
            let mut delta = 0u32;
            loop {
                let b = track[k];
                k += 1;
                delta = (delta << 7) | u32::from(b & 0x7f);
                if b & 0x80 == 0 {
                    break;
                }
            }
            at += delta;
            if track[k] & 0x80 != 0 {
                status = track[k];
                k += 1;
            }
            match status & 0xf0 {
                0x80 | 0x90 => {
                    let (key, vel) = (track[k], track[k + 1]);
                    k += 2;
                    if status & 0xf0 == 0x90 && vel > 0 {
                        open.push((key, at));
                    } else if let Some(p) = open.iter().position(|o| o.0 == key) {
                        let (key, on) = open.remove(p);
                        out.push((key, on, at));
                    }
                }
                0xa0 | 0xb0 | 0xe0 => k += 2,
                0xc0 | 0xd0 => k += 1,
                _ => {
                    // Meta (FF type len) or SysEx (F0 len).
                    if status == 0xff {
                        k += 1;
                    }
                    let mut l = 0usize;
                    loop {
                        let b = track[k];
                        k += 1;
                        l = (l << 7) | usize::from(b & 0x7f);
                        if b & 0x80 == 0 {
                            break;
                        }
                    }
                    k += l;
                }
            }
        }
    }
    out.sort_by_key(|n| (n.1, n.0));
    (division, out)
}

#[test]
fn after_grace_notes_ride_on_their_note_taking_no_time_and_no_number() {
    let src = r"{ c''4 \afterGrace d''2 { e''16 f'' } r4 }";
    assert_eq!(protocol(src).unwrap(), 11, "beyond the Soli engraver");
    let p = parse(src).unwrap();
    let evs: Vec<_> = p.staves[0].events.iter().filter(|e| matches!(e.k, Kind::Note | Kind::Rest)).collect();
    assert_eq!(evs.len(), 3, "c d r: the after-grace notes are not events of their own");
    let main = evs[1];
    assert_eq!(main.after_graces.len(), 2);
    assert!(main.graces.is_empty(), "nothing before d");
    assert_eq!(main.after_at, AFTER_GRACE);
    assert_eq!(main.after_graces.iter().map(|g| g.pitches[0].d).collect::<Vec<_>>(), vec![16, 17], "e'' f''");
    assert_eq!(evs.iter().map(|e| e.i).collect::<Vec<_>>(), vec![0, 1, 2], "numbered as if they were not there");
    let bars = measures(&p.staves[0].events).unwrap();
    assert_eq!(bars.len(), 1, "they take no time: one bar of 4/4");
    // LilyPond's fraction, and \relative reading them from the note.
    let p = parse(r"\relative c'' { \afterGrace 7/8 c2 { d16 } c2 }").unwrap();
    let main = p.staves[0].events.iter().find(|e| !e.after_graces.is_empty()).unwrap();
    assert!((main.after_at - 0.875).abs() < 1e-9);
    assert_eq!(main.after_graces[0].pitches[0].d, 15, "d'' after c''");
    // A grace note before the note and one after: both kept.
    let both = parse(r"{ \acciaccatura c''8 \afterGrace d''2 { e''16 } r2 }").unwrap();
    let main = both.staves[0].events.iter().find(|e| !e.after_graces.is_empty()).unwrap();
    assert_eq!((main.graces.len(), main.after_graces.len()), (1, 1));
}

#[test]
fn what_after_grace_does_not_take_is_refused_with_its_place() {
    for (src, says) in [
        (r"{ \afterGrace 9/8 c''2 { d''16 } r2 }", "between 0 and 1"),
        (r"{ \afterGrace r2 { d''16 } r2 }", "one note"),
        (r"{ \afterGrace c''2 }", "grace notes"),
        (r"{ \afterGrace c''2 { r16 } r2 }", "grace notes are notes"),
        (r"{ \tuplet 3/2 { \afterGrace c''4 { d''16 } c'' c'' } r2 }", "tuplet"),
    ] {
        let err = parse(src).expect_err(src).message;
        assert!(err.contains(says), "{src}: {err}");
        assert!(err.contains("(at character "), "{err}");
    }
}

#[test]
fn after_grace_notes_are_drawn_small_after_the_note_before_the_next() {
    let plain = r"{ c''4 d''2 r4 }";
    let src = r"{ c''4 \afterGrace d''2 { e''16 f'' } r4 }";
    let eng = engrave(src, 80.0, &Layout::default()).unwrap();
    let (drawn, _) = paths(&eng, 8.0, 0).unwrap();
    let (before, _) = paths(&engrave(plain, 80.0, &Layout::default()).unwrap(), 8.0, 0).unwrap();
    assert!(drawn.len() >= before.len() + 4, "two heads, two stems and a beam more: {} against {}", drawn.len(), before.len());
    let items = &eng.staves[0][0].items;
    let main = items.iter().find(|e| !e.after_graces.is_empty()).unwrap();
    let next = items.iter().find(|e| e.start > main.start).unwrap();
    for g in &main.after_graces {
        assert!(g.x > 0.0, "right of their note");
        assert!(main.x + g.x < next.x, "left of the next one");
        assert!(g.head_w.unwrap() < main.head_w.unwrap(), "small");
    }
    assert_eq!(main.after_graces[0].beam, Some(0), "two sixteenths beamed");
    // The note makes room for them.
    let plain_eng = engrave(plain, 80.0, &Layout::default()).unwrap();
    let plain_main = &plain_eng.staves[0][0].items[1];
    assert!(main.right > plain_main.right + 1.0);
    // Cut at a bar line, the note keeps them on its last piece.
    let cut = parse(r"{ r2. \afterGrace c''2 { d''16 } r2. }").unwrap();
    let bars = measures(&cut.staves[0].events).unwrap();
    let pieces: Vec<_> = bars.iter().flat_map(|b| &b.items).filter(|e| e.k == Kind::Note).collect();
    assert_eq!(pieces.len(), 2, "c2 cut into two tied quarters");
    assert!(pieces[0].after_graces.is_empty() && pieces[1].after_graces.len() == 1);
    engrave(r"{ r2. \afterGrace c''2 { d''16 } r2. }", 80.0, &Layout::default()).unwrap();
}

#[test]
fn after_grace_notes_are_played_in_the_end_of_their_note() {
    // d''2 at 100 a quarter: the note to its 3/4 point, the two graces sharing the last quarter.
    let (division, all) = notes(&midi(r"{ \afterGrace d''2 { e''16 f'' } }", &MidiOptions::default()).unwrap());
    let half = 2 * u32::from(division);
    assert_eq!(all.len(), 3);
    let (d, e, f) = (all[0], all[1], all[2]);
    assert_eq!((d.0, e.0, f.0), (74, 76, 77));
    assert_eq!((d.1, d.2), (0, half * 3 / 4), "the note gives up its last quarter");
    assert_eq!(e.1, half * 3 / 4);
    assert_eq!(f.2, half, "the last grace ends with the note's time");
    assert!(e.2 <= f.1 && f.1 > e.1, "one after the other");
    assert!((f.1 - e.1).abs_diff(f.2 - f.1) <= 1, "two sixteenths share it evenly");
    // The fraction written.
    let (_, seven) = notes(&midi(r"{ \afterGrace 7/8 d''2 { e''16 } }", &MidiOptions::default()).unwrap());
    assert_eq!(seven[0].2, half * 7 / 8);
    assert_eq!((seven[1].1, seven[1].2), (half * 7 / 8, half));
    // Unequal values share it by what they are written.
    let (_, uneven) = notes(&midi(r"{ \afterGrace d''2 { e''8 f''16 } }", &MidiOptions::default()).unwrap());
    let (e_len, f_len) = (uneven[1].2 - uneven[1].1, uneven[2].2 - uneven[2].1);
    assert!(e_len.abs_diff(2 * f_len) <= 1, "an eighth twice a sixteenth: {e_len} {f_len}");
}

/// The note `\afterGrace` is written on, among `src`'s events.
fn main_note(src: &str) -> eui_score::parse::Ev {
    let p = parse(src).unwrap_or_else(|e| panic!("{src}: {}", e.message));
    p.staves[0].events.iter().find(|e| !e.after_graces.is_empty()).cloned().unwrap()
}

#[test]
fn what_is_written_after_the_note_before_its_braces_is_the_notes() {
    // A tie, a slur, a phrasing slur, a dynamic, a hairpin, articulations,
    // a fingering, words: on the note, as for a note on its own.
    let tie = main_note(r"{ \afterGrace c''1~ { d''16 } c''1 }");
    assert!(tie.tie);
    assert!(tie.after_graces.iter().all(|g| !g.tie), "the tie is the note's, not its grace's");
    let slur = parse(r"{ \afterGrace c''1( { d''16 c'' } d''1) }").unwrap();
    let notes: Vec<_> = slur.staves[0].events.iter().filter(|e| e.k == Kind::Note).collect();
    assert_eq!(notes.len(), 2);
    assert!(notes[0].slur_on && !notes[0].slur_off && notes[1].slur_off);
    assert_eq!(notes[0].after_graces.len(), 2);
    assert!(notes[0].after_graces.iter().all(|g| !g.slur_on && !g.slur_off));
    assert!(main_note(r"{ \afterGrace c''1\( { d''16 } d''1\) }").phrase_on);
    assert_eq!(main_note(r"{ \afterGrace c''1\f { d''16 } r1 }").dynamic.as_deref(), Some("f"));
    assert_eq!(main_note(r"{ \afterGrace c''1\< { d''16 } r1\! }").hairpin, Some('<'));
    let names = |ev: &eui_score::parse::Ev| ev.scripts.iter().map(|s| s.name.clone()).collect::<Vec<_>>();
    assert_eq!(names(&main_note(r"{ \afterGrace c''1-> { d''16 } r1 }")), vec!["accent"]);
    assert_eq!(names(&main_note(r"{ \afterGrace c''1^. { d''16 } r1 }")), vec!["staccato"]);
    assert_eq!(names(&main_note(r"{ \afterGrace c''1\fermata { d''16 } r1 }")), vec!["fermata"]);
    assert_eq!(names(&main_note(r"{ \afterGrace c''1\startTrillSpan { b'16 c'' } d''1\stopTrillSpan }")), vec!["trill"]);
    assert_eq!(main_note(r"{ \afterGrace c''1-3 { d''16 } r1 }").fingers[0].text, "3");
    assert_eq!(main_note(r#"{ \afterGrace c''1^"rit." { d''16 } r1 }"#).texts[0].text, "rit.");
    // Several, one after the other, and the fraction written before the note.
    let many = main_note(r"{ \afterGrace 7/8 c''1~\p-> { d''16 } c''1 }");
    assert!(many.tie && many.dynamic.as_deref() == Some("p") && names(&many) == vec!["accent"]);
    assert!((many.after_at - 0.875).abs() < 1e-9);
    assert!(main_note(r"{ \afterGrace 7/8 c''1~ { d''16 } c''1 }").tie);
    // The tie after the braces still the note's, as before.
    assert!(main_note(r"{ \afterGrace c''1 { d''16 }~ c''1 }").tie);
    // A chord, and \relative reading the graces from the note.
    assert!(main_note(r"{ \afterGrace <c'' e''>1~ { d''16 } <c'' e''>1 }").tie);
    let rel = main_note(r"\relative c'' { \afterGrace c1~ { d16 } c1 }");
    assert!(rel.tie);
    assert_eq!(rel.after_graces[0].pitches[0].d, 15, "d'' after c''");
    // Music between the note and its braces is still refused.
    let err = parse(r"{ \afterGrace c''1 \clef bass { d16 } r1 }").expect_err("a clef").message;
    assert!(err.contains("\\afterGrace wants"), "{err}");
}

#[test]
fn a_tie_from_the_note_is_drawn_over_its_grace_notes_to_the_next() {
    for src in [r"{ \afterGrace c''2~ { d''16 } c''2 }", r"{ \afterGrace c''2 { d''16 }~ c''2 }", r"{ \afterGrace 7/8 c''2~ { d''16 } c''2 }"] {
        let eng = engrave(src, 80.0, &Layout::default()).unwrap();
        let line = &eng.systems[0].lines[0];
        assert_eq!(line.curves.len(), 1, "{src}: one tie");
        let items = &eng.staves[0][0].items;
        let main = items.iter().find(|e| !e.after_graces.is_empty()).unwrap();
        let next = items.iter().find(|e| e.k == Kind::Note && e.start > main.start).unwrap();
        let (xa, xb) = (line.curves[0][1], line.curves[0][8]);
        assert!(xa > main.x && xa < main.x + main.after_graces[0].x, "{src}: from the note, before its grace");
        assert!(xb > main.x + main.after_graces[0].x && xb < next.x, "{src}: past the grace, into the next note");
        paths(&eng, 8.0, 0).unwrap();
    }
    // The slur, the dynamic and the accent drawn as on a plain note.
    let eng = engrave(r"{ \afterGrace c''2(\f-> { d''16 } d''2) }", 80.0, &Layout::default()).unwrap();
    let line = &eng.systems[0].lines[0];
    assert_eq!(line.curves.len(), 1, "one slur");
    assert_eq!(line.dynamics.len(), 1);
    assert_eq!(line.scripts.len(), 1);
}

#[test]
fn a_tied_note_is_held_under_its_grace_notes_into_the_next() {
    // c''2~ c''2 with d''16 after the first: c sounds once, the whole
    // note long, the grace over it in the first half's last quarter.
    for src in [r"{ \afterGrace c''2~ { d''16 } c''2 }", r"{ \afterGrace c''2 { d''16 }~ c''2 }"] {
        let (division, all) = notes(&midi(src, &MidiOptions::default()).unwrap());
        let half = 2 * u32::from(division);
        assert_eq!(all.len(), 2, "{src}: c once, held; d: {all:?}");
        let c = all.iter().find(|n| n.0 == 72).unwrap();
        let d = all.iter().find(|n| n.0 == 74).unwrap();
        assert_eq!((c.1, c.2), (0, 2 * half), "{src}: held into the next");
        assert_eq!((d.1, d.2), (half * 3 / 4, half), "{src}");
    }
    // The fraction written: the grace moves, the note is still held.
    let (division, all) = notes(&midi(r"{ \afterGrace 7/8 c''2~ { d''16 } c''2 }", &MidiOptions::default()).unwrap());
    let half = 2 * u32::from(division);
    assert_eq!(all, vec![(72, 0, 2 * half), (74, half * 7 / 8, half)]);
    // Not tied: struck again, the note giving its end to the grace.
    let (_, untied) = notes(&midi(r"{ \afterGrace c''2 { d''16 } c''2 }", &MidiOptions::default()).unwrap());
    assert_eq!(untied.len(), 3);
    assert_eq!((untied[0].0, untied[0].2), (72, half * 3 / 4));
    // A grace of the note's own key strikes it again: the tie gives way.
    let (_, own) = notes(&midi(r"{ \afterGrace c''2~ { c''16 } c''2 }", &MidiOptions::default()).unwrap());
    assert_eq!(own.iter().filter(|n| n.0 == 72).count(), 3, "{own:?}");
    assert_eq!(own[0].2, half * 3 / 4);
    // Tied into a different note, nothing is held.
    let (_, other) = notes(&midi(r"{ \afterGrace c''2~ { d''16 } e''2 }", &MidiOptions::default()).unwrap());
    assert_eq!(other.len(), 3);
    assert_eq!(other[0].2, half, "held its own time, the tie pointing nowhere: {other:?}");
}
