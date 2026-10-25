//! What a score's metre may do that the engraver refused before (score
//! #408), as LilyPond 2.24 does it: a `\time` in the middle of a bar, a
//! `\partial` after the first note, staves showing different metres over
//! bars of one length (`\set Staff.timeSignatureFraction`,
//! `\scaleDurations`, `\enablePolymeter`), a `\time` or `\partial` one staff
//! writes being the whole score's, and voices inside a voice.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing, clippy::panic, clippy::arithmetic_side_effects)]

use eui_score::measure::Bar;
use eui_score::parse::{parse, Kind, SHOWN};
use eui_score::{engrave, midi, protocol, warnings, Engraved, Layout, MidiOptions, WHOLE};

fn eng(src: &str) -> Engraved {
    engrave(src, 900.0 / 8.0, &Layout::default()).unwrap_or_else(|e| panic!("{src}: {e}"))
}

fn refusal(src: &str) -> String {
    engrave(src, 900.0 / 8.0, &Layout::default()).err().map(|e| e.message).unwrap_or_else(|| panic!("{src} was engraved"))
}

/// Each bar of a staff: its length, in quarters.
fn quarters(bars: &[Bar]) -> Vec<f64> {
    bars.iter().map(|b| b.length as f64 / (WHOLE / 4) as f64).collect()
}

/// The time signatures a MIDI file sets: (tick, numerator, denominator).
fn metres(bytes: &[u8]) -> Vec<(u32, u8, u8)> {
    let mut out = Vec::new();
    let track = &bytes[22..];
    let (mut k, mut at) = (0usize, 0u32);
    while k + 3 < track.len() {
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
        match track[k] {
            0xff => {
                let (kind, len) = (track[k + 1], track[k + 2] as usize);
                if kind == 0x58 {
                    out.push((at, track[k + 3], 1u8 << track[k + 4]));
                }
                if kind == 0x2f {
                    break;
                }
                k += 3 + len;
            }
            s if s & 0xf0 == 0xc0 || s & 0xf0 == 0xd0 => k += 2,
            _ => k += 3,
        }
    }
    out
}

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
fn a_time_in_the_middle_of_a_bar_goes_on_in_the_new_metre() {
    // Half a 4/4 bar, then 6/4: the bar goes on to six quarters, the 6/4
    // drawn before the e', and the next bar is 6/4 without saying so again.
    let e = eng(r"{ c'4 d' \time 6/4 e' f' g' a' | b'1. }");
    let bars = &e.staves[0];
    assert_eq!(quarters(bars), vec![6.0, 6.0]);
    assert_eq!(bars[0].time, (4, 4), "the bar opened in 4/4");
    assert_eq!(bars[0].items[2].time_change, Some(((6, 4), [0; 4])), "drawn before the e'");
    assert!(bars[0].items[2].pre > 0.0 && bars[0].items[2].change_x.is_some(), "room made for it");
    assert!(bars[0].items.iter().filter(|ev| ev.time_change.is_some()).count() == 1);
    assert_eq!((bars[1].time, bars[1].changes.time), ((6, 4), false));
    // Already as long as the new metre's bar, or longer: the bar ends where
    // the \time is, and the next one opens with it.
    for (src, first) in [(r"{ c'4 d' \time 2/4 e' f' | g'2 }", 2.0), (r"{ c'4 d' e' \time 2/4 f' g' | a'2 }", 3.0)] {
        let e = eng(src);
        let bars = &e.staves[0];
        assert_eq!(quarters(bars)[0], first, "{src}");
        assert!(bars[1].changes.time && bars[1].time == (2, 4), "{src}");
        assert!(bars.iter().flat_map(|b| &b.items).all(|ev| ev.time_change.is_none()));
    }
    // Played: the metre is set where its bar starts.
    let bytes = midi(r"{ c'4 d' \time 6/4 e' f' g' a' | b'1. }", &MidiOptions::default()).unwrap();
    assert_eq!(metres(&bytes), vec![(0, 4, 4), (12 * 6720 / 2, 6, 4)]);
    // Written again where a staff says it: no metre twice.
    let e = eng(r"{ c'4 d' \time 3/4 \time 3/4 e'4 | f'2. }");
    assert_eq!(quarters(&e.staves[0]), vec![3.0, 3.0]);
}

#[test]
fn a_partial_after_the_first_note_sets_what_is_left_of_the_bar() {
    // A section in another metre, starting with a pickup.
    let src = r#"{ \time 3/4 c'2. | d'2. \bar "||" \time 4/4 \partial 4 g'4 | c''1 }"#;
    let e = eng(src);
    let bars = &e.staves[0];
    assert_eq!(quarters(bars), vec![3.0, 3.0, 1.0, 4.0]);
    assert_eq!(bars[1].bar, "||");
    assert_eq!(bars[2].offset, 3 * WHOLE / 4, "the pickup is the end of a 4/4 bar");
    assert!(bars[2].changes.time);
    assert_eq!(bars.iter().map(|b| b.number).collect::<Vec<_>>(), vec![1, 2, 3, 4], "counted as LilyPond counts on");
    // In the middle of a bar: it ends that much later.
    let e = eng(r"{ c'4 d' \partial 4 e' | f'1 }");
    assert_eq!(quarters(&e.staves[0]), vec![3.0, 4.0]);
    // After a \time in the middle of a bar, as the Notation Reference writes it.
    let e = eng(r"{ c'4 d' e' \time 3/4 \partial 4 f' | g'2. }");
    assert_eq!(quarters(&e.staves[0]), vec![3.0, 1.0, 3.0]);
    // A pickup beams as the end of its bar: the two eighths in one beat group.
    let e = eng(r"{ c'1 | \partial 4 c'8 d' | e'8 f' g' a' b' c'' d'' e'' }");
    assert_eq!(e.staves[0][1].beams.len(), 1);
    // As long as a bar is no pickup.
    assert!(refusal(r"{ c'1 \partial 1 c'1 }").contains("shorter than a bar"));
    assert_eq!(protocol(r"{ c'1 \partial 4 c'4 }"), Ok(11));
}

#[test]
fn a_time_or_a_partial_one_staff_writes_is_the_scores() {
    // The upper staff says 3/4, then a 2/4 in the middle of a bar and a
    // pickup: the lower one, saying nothing, has the same bars.
    let src = r"<< \new Staff { \time 3/4 c''2. | d''4 e'' \time 2/4 f'' g'' | \partial 4 a''4 | b''2 }
                  \new Staff { \clef bass c2. | d2 | e2 | f4 | g2 } >>";
    let e = eng(src);
    assert_eq!(quarters(&e.staves[0]), vec![3.0, 2.0, 2.0, 1.0, 2.0]);
    assert_eq!(quarters(&e.staves[1]), quarters(&e.staves[0]));
    assert!(e.staves[1][0].changes.time && e.staves[1][0].time == (3, 4), "the lower staff draws the 3/4 too");
    // Chord names follow a change of metre they do not write.
    let e = eng(r"<< \new ChordNames \chordmode { c2. g2 c2 } \new Staff { \time 3/4 c'2. \time 2/4 d'2 e'2 } >>");
    assert_eq!(quarters(&e.staves[0]), vec![3.0, 2.0, 2.0]);
    // Each staff's own: kept, the first's taken where both write one.
    let e = eng(r"<< \new Staff { \time 3/4 c''2. } \new Staff { \time 6/8 c'4. c' } >>");
    assert_eq!((e.staves[0][0].time, e.staves[1][0].time), ((3, 4), (6, 8)));
    // The MIDI's tracks agree.
    let bytes = midi(r"<< \new Staff { \time 3/4 c''2. c''2. } \new Staff { c'2. c'2. } >>", &MidiOptions::default()).unwrap();
    let (_, all) = notes(&bytes);
    assert_eq!(all.iter().map(|n| n.1).collect::<Vec<_>>(), vec![0, 0, 3 * 6720, 3 * 6720]);
}

#[test]
fn staves_show_different_metres_over_bars_of_one_length() {
    // The Notation Reference's example: 3/4, 9/8 and 10/8 bar by bar.
    let src = r"<<
      \new Staff \relative c' { \time 3/4 c4 c c | c4 c c | }
      \new Staff \relative c' { \time 3/4 \set Staff.timeSignatureFraction = 9/8 \scaleDurations 2/3 \repeat unfold 6 { c8[ c c] } }
      \new Staff \relative c' { \time 3/4 \set Staff.timeSignatureFraction = 10/8
        \scaleDurations 3/5 { \repeat unfold 2 { c8[ c c] } \repeat unfold 2 { c8[ c] } | c4. c4. \tuplet 3/2 { c8[ c c] } c4 } }
    >>";
    assert!(warnings(src).unwrap().is_empty());
    let e = eng(src);
    for s in 0..3 {
        assert_eq!(quarters(&e.staves[s]), vec![3.0, 3.0], "staff {s}");
    }
    let shown: Vec<((i64, i64), (i64, i64))> = e.staves.iter().map(|bars| (bars[0].time, bars[0].metre())).collect();
    assert_eq!(shown, vec![((3, 4), (3, 4)), ((9, 8), (3, 4)), ((10, 8), (3, 4))]);
    assert_eq!(e.staves[1][1].time, (9, 8), "and in the bars after");
    // An eighth there is two thirds of one, drawn as an eighth, no tuplet.
    let eighth = &e.staves[1][0].items[0];
    assert_eq!((eighth.base, eighth.ticks, eighth.tuplet, eighth.scale), (8, WHOLE / 8 * 2 / 3, None, Some((2, 3))));
    // A triplet inside: both.
    let trip = e.staves[2][1].items.iter().find(|ev| ev.tuplet.is_some()).unwrap();
    assert_eq!(trip.ticks, WHOLE / 8 * 2 / 3 * 3 / 5);
    // Beamed as written: three and three in 9/8, and by the bar's beats unwritten.
    assert_eq!(e.staves[1][0].beams.iter().map(|b| b.notes.len()).collect::<Vec<_>>(), vec![3, 3, 3]);
    let e = eng(r"{ \time 3/4 \set Staff.timeSignatureFraction = 9/8 \scaleDurations 2/3 { c'8 c' c' c' c' c' c' c' c' } }");
    assert_eq!(e.staves[0][0].beams.iter().map(|b| b.notes.len()).collect::<Vec<_>>(), vec![3, 3, 3]);
    // Played as long as they last.
    let (_, all) = notes(&midi(r"{ \time 3/4 \scaleDurations 2/3 { c'8 d' e' } c'2 }", &MidiOptions::default()).unwrap());
    assert_eq!(all.iter().map(|n| (n.1, n.2)).collect::<Vec<_>>(), vec![(0, 2240), (2240, 4480), (4480, 6720), (6720, 20160)]);
    // Scheme's pair, and \enablePolymeter or the old way of moving the
    // Timing_translator: read, no warning.
    let p = parse(r"{ \set Staff.timeSignatureFraction = #'(9 . 8) c'4 }").unwrap();
    assert!(p.staves[0].events.iter().any(|ev| ev.k == Kind::Time && ev.time == (9, 8) && ev.v == SHOWN));
    for src in [
        r"\layout { \enablePolymeter } << \new Staff { \time 3/4 c'2. } \new Staff { \time 6/8 c'4. c' } >>",
        r#"\layout { \context { \Score \remove "Timing_translator" \remove "Default_bar_line_engraver" } \context { \Staff \consists "Timing_translator" \consists "Default_bar_line_engraver" } } << \new Staff { \time 3/4 c'2. } \new Staff { \time 6/8 c'4. c' } >>"#,
    ] {
        assert!(warnings(src).unwrap().is_empty(), "{src}");
        assert!(parse(src).unwrap().paper.polymeter);
        assert_eq!(eng(src).staves[1][0].time, (6, 8));
    }
    // Under \enablePolymeter a staff keeps its own \time: bars of other
    // lengths are refused, with what lines them up.
    let unequal = r"\layout { \enablePolymeter } << \new Staff { \time 3/4 c'2. c'2. } \new Staff { \time 2/4 c'2 c'2 c'2 } >>";
    let why = refusal(unequal);
    assert!(why.contains("\\scaleDurations"), "{why}");
    // …where without it the first staff's 3/4 is the score's: the second
    // staff's halves run across its bar line, tied.
    let e = eng(r"<< \new Staff { \time 3/4 c'2. c'2. } \new Staff { c'2 c'2 c'2 } >>");
    assert_eq!(quarters(&e.staves[1]), vec![3.0, 3.0]);
    assert!(e.staves[1][0].items.last().unwrap().tie);
    // Refused: a ratio missing, a fraction that is not one, a scaled note
    // across a bar line (no value spells its pieces).
    assert!(refusal(r"{ \scaleDurations c'4 }").contains("ratio"));
    assert!(refusal(r"{ \set Staff.timeSignatureFraction = 9 c'4 }").contains("fraction"));
    assert!(refusal(r"{ \scaleDurations 2/3 { c'4 c'2. c'2. } }").contains("bar line"));
    assert_eq!(protocol(r"{ \scaleDurations 2/3 { c'8 c' c' } }"), Ok(11));
}

#[test]
fn voices_inside_a_voice_are_more_voices_of_the_staff() {
    let src = r"\new Staff \relative c'' { << { c4 << { e4 d } \\ { c4 b } >> c4 | d2 c } \\ { a2 g4 f | e2 e } >> }";
    let p = parse(src).unwrap();
    let ev = &p.staves[0].events;
    assert_eq!(ev.iter().filter(|e| e.k == Kind::Voices).count(), 1, "one << >> on the staff");
    assert_eq!(ev.iter().filter(|e| e.k == Kind::VoiceNext).count(), 2, "three voices in it");
    let voice = |v: u8| -> Vec<(i64, bool)> { ev.iter().filter(|e| matches!(e.k, Kind::Note | Kind::Rest) && e.voice == Some(v)).map(|e| (e.pitches.first().map_or(-1, |p| p.d), e.spacer)).collect() };
    assert_eq!(voice(1).iter().map(|n| n.0).collect::<Vec<_>>(), vec![14, 16, 15, 14, 15, 14], "c e d c, d c: the inner first goes on in its voice");
    assert_eq!(voice(4).iter().filter(|n| !n.1).map(|n| n.0).collect::<Vec<_>>(), vec![14, 13], "c b, a voice of their own, stems down");
    assert!(voice(4).first().unwrap().1 && voice(4).last().unwrap().1, "silent before and after");
    assert_eq!(voice(2).len(), 5);
    let e = eng(src);
    let bar = &e.staves[0][0];
    assert!(bar.poly);
    let ups: Vec<(u8, bool)> = bar.items.iter().filter(|e| e.k == Kind::Note).map(|e| (e.voice.unwrap(), e.up)).collect();
    assert!(ups.iter().all(|(v, up)| *up == (v % 2 == 1)), "{ups:?}");
    // Every note is played.
    let (_, all) = notes(&midi(src, &MidiOptions::default()).unwrap());
    assert_eq!(all.len(), 6 + 2 + 5);
    // In the second voice: the inner second goes down too, as `\\` sets it.
    let p = parse(r"<< { c''1 } \\ { << { a'2 } \\ { f'2 } >> g'2 } >>").unwrap();
    let f = p.staves[0].events.iter().find(|e| e.k == Kind::Note && e.pitches[0].d == 10).unwrap();
    assert_eq!(f.voice, Some(4));
    // In a staff of one voice, as before: two voices.
    let p = parse(r"{ c''4 << { e''4 } \\ { c''4 } >> d''2 }").unwrap();
    assert_eq!(p.staves[0].events.iter().filter(|e| e.k == Kind::VoiceNext).count(), 1);
    // Refused: inner voices of different lengths, more than four at once.
    assert!(refusal(r"<< { c''4 << { e''4 d'' } \\ { c''4 } >> c''2 } \\ { a'1 } >>").contains("not as long"));
    assert!(refusal(r"<< { c''1 } \\ { a'1 } \\ { << { f'2 } \\ { d'2 } \\ { b2 } >> e'2 } >>").contains("four voices"));
}
