//! Ornaments played (score #978): a trill, a prall, a mordent, a turn and
//! a rolled chord written out in the MIDI, as LilyPond's articulate.ly
//! plays them — the neighbours the key's.

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

fn keys(all: &[(u8, u32, u32)]) -> Vec<u8> {
    all.iter().map(|n| n.0).collect()
}

#[test]
fn a_trill_goes_between_the_note_and_the_one_above_in_32nds() {
    let (q, all) = played(r"{ c''4\trill }");
    assert_eq!(keys(&all), vec![72, 74, 72, 74, 72, 74, 72, 74]);
    assert!(all.iter().all(|n| n.2 - n.1 == q / 8), "32nds");
    assert_eq!(all.last().unwrap().2, q, "to the note's end");
}

#[test]
fn the_neighbour_is_the_keys() {
    // G major: above f'' sharp, g''.
    let (_, g) = played(r"{ \key g \major fis''4\trill }");
    assert_eq!(&keys(&g)[..2], &[78, 79]);
    // F major: above a', b flat.
    let (_, f) = played(r"{ \key f \major a'4\trill }");
    assert_eq!(&keys(&f)[..2], &[69, 70]);
    // B flat major: below c'', b' flat.
    let (_, b) = played(r"{ \key bes \major c''4\mordent }");
    assert_eq!(keys(&b), vec![72, 70, 72]);
}

#[test]
fn a_prall_a_mordent_and_a_turn_then_the_note_held() {
    let (q, prall) = played(r"{ c''4\prall }");
    assert_eq!(keys(&prall), vec![72, 74, 72]);
    assert_eq!((prall[2].1, prall[2].2), (q / 4, q), "the note held to its end");
    let (_, mordent) = played(r"{ c''4\mordent }");
    assert_eq!(keys(&mordent), vec![72, 71, 72]);
    let (q, turn) = played(r"{ c''4\turn }");
    assert_eq!(keys(&turn), vec![74, 72, 71, 72]);
    assert_eq!(turn[3].1, 3 * q / 8);
}

#[test]
fn a_rolled_chord_comes_in_from_the_lowest_and_is_held() {
    let (q, all) = played(r"{ <c' e' g'>4\arpeggio }");
    assert_eq!(keys(&all), vec![60, 64, 67]);
    assert_eq!(all.iter().map(|n| n.1).collect::<Vec<_>>(), vec![0, q / 16, q / 8], "a 64th apart");
    assert!(all.iter().all(|n| n.2 == q), "all held to the end");
}

#[test]
fn a_chord_trills_its_top_note_the_others_held() {
    let (q, all) = played(r"{ <e' c''>4\trill }");
    let low: Vec<_> = all.iter().filter(|n| n.0 == 64).collect();
    assert_eq!(low.len(), 1);
    assert_eq!((low[0].1, low[0].2), (0, q));
    assert_eq!(all.iter().filter(|n| n.0 == 74).count(), 4);
}

#[test]
fn a_short_note_shares_itself_out() {
    // A sixteenth trilled: four notes of a quarter of it each.
    let (q, all) = played(r"{ c''16\trill }");
    assert_eq!(keys(&all), vec![72, 74, 72, 74]);
    assert_eq!(all.last().unwrap().2, q / 4);
}

#[test]
fn a_note_with_no_ornament_is_played_as_before() {
    let (q, all) = played(r"{ c''4 <c' e'>4 }");
    assert_eq!(all, vec![(72, 0, q), (60, q, 2 * q), (64, q, 2 * q)]);
}
