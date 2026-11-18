//! A fermata, played (score #977): the note held half its value again,
//! then the music breathes FERMATA_BREATH_SECONDS, every staff together,
//! before it goes on — at the tempo of that moment.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing, clippy::panic, clippy::arithmetic_side_effects)]

use eui_score::{midi, MidiOptions};

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

fn played(src: &str) -> (u32, Vec<(u8, u32, u32)>) {
    let (division, all) = notes(&midi(src, &MidiOptions::default()).unwrap());
    (u32::from(division), all)
}

#[test]
fn a_fermata_holds_its_note_half_again_then_breathes_two_seconds() {
    // No \tempo: MIDI's 120 a minute, a quarter half a second, two seconds four quarters.
    let (q, all) = played(r"{ c''4\fermata d''4 }");
    assert_eq!(all.len(), 2);
    assert_eq!((all[0].1, all[0].2), (0, q * 3 / 2), "held half its value again");
    assert_eq!(all[1].1, q * 3 / 2 + 4 * q, "then two seconds of silence");
    assert_eq!(all[1].2 - all[1].1, q, "what follows keeps its own length");
}

#[test]
fn the_breath_is_two_seconds_at_the_tempo_of_the_moment() {
    // 60 a quarter: two seconds are two quarters.
    let (q, slow) = played(r"{ \tempo 4 = 60 c''4\fermata d''4 }");
    assert_eq!(slow[1].1, q * 3 / 2 + 2 * q);
    // A tempo changed before the fermata: the new one counts.
    let (q, changed) = played(r"{ c''4 \tempo 4 = 240 d''4\fermata e''4 }");
    assert_eq!(changed[1].2, q + q * 3 / 2);
    assert_eq!(changed[2].1, q + q * 3 / 2 + 8 * q, "at 240, two seconds are eight quarters");
}

#[test]
fn every_staff_holds_and_breathes_together() {
    let (q, all) = played(r"<< \new Staff { c''4\fermata d''4 } \new Staff { c4 b,4 } >>");
    let lower: Vec<_> = all.iter().filter(|n| n.0 < 60).collect();
    assert_eq!(lower[0].2, q * 3 / 2, "the other staff's chord note is held as long");
    assert_eq!(lower[1].1, q * 3 / 2 + 4 * q, "and waits for the breath");
}

#[test]
fn without_a_fermata_nothing_waits() {
    let (q, all) = played(r"{ c''4 d''4 }");
    assert_eq!((all[0].2, all[1].1), (q, q));
}
