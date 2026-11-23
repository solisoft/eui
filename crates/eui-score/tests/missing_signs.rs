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
