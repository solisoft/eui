//! `\partCombine` (score #405): two parts on one staff — unisono with
//! "a2", chords, solos with "Solo" and "Solo II", two voices where they
//! part — and the commands that say how, `\partCombineUp` and `Down`; the
//! MIDI plays both parts.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing, clippy::panic, clippy::arithmetic_side_effects)]

use eui_score::parse::{parse, Ev, Kind, Parsed};
use eui_score::{engrave, midi, protocol, Layout, MidiOptions};

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

fn read(src: &str) -> Parsed {
    parse(src).unwrap_or_else(|e| panic!("{src}: {e}"))
}

/// The staff's notes and rests, what each says over it, and its voice.
fn items(p: &Parsed) -> Vec<&Ev> {
    p.staves[0].events.iter().filter(|e| matches!(e.k, Kind::Note | Kind::Rest)).collect()
}

fn said(ev: &Ev) -> Vec<String> {
    ev.texts.iter().map(|w| w.text.clone()).collect()
}

fn combined(one: &str, two: &str) -> Parsed {
    read(&format!("\\new Staff \\partCombine {{ {one} }} {{ {two} }}"))
}

#[test]
fn the_same_notes_are_one_with_a2_when_they_come_together() {
    let p = combined("c''4 d'' e'' f''", "c''4 d'' e'' f''");
    let it = items(&p);
    assert_eq!(it.len(), 4, "one note for both");
    assert_eq!(said(it[0]), vec!["a2"], "a2 over the first, as they start together");
    assert!(it[1..].iter().all(|e| e.texts.is_empty()));
    assert!(it.iter().all(|e| e.voice.is_none()));
    assert!(!p.staves[0].events.iter().any(|e| e.k == Kind::Voices));
    // Apart, then together again: a2 where they join.
    let p = combined("c''4 d''8 e'' f''4 g''", "c'4 b4 f''4 g''");
    let it = items(&p);
    let a2: Vec<usize> = it.iter().enumerate().filter(|(_, e)| said(e) == vec!["a2"]).map(|(k, _)| k).collect();
    assert_eq!(a2.len(), 1);
    assert_eq!(it[a2[0]].pitches[0].d, eui_score::parse::Pitch { d: 17, alter: 0 }.d, "on the f'' they join on");
    assert_eq!(protocol("\\partCombine { c'4 } { c'4 }"), Ok(11));
    // A dynamic of either part is the note's.
    let it_p = combined("c''4", "c''4\\p");
    assert_eq!(items(&it_p)[0].dynamic.as_deref(), Some("p"));
}

#[test]
fn the_same_rhythm_close_together_is_one_chord_and_crossing_parts_are_two_voices() {
    let p = combined("e''4 f''2.", "c''4 d''2.");
    let it = items(&p);
    assert_eq!(it.len(), 2);
    assert_eq!(it[0].pitches.len(), 2, "c'' and e'' on one stem");
    assert!(it.iter().all(|e| e.voice.is_none() && e.texts.is_empty()));
    // Part II over part I: two voices, I stems up, II down.
    let p = combined("c''4 d''", "e''4 f''");
    let ev = &p.staves[0].events;
    assert_eq!(ev.iter().filter(|e| e.k == Kind::Voices).count(), 1);
    let voices: Vec<Option<u8>> = items(&p).iter().map(|e| e.voice).collect();
    assert_eq!(voices, vec![Some(1), Some(1), Some(2), Some(2)]);
    // More than a ninth apart: two voices.
    let p = combined("e''4", "c'4");
    assert_eq!(items(&p).iter().map(|e| e.voice).collect::<Vec<_>>(), vec![Some(1), Some(2)]);
    // Different rhythms: two voices for as long as they differ.
    let p = combined("c''2 d''4 e''", "a'4 b' c''2");
    let voices: Vec<Option<u8>> = items(&p).iter().map(|e| e.voice).collect();
    assert_eq!(voices, vec![Some(1), Some(1), Some(1), Some(2), Some(2), Some(2)]);
    let eng = engrave("\\partCombine { c''2 d''4 e'' } { a'4 b' c''2 }", 900.0 / 8.0, &Layout::default()).unwrap();
    let ups: Vec<(Option<u8>, bool)> = eng.staves[0][0].items.iter().map(|e| (e.voice, e.up)).collect();
    assert!(ups.iter().all(|(v, up)| *up == (*v == Some(1))), "{ups:?}");
}

#[test]
fn one_part_resting_is_a_solo_and_both_resting_one_rest() {
    let p = combined("c''4 d'' r2", "r4 r e'' f''");
    let it = items(&p);
    let words: Vec<Vec<String>> = it.iter().map(|e| said(e)).collect();
    assert_eq!(it.len(), 4, "c'' d'' then e'' f'': no rests drawn for the part resting");
    assert_eq!(words[0], vec!["Solo"]);
    assert_eq!(words[2], vec!["Solo II"]);
    assert!(words[1].is_empty() && words[3].is_empty());
    // Both resting: one rest; and a solo again after it says so again only if it changes.
    let p = combined("c''4 r d'' e''", "r4 r r f''");
    let it = items(&p);
    assert_eq!(it.iter().filter(|e| e.k == Kind::Rest).count(), 1);
    assert_eq!(said(it[0]), vec!["Solo"]);
    assert!(said(it[2]).is_empty(), "still part I alone after the rest both share");
    // Whole bars of rest, R1*2, in one part: the other plays its solo.
    let p = combined("c''1 d''", "R1*2");
    assert_eq!(said(items(&p)[0]), vec!["Solo"]);
    // A part longer than the other plays alone at its end.
    let p = combined("c''4 d'' e'' f''", "c''4 d''");
    let it = items(&p);
    assert_eq!(said(it[2]), vec!["Solo"]);
}

#[test]
fn the_commands_say_how_from_where_they_are_written() {
    // \partCombineApart: two voices though they are the same.
    let p = combined("\\partCombineApart c''4 d'' \\partCombineAutomatic e'' f''", "c''4 d'' e'' f''");
    let voices: Vec<Option<u8>> = items(&p).iter().map(|e| e.voice).collect();
    assert_eq!(voices, vec![Some(1), Some(1), Some(2), Some(2), None, None]);
    assert_eq!(said(items(&p)[4]), vec!["a2"]);
    // \partCombineChords: one chord though they are far apart.
    let p = combined("\\partCombineChords e''4", "c'4");
    assert_eq!(items(&p).len(), 1);
    assert_eq!(items(&p)[0].pitches.len(), 2);
    // \partCombineUnisono: part I's notes, a2.
    let p = combined("\\partCombineUnisono c''4 d''", "c''4 b'");
    let it = items(&p);
    assert_eq!(it.len(), 2);
    assert_eq!(said(it[0]), vec!["a2"]);
    // \partCombineSoloI and II: one part, whatever the other plays.
    let p = combined("\\partCombineSoloI c''4 d'' \\partCombineSoloII e'' f''", "a'4 b' c'' d''");
    let it = items(&p);
    let firsts: Vec<i64> = it.iter().map(|e| e.pitches[0].d).collect();
    assert_eq!(firsts, vec![14, 15, 14, 15], "c'' d'' from part I, c'' d'' from part II");
    assert_eq!((said(it[0]), said(it[2])), (vec!["Solo".to_owned()], vec!["Solo II".to_owned()]));
    // In part II, too.
    let p = combined("c''4 d''", "\\partCombineApart c''4 d''");
    assert!(items(&p).iter().all(|e| e.voice.is_some()));
    // The old spelling.
    let p = read("\\partcombine { c''4 } { c''4 }");
    assert_eq!(said(items(&p)[0]), vec!["a2"]);
    // Refused: one part, or voices inside a part.
    assert!(parse("\\partCombine { c'4 }").is_err());
    assert!(parse("\\partCombine { << { c''4 } \\\\ { a'4 } >> } { c'4 }").unwrap_err().message.contains("voices inside a part"));
}

#[test]
fn up_and_down_pairs_share_a_staff_four_voices_on_it() {
    let src = "\\new Staff << \\partCombineUp { c''4 d'' e''2 } { a'8 b' b'4 g'2 } \\partCombineDown { e'4 f' c'2 } { c'8 d' d'4 a2 } >>";
    let p = read(src);
    assert!(p.warnings.is_empty());
    let eng = engrave(src, 900.0 / 8.0, &Layout::default()).unwrap();
    let bar = &eng.staves[0][0];
    let mut by_voice: Vec<(u8, bool)> = bar.items.iter().filter(|e| e.k == Kind::Note).map(|e| (e.voice.unwrap_or(0), e.up)).collect();
    by_voice.sort_unstable();
    by_voice.dedup();
    assert_eq!(by_voice, vec![(1, true), (2, false), (3, true), (4, false)], "the upper pair's stems up, the lower's down");
    // Alone on a staff: its stems the way it says.
    let eng = engrave("\\partCombineDown { c''4 d'' } { c''4 d'' }", 900.0 / 8.0, &Layout::default()).unwrap();
    assert!(eng.staves[0][0].items.iter().all(|e| !e.up));
}

#[test]
fn the_midi_plays_both_parts() {
    let ours = |src: &str| notes(&midi(src, &MidiOptions::default()).unwrap()).1.iter().map(|n| (n.0, n.1)).collect::<Vec<_>>();
    let both = ours("<< \\new Staff { c''4 d'' e'' r } \\new Staff { a'4 b'8 c'' r4 g' } >>");
    let mut combined = ours("\\partCombine { c''4 d'' e'' r } { a'4 b'8 c'' r4 g' }");
    combined.sort_unstable();
    let mut both = both;
    both.sort_unstable();
    assert_eq!(combined, both);
    // In unison both parts sound, as LilyPond's MIDI plays them: the note
    // drawn once is struck by each (score #412).
    assert_eq!(ours("\\partCombine { c''4 } { c''4 }"), vec![(72, 0), (72, 0)]);
}

/// Notes played: `(key, on, off)`.
type Played = Vec<(u8, u32, u32)>;

/// Every note of `src`, in time order.
fn played(src: &str) -> Played {
    let mut all = notes(&midi(src, &MidiOptions::default()).unwrap()).1;
    all.sort_unstable_by_key(|n| (n.1, n.0, n.2));
    all
}

/// The two parts, combined on one staff and written on two staves of their own.
fn both_ways(one: &str, two: &str) -> (Played, Played) {
    (played(&format!("\\new Staff \\partCombine {{ {one} }} {{ {two} }}")), played(&format!("<< \\new Staff {{ {one} }} \\new Staff {{ {two} }} >>")))
}

#[test]
fn what_the_staff_leaves_out_is_played_as_two_staves_play_it() {
    // A solo forced while the other part plays: the silenced part sounds.
    let (together, apart) = both_ways("\\partCombineSoloI c''4 d'' \\partCombineSoloII e'' f''", "a'4 b' c'' d''");
    assert_eq!(together, apart);
    assert_eq!(together.len(), 8, "four notes a part");
    // The part left out has its own rhythm, and ties.
    let (together, apart) = both_ways("\\partCombineSoloI c''2 d''", "a'8 b' c''4~ c''2");
    assert_eq!(together, apart);
    assert!(together.contains(&(72, 6720, 26880)), "c'' held over the tie, as one note: {together:?}");
    // Unisono forced on different notes: part II's own notes sound.
    let (together, apart) = both_ways("\\partCombineUnisono c''4 d''", "c''4 b'");
    assert_eq!(together, apart);
    assert!(together.iter().any(|n| n.0 == 71), "b' played though it is not drawn");
    // Unisono found by the combiner: each part strikes the note.
    let (together, apart) = both_ways("c''4 d'' e''2", "c''4 d'' e''2");
    assert_eq!(together, apart);
    assert_eq!(together.len(), 6);
    // A solo over the other part's rest, and over a whole bar's.
    let (together, apart) = both_ways("\\partCombineSoloI r4 d'' r2 R1", "c''4 r f''2 g''1");
    assert_eq!(together, apart);
    assert_eq!(together.iter().map(|n| n.0).collect::<Vec<_>>(), vec![72, 74, 77, 79]);
    // A staccato of the part left out is its own.
    let (together, apart) = both_ways("\\partCombineSoloI c''2 c''", "e''4-. f'' g''2");
    assert_eq!(together, apart);
    // Nothing is played twice that was written once: chords and two voices.
    let (together, apart) = both_ways("e''4 f''2. c''4 d''", "c''4 d''2. e''8 f'' g''4");
    assert_eq!(together, apart);
    // What is drawn is unchanged: the solo still shows one part.
    let p = combined("\\partCombineSoloI c''4 d''", "a'4 b'");
    let it = items(&p);
    assert_eq!(it.iter().map(|e| e.pitches[0].d).collect::<Vec<_>>(), vec![14, 15]);
    assert_eq!(it[0].unseen.iter().map(|(t, e)| (*t, e.pitches[0].d)).collect::<Vec<_>>(), vec![(0, 12)], "a' kept to be played, from where it starts");
    assert_eq!(it[1].unseen.iter().map(|(t, e)| (*t, e.pitches[0].d)).collect::<Vec<_>>(), vec![(0, 13)]);
    // A note cut at a bar line keeps what it plays on its first piece only:
    // played once, not once a piece.
    let (together, apart) = both_ways("\\partCombineSoloI c''1. d''2", "a'1. b'2");
    assert_eq!(together, apart);
    assert_eq!(together.iter().filter(|n| n.0 == 69).count(), 1);
}
