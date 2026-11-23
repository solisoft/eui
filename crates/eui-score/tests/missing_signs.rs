//! The signs an engraver is expected to know (score #1026): arpeggio
//! styles, two-note tremolos, the other piano pedals, lyric elisions and
//! the rhythmic staff — each read, drawn, and played where it sounds.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing, clippy::panic, clippy::arithmetic_side_effects, clippy::float_cmp)]

use eui_score::parse::{parse, ArpStyle, Ev, Kind};
use eui_score::{engrave, midi, paths, warnings, Engraved, Layout, MidiOptions, Path};

fn eng(src: &str) -> Engraved {
    engrave(src, 900.0 / 8.0, &Layout::default()).unwrap_or_else(|e| panic!("{src}: {e}"))
}

/// The engraved notes and rests of the first staff, bar after bar.
fn items(e: &Engraved) -> Vec<&Ev> {
    e.staves[0].iter().flat_map(|b| b.items.iter()).filter(|ev| matches!(ev.k, Kind::Note | Kind::Rest)).collect()
}

/// The paths drawn for the first staff's `n`th note or rest: (rectangles, outlines).
fn drawn_item(src: &str, n: usize) -> (usize, usize) {
    let (all, ranges) = paths(&eng(src), 8.0, 0).unwrap();
    let range = ranges.iter().find(|r| r.0 == n).map(|r| r.1.clone()).unwrap();
    let rects = all[range.clone()].iter().filter(|p| matches!(p, Path::Rect(_))).count();
    (rects, range.len() - rects)
}

/// Every note of a Standard MIDI File: (key, on tick, off tick), in time order.
fn notes(bytes: &[u8]) -> Vec<(u8, u32, u32)> {
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
    out
}

fn played(src: &str) -> Vec<(u8, u32, u32)> {
    notes(&midi(src, &MidiOptions::default()).unwrap())
}

/// The MIDI file's division: ticks to a quarter.
fn quarter(src: &str) -> u32 {
    let bytes = midi(src, &MidiOptions::default()).unwrap();
    u32::from(u16::from_be_bytes([bytes[12], bytes[13]]))
}

// ---- Arpeggio styles ---------------------------------------------------------

#[test]
fn an_arpeggio_style_holds_until_another_is_said() {
    let src = r"{ <c' e' g'>4\arpeggio \arpeggioBracket <c' e' g'>\arpeggio \arpeggioParenthesis <c' e' g'>\arpeggio <d' f' a'>\arpeggio \arpeggioNormal <c' e' g'>\arpeggio }";
    let p = parse(src).unwrap();
    let styles: Vec<ArpStyle> = p.staves[0].events.iter().filter(|e| e.k == Kind::Note).map(|e| e.arp_style).collect();
    assert_eq!(styles, vec![ArpStyle::Wave, ArpStyle::Bracket, ArpStyle::Paren, ArpStyle::Paren, ArpStyle::Wave]);
    assert!(warnings(src).unwrap().is_empty(), "honoured, not passed over: {:?}", warnings(src).unwrap());
}

#[test]
fn a_bracket_is_a_line_and_its_two_ends_and_a_parenthesis_one_curve() {
    let plain = drawn_item(r"{ <c' e' g'>4 }", 0);
    let bracket = drawn_item(r"{ \arpeggioBracket <c' e' g'>4\arpeggio }", 0);
    let paren = drawn_item(r"{ \arpeggioParenthesis <c' e' g'>4\arpeggio }", 0);
    let wave = drawn_item(r"{ <c' e' g'>4\arpeggio }", 0);
    assert_eq!(bracket, (plain.0 + 3, plain.1), "three strokes, no wiggle");
    assert_eq!(paren, (plain.0, plain.1 + 1), "one outline");
    assert!(wave.1 > plain.1 + 1, "the wavy line, several wiggles");
    // Each before the chord, the room it takes made.
    for src in [r"{ \arpeggioBracket <c' e' g'>4\arpeggio }", r"{ \arpeggioParenthesis <c' e' g'>4\arpeggio }"] {
        let e = eng(src);
        let ev = items(&e)[0];
        let x = ev.arp_x.unwrap();
        assert!(x < -0.3 && ev.left >= -x, "{src}: left of the heads, in its room ({x}, {})", ev.left);
    }
}

#[test]
fn a_bracketed_chord_is_played_together_and_a_parenthesis_rolled() {
    let q = quarter(r"{ c'4 }");
    let together = played(r"{ \arpeggioBracket <c' e' g'>4\arpeggio }");
    assert!(together.iter().all(|n| n.1 == 0 && n.2 == q), "struck at once: {together:?}");
    let rolled = played(r"{ \arpeggioParenthesis <c' e' g'>4\arpeggio }");
    assert_eq!(rolled.iter().map(|n| n.1).collect::<Vec<_>>(), vec![0, q / 16, q / 8]);
}

// ---- Two-note tremolos -------------------------------------------------------

/// The widths of the beams drawn on the first page: the outlines of four
/// straight sides.
fn beam_strokes(e: &Engraved) -> Vec<f64> {
    let (all, _) = paths(e, 8.0, 0).unwrap();
    all.iter()
        .filter_map(|p| match p {
            Path::Fill(c) if c.len() == 13 && c[0] == 0.0 && c[3] == 1.0 && c[6] == 1.0 && c[9] == 1.0 && c[12] == 4.0 && (c[1] - c[10]).abs() < 1e-6 => Some(c[4] - c[1]),
            _ => None,
        })
        .collect()
}

#[test]
fn a_tremolo_of_two_notes_draws_each_as_long_as_the_whole() {
    // Eight times two sixteenths: a whole bar, two whole notes.
    let p = parse(r"{ \repeat tremolo 8 { c'16 e' } }").unwrap();
    let notes: Vec<&Ev> = p.staves[0].events.iter().filter(|e| e.k == Kind::Note).collect();
    assert_eq!(notes.len(), 2);
    assert_eq!((notes[0].base, notes[1].base), (1, 1), "drawn as whole notes");
    assert_eq!(notes[0].ticks + notes[1].ticks, eui_score::WHOLE, "a bar between them");
    assert_eq!((notes[0].trem_beams, notes[0].trem_first, notes[1].trem_first), (2, true, false), "a sixteenth's two beams");
    // Four times: two halves; two eighths alternating twice: two quarters, one beam.
    let halves = parse(r"{ \repeat tremolo 4 { c'16 e' } }").unwrap();
    assert!(halves.staves[0].events.iter().filter(|e| e.k == Kind::Note).all(|e| e.base == 2));
    let quarters = parse(r"{ \repeat tremolo 2 { c'16 e' } }").unwrap();
    assert!(quarters.staves[0].events.iter().filter(|e| e.k == Kind::Note).all(|e| e.base == 4 && e.trem_beams == 2));
    let eighths = parse(r"{ \repeat tremolo 2 { c'8 e' } }").unwrap();
    assert!(eighths.staves[0].events.iter().filter(|e| e.k == Kind::Note).all(|e| e.base == 2 && e.trem_beams == 1), "halves, one beam");
    // One note is a tremolo of its own: `c'4:16`.
    let one = parse(r"{ \repeat tremolo 4 c'16 }").unwrap();
    let c = one.staves[0].events.iter().find(|e| e.k == Kind::Note).unwrap();
    assert_eq!((c.base, c.tremolo, c.trem_beams), (4, 16, 0));
}

#[test]
fn a_tremolo_that_is_not_one_is_refused() {
    for src in [
        r"{ \repeat tremolo 4 { c'16 e' g' } }",
        r"{ \repeat tremolo 4 { c'16 e'8 } }",
        r"{ \repeat tremolo 3 { c'16 e' } }",
        r"{ \repeat tremolo 4 { c'4 e' } }",
        r"{ \repeat tremolo 32 { c'16 e' } }",
        r"{ \repeat tremolo 4 { c'16 r } }",
        r"{ \repeat tremolo 4 { c'16. e' } }",
    ] {
        let err = parse(src).err().unwrap_or_else(|| panic!("{src}: read"));
        assert!(err.message.contains("\\repeat tremolo"), "{src}: {}", err.message);
    }
}

#[test]
fn the_two_notes_are_beamed_together_with_the_written_beams() {
    for (src, n) in
        [(r"{ \repeat tremolo 4 { c''16 a' } r2 }", 2), (r"{ \repeat tremolo 2 { c''8 a' } r2. }", 1), (r"{ \repeat tremolo 2 { c''32 a' } r4. }", 3), (r"{ \repeat tremolo 8 { c''16 a' } }", 2)]
    {
        let e = eng(src);
        let bar = &e.staves[0][0];
        assert_eq!(bar.beams.len(), 1, "{src}");
        assert_eq!((bar.beams[0].notes.len(), bar.beams[0].n), (2, n), "{src}");
        assert!(bar.items[0].up == bar.items[1].up, "{src}: one direction");
    }
    // Between halves the beams stop short of the stems, 0.8 space from
    // each (LilyPond's Beam.gap); between quarters they reach them.
    for (src, short) in [(r"{ \repeat tremolo 4 { c''16 a' } r2 }", true), (r"{ \repeat tremolo 2 { c''16 a' } r2. }", false)] {
        let e = eng(src);
        let g = &e.staves[0][0].beams[0];
        let strokes = beam_strokes(&e);
        assert_eq!(strokes.len(), 2, "{src}");
        for w in strokes {
            let between = (g.x1 - g.x0) * 8.0;
            if short {
                assert!((w - (between - 1.6 * 8.0)).abs() < 0.5, "{src}: {w} against {between}");
            } else {
                assert!(w > between, "{src}: {w} against {between}");
            }
        }
    }
    // Next to them, notes beam as before: the tremolo is no part of their beam.
    let e = eng(r"{ c''8 d'' \repeat tremolo 2 { c''16 a' } e''8 f'' r2 }");
    let groups: Vec<usize> = e.staves[0][0].beams.iter().map(|g| g.notes.len()).collect();
    assert_eq!(groups, vec![2, 2, 2]);
    // Cut by a bar line, the notes are what they sound, tied, and the beam
    // goes; the music after it is beamed as ever.
    let e = eng(r"{ r2.. \repeat tremolo 4 { c''16 a' } c''8 d'' r2 }");
    assert!(e.staves[0].iter().all(|b| b.beams.iter().all(|g| g.notes.iter().all(|&j| b.items[j].trem_beams == 0))), "no tremolo beam");
    played(r"{ r2.. \repeat tremolo 4 { c''16 a' } c''8 d'' r2 }");
}

#[test]
fn the_two_notes_are_played_in_turn_in_the_written_value() {
    let q = quarter(r"{ c'4 }");
    let all = played(r"{ \repeat tremolo 4 { c'16 e' } c'4 }");
    let keys: Vec<u8> = all.iter().map(|n| n.0).collect();
    assert_eq!(keys, vec![60, 64, 60, 64, 60, 64, 60, 64, 60]);
    assert!(all[..8].iter().enumerate().all(|(k, n)| n.1 == k as u32 * q / 4 && n.2 == n.1 + q / 4), "sixteenths back to back: {all:?}");
    assert_eq!(all[8].1, 2 * q, "the next note after the half note's time");
}

// ---- The other pedals --------------------------------------------------------

/// Every controller change of a Standard MIDI File: (tick, controller, value), in order.
fn controls(bytes: &[u8]) -> Vec<(u32, u8, u8)> {
    let mut out = Vec::new();
    let mut i = 14;
    while i + 8 <= bytes.len() {
        let len = u32::from_be_bytes([bytes[i + 4], bytes[i + 5], bytes[i + 6], bytes[i + 7]]) as usize;
        let track = &bytes[i + 8..i + 8 + len];
        i += 8 + len;
        let (mut at, mut k, mut status) = (0u32, 0usize, 0u8);
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
                0xb0 => {
                    out.push((at, track[k], track[k + 1]));
                    k += 2;
                }
                0x80 | 0x90 | 0xa0 | 0xe0 => k += 2,
                0xc0 | 0xd0 => k += 1,
                _ => {
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
    out
}

#[test]
fn the_sostenuto_and_the_soft_pedal_are_read_and_written_under_the_staff() {
    let src = r"{ c'4\sostenutoOn d'\unaCorda e'\sostenutoOff\sostenutoOn f'\sostenutoOff\treCorde }";
    let p = parse(src).unwrap();
    let notes: Vec<&Ev> = p.staves[0].events.iter().filter(|e| e.k == Kind::Note).collect();
    assert_eq!(notes[0].pedals, vec![(66, true)]);
    assert_eq!(notes[1].pedals, vec![(67, true)]);
    assert_eq!(notes[2].pedals, vec![(66, false), (66, true)], "let go, then pressed again");
    assert_eq!(notes[3].pedals, vec![(66, false), (67, false)]);
    let words = |ev: &Ev| -> Vec<(u8, Vec<String>)> { ev.texts.iter().map(|w| (w.pedal, w.runs.iter().map(|r| r.text.clone()).collect())).collect() };
    assert_eq!(words(notes[0]), vec![(66, vec!["Sost. Ped.".to_owned()])]);
    assert_eq!(words(notes[1]), vec![(67, vec!["una corda".to_owned()])]);
    assert_eq!(words(notes[2]), vec![(66, vec!["*".to_owned(), "Sost. Ped.".to_owned()])], "side by side, as \\sustainOff\\sustainOn");
    assert_eq!(words(notes[3]), vec![(66, vec!["*".to_owned()]), (67, vec!["tre corde".to_owned()])]);
    assert!(notes[0].texts[0].runs.iter().all(|r| r.weight == eui_score::glyphs::Weight::Italic && !r.glyph), "italic words, LilyPond's font-shape");
    // Drawn under the staff, each at its note.
    let e = eng(src);
    let line = &e.systems[0].lines[0];
    let placed = |t: &str| line.texts.iter().find(|tx| tx.runs.iter().any(|r| r.text == t)).unwrap_or_else(|| panic!("{t} not drawn"));
    let items = items(&e);
    for (t, at) in [("Sost. Ped.", 0), ("una corda", 1), ("tre corde", 3)] {
        let tx = placed(t);
        assert!(tx.y > 4.0, "{t}: under the staff ({})", tx.y);
        assert!((tx.x0 - items[at].x).abs() < 1.0, "{t}: at its note");
    }
}

#[test]
fn the_sostenuto_is_controller_66_and_the_soft_pedal_67() {
    let q = quarter(r"{ c'4 }");
    let bytes = midi(r"{ c'4\sostenutoOn d'\unaCorda e'\sostenutoOff\sostenutoOn f'\sostenutoOff\treCorde }", &MidiOptions::default()).unwrap();
    let pedals: Vec<(u32, u8, u8)> = controls(&bytes).into_iter().filter(|c| c.1 == 66 || c.1 == 67).collect();
    assert_eq!(pedals, vec![(0, 66, 127), (q, 67, 127), (2 * q, 66, 0), (2 * q, 66, 127), (3 * q, 66, 0), (3 * q, 67, 0)]);
}

// ---- Lyric elisions ----------------------------------------------------------

#[test]
fn a_tilde_sings_two_syllables_on_one_note() {
    for src in [r"{ c'4 d' e' f' } \addlyrics { Ky -- ri -- e~e son }", r"{ c'4 d' e' f' } \addlyrics { Ky -- ri -- e ~ e son }"] {
        let p = parse(src).unwrap();
        let sung: Vec<String> = p.staves[0].lyrics.as_ref().unwrap().iter().map(|y| y.text.clone()).collect();
        assert_eq!(sung, ["Ky", "ri", "e~e", "son"], "{src}");
        assert_eq!(eui_score::protocol(src), Ok(11));
        let e = eng(src);
        let items = items(&e);
        assert_eq!(items[2].lyric.as_ref().unwrap().text, "e~e", "{src}: on the third note");
        assert_eq!(items[3].lyric.as_ref().unwrap().text, "son");
    }
    for src in [r"{ c'4 d' } \addlyrics { ~ a b }", r"{ c'4 d' } \addlyrics { a b ~ }", r"{ c'4 d' } \addlyrics { a ~ -- b }"] {
        let err = parse(src).err().unwrap_or_else(|| panic!("{src}: read"));
        assert!(err.message.contains("~ joins two syllables"), "{src}: {}", err.message);
    }
}

#[test]
fn the_two_syllables_are_joined_by_a_tie_under_them() {
    // The same letters, and one outline more: the tie, not a "~".
    let (tied, _) = paths(&eng(r"{ c'2 d' } \addlyrics { e~a la }"), 8.0, 0).unwrap();
    let (plain, _) = paths(&eng(r"{ c'2 d' } \addlyrics { ea la }"), 8.0, 0).unwrap();
    assert_eq!(tied.len(), plain.len() + 1);
    let curves = |all: &[Path]| all.iter().filter(|p| matches!(p, Path::Fill(c) if c.len() == 21 && c[3] == 3.0 && c[10] == 1.0 && c[13] == 3.0 && c[20] == 4.0)).count();
    assert_eq!(curves(&tied), curves(&plain) + 1, "a tie's outline: out along one curve, back along the other");
    // It takes room: the tie and half a space either side.
    let wide = items(&eng(r"{ c'16 d' } \addlyrics { e~a la }"))[0].right;
    let narrow = items(&eng(r"{ c'16 d' } \addlyrics { ea la }"))[0].right;
    assert!(wide > narrow + 0.9, "{wide} against {narrow}");
}

// ---- RhythmicStaff -----------------------------------------------------------

/// The long horizontal strokes drawn: a staff's lines.
fn staff_lines(e: &Engraved) -> usize {
    let (all, _) = paths(e, 8.0, 0).unwrap();
    all.iter().filter(|p| matches!(p, Path::Rect(r) if r[2] > 100.0 && r[3] < 2.0)).count()
}

#[test]
fn a_rhythmic_staff_is_one_line_with_every_note_on_it() {
    let src = r"\new RhythmicStaff { \key d \major c'4 fis''8 g'' <c' e' g'>4. b8 }";
    let p = parse(src).unwrap();
    assert_eq!(p.staves[0].kind, eui_score::parse::StaffKind::Rhythmic);
    assert_eq!(eui_score::protocol(src), Ok(11));
    let e = eng(src);
    let notes: Vec<&Ev> = items(&e);
    assert!(notes.iter().all(|ev| ev.heads.iter().all(|h| h.pos == 0 && h.acc.is_none())), "on the line, no accidental");
    assert!(notes.iter().all(|ev| ev.up), "stems up");
    assert!(e.staves[0][0].beams.iter().all(|g| g.up), "beams over");
    assert_eq!(e.staves[0][0].fifths, 0, "no key");
    assert_eq!(e.staves[0][0].clef.sign, eui_score::parse::Sign::Rhythmic, "no clef");
    assert_eq!(staff_lines(&e), 1);
    assert_eq!(staff_lines(&eng(r"\new Staff { c'4 fis''8 g'' <c' e' g'>4. b8 }")), 5);
    // No clef's room before the metre: the notes start further left.
    let staff = eng(r"\new Staff { c'4 d' e' f' }");
    let rhythmic = eng(r"\new RhythmicStaff { c'4 d' e' f' }");
    assert!(items(&rhythmic)[0].x < items(&staff)[0].x - 2.0, "{} against {}", items(&rhythmic)[0].x, items(&staff)[0].x);
    // Beside a staff of notes.
    let both = eng(r"<< \new Staff { c''4 d'' e'' f'' } \new RhythmicStaff { c4 c8 c c2 } >>");
    assert!(both.staves[1].iter().flat_map(|b| b.items.iter()).all(|ev| ev.heads.iter().all(|h| h.pos == 0)));
    // A second voice's stems still go down.
    let two = eng(r"\new RhythmicStaff << { c4 c c c } \\ { c2 c } >>");
    assert!(two.staves[0][0].items.iter().filter(|ev| ev.k == Kind::Note).any(|ev| !ev.up));
}

#[test]
fn a_rhythmic_staff_plays_its_notes_as_written() {
    let all = played(r"\new RhythmicStaff { c'4 e' g'2 }");
    assert_eq!(all.iter().map(|n| n.0).collect::<Vec<_>>(), vec![60, 64, 67]);
}
