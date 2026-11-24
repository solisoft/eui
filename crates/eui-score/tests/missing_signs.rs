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

// ---- Ornaments and fermatas ----------------------------------------------------

#[test]
fn the_other_ornaments_and_fermatas_are_drawn_in_their_own_signs() {
    for (cmd, above, below) in [
        ("reverseturn", "ornamentTurnInverted", "ornamentTurnInverted"),
        ("prallprall", "ornamentTremblement", "ornamentTremblement"),
        ("prallmordent", "ornamentPrecompTrillWithMordent", "ornamentPrecompTrillWithMordent"),
        ("upprall", "ornamentPrecompSlideTrillDAnglebert", "ornamentPrecompSlideTrillDAnglebert"),
        ("lineprall", "ornamentPrecompAppoggTrill", "ornamentPrecompAppoggTrill"),
        ("snappizzicato", "pluckedSnapPizzicatoAbove", "pluckedSnapPizzicatoAbove"),
        ("veryshortfermata", "fermataVeryShortAbove", "fermataVeryShortBelow"),
        ("shortfermata", "fermataShortAbove", "fermataShortBelow"),
        ("longfermata", "fermataLongAbove", "fermataLongBelow"),
        ("verylongfermata", "fermataVeryLongAbove", "fermataVeryLongBelow"),
        ("henzeshortfermata", "fermataShortHenzeAbove", "fermataShortHenzeBelow"),
        ("henzelongfermata", "fermataLongHenzeAbove", "fermataLongHenzeBelow"),
    ] {
        let src = format!(r"{{ c''4\{cmd} c''4_\{cmd} }}");
        let e = eng(&src);
        let line = &e.systems[0].lines[0];
        let glyphs: Vec<(&str, f64)> = line.scripts.iter().map(|sc| (sc.glyph, sc.y)).collect();
        assert_eq!(glyphs.len(), 2, "{src}: {glyphs:?}");
        assert_eq!(glyphs[0].0, above, "{src}");
        assert!(glyphs[0].1 < 0.0, "{src}: over the staff");
        if above != below {
            assert_eq!(glyphs[1].0, below, "{src}: its own sign under the note");
            assert!(glyphs[1].1 > 4.0, "{src}: under the staff");
        }
        // The glyph is Emmentaler's, in the table.
        assert!(eui_score::glyphs::glyph(above).advance > 0.0, "{above}");
        paths(&e, 8.0, 0).unwrap();
    }
}

/// Score #1039: the rest of LilyPond's script.ly, which was refused.
#[test]
fn the_rest_of_lilyponds_scripts_are_read_and_drawn_where_it_sets_them() {
    // (command, its sign written with no side, over the staff?, its sign under)
    for (cmd, sign, over, under) in [
        ("upmordent", "scripts.upmordent", true, "scripts.upmordent"),
        ("downmordent", "scripts.downmordent", true, "scripts.downmordent"),
        ("downprall", "scripts.downprall", true, "scripts.downprall"),
        ("prallup", "scripts.prallup", true, "scripts.prallup"),
        ("pralldown", "scripts.pralldown", true, "scripts.pralldown"),
        ("haydnturn", "scripts.haydnturn", true, "scripts.haydnturn"),
        ("slashturn", "scripts.slashturn", true, "scripts.slashturn"),
        ("halfopen", "scripts.halfopen", true, "scripts.halfopen"),
        ("halfopenvertical", "scripts.halfopenvertical", true, "scripts.halfopenvertical"),
        ("thumb", "scripts.thumb", true, "scripts.thumb"),
        ("rheel", "scripts.dpedalheel", true, "scripts.dpedalheel"),
        ("rtoe", "scripts.dpedaltoe", true, "scripts.dpedaltoe"),
        ("lheel", "scripts.upedalheel", false, "scripts.upedalheel"),
        ("ltoe", "scripts.upedaltoe", false, "scripts.upedaltoe"),
        ("signumcongruentiae", "scripts.usignumcongruentiae", true, "scripts.dsignumcongruentiae"),
        ("semicirculus", "scripts.dsemicirculus", true, "scripts.dsemicirculus"),
        ("ictus", "scripts.ictus", false, "scripts.ictus"),
        ("varcoda", "scripts.varcoda", true, "scripts.varcoda"),
    ] {
        let src = format!(r"{{ c''4\{cmd} c''4_\{cmd} c''4^\{cmd} }}");
        let e = eng(&src);
        let glyphs: Vec<(&str, f64)> = e.systems[0].lines[0].scripts.iter().map(|sc| (sc.glyph, sc.y)).collect();
        assert_eq!(glyphs.len(), 3, "{src}: {glyphs:?}");
        assert_eq!(glyphs[0].0, sign, "{src}");
        assert_eq!(glyphs[0].1 < 2.0, over, "{src}: {glyphs:?}");
        assert_eq!(glyphs[1].0, under, "{src}");
        assert!(glyphs[1].1 > 2.0, "{src}: under when written _");
        assert!(glyphs[2].1 < 2.0, "{src}: over when written ^");
        assert!(eui_score::glyphs::glyph(sign).advance > 0.0, "{sign}: Emmentaler's, in the table");
        assert!(eui_score::glyphs::glyph(under).advance > 0.0, "{under}");
        paths(&e, 8.0, 0).unwrap();
        assert!(warnings(&src).unwrap().is_empty(), "{src}");
    }
    // The accentus and the circulus go to the head's side: under a note
    // whose stem is up, over one whose stem is down.
    for cmd in ["accentus", "circulus"] {
        let src = format!(r"{{ e'4\{cmd} e''4\{cmd} }}");
        let e = eng(&src);
        let ys: Vec<f64> = e.systems[0].lines[0].scripts.iter().map(|sc| sc.y).collect();
        assert_eq!(ys.len(), 2, "{src}");
        assert!(ys[0] > 2.0 && ys[1] < 2.0, "{src}: {ys:?}");
    }
    // \musicglyph knows them by LilyPond's names.
    eng(r#"{ c''4^\markup { \musicglyph "scripts.haydnturn" \musicglyph "scripts.thumb" \musicglyph "scripts.varcoda" } }"#);
}

#[test]
fn the_ornaments_with_a_hook_are_played() {
    let keys = |cmd: &str| played(&format!(r"{{ c''2\{cmd} }}")).iter().map(|n| n.0).collect::<Vec<u8>>();
    assert_eq!(keys("prallmordent"), vec![72, 74, 72, 71, 72]);
    assert_eq!(keys("upprall"), vec![71, 72, 74, 72, 74, 72], "from below");
    assert_eq!(keys("downprall"), vec![74, 72, 74, 72, 74, 72], "from above");
    assert_eq!(keys("lineprall"), vec![74, 72, 74, 72]);
    assert_eq!(keys("prallup"), vec![72, 74, 72, 74, 72, 71, 72], "left through the note below");
    assert_eq!(keys("pralldown"), vec![72, 74, 72, 74, 72]);
    assert_eq!(keys("upmordent"), vec![71, 72, 71, 72]);
    assert_eq!(keys("downmordent"), vec![74, 72, 71, 72]);
    assert_eq!(keys("haydnturn"), vec![74, 72, 71, 72], "as the turn");
    assert_eq!(keys("slashturn"), vec![71, 72, 74, 72], "as the turn the other way round");
    // The signs that say how to play, not what: the note alone.
    for cmd in ["halfopen", "thumb", "lheel", "rtoe", "ictus", "signumcongruentiae"] {
        assert_eq!(keys(cmd), vec![72], "{cmd}");
    }
    // A long ornament on a short note fits in it, the note itself last.
    let q = quarter(r"{ c'4 }");
    let short = played(r"{ c''16\prallup d''8. }");
    assert_eq!(short.iter().map(|n| n.0).collect::<Vec<_>>(), vec![72, 74, 72, 74, 72, 71, 72, 74]);
    assert!(short[..7].iter().all(|n| n.2 <= q / 4 && n.2 > n.1), "{short:?}");
    assert_eq!(short[7].1, q / 4);
}

#[test]
fn wide_ornaments_on_neighbouring_notes_keep_apart() {
    for (src, n) in [
        (r"{ c''4\prallmordent c''\upprall c''\lineprall c''\prallprall }", 4),
        (r"{ c''8\prallmordent c''\upprall c''\lineprall c''\prallprall c''\prallmordent c''\upprall c''\lineprall c''\prallprall }", 8),
    ] {
        let e = eng(src);
        let line = &e.systems[0].lines[0];
        let mut boxes: Vec<(f64, f64, bool)> = line
            .scripts
            .iter()
            .map(|sc| {
                let b = eui_score::glyphs::glyph(sc.glyph).bbox;
                let x0 = sc.x - (b[2] - b[0]) / 2.0;
                (x0, x0 + b[2] - b[0], sc.y < 2.0)
            })
            .collect();
        boxes.sort_by(|a, b| a.0.total_cmp(&b.0));
        assert_eq!(boxes.len(), n);
        for pair in boxes.windows(2) {
            if pair[0].2 == pair[1].2 {
                assert!(pair[1].0 - pair[0].1 >= 0.25, "{src}: {pair:?}");
            }
        }
    }
    // A staccato or an accent, no wider than its head, spaces nothing; nor a
    // fermata with no wide neighbour.
    for (bare, marked) in [(r"{ c''8 d'' e'' f'' g'' a'' b'' c''' }", r"{ c''8-. d''-> e''-. f''-> g''-- a''-. b''-> c'''-. }"), (r"{ c''8 d'' e'' f'' }", r"{ c''8 d''\fermata e'' f'' }")] {
        let xs = |src: &str| items(&eng(src)).iter().map(|ev| ev.x).collect::<Vec<_>>();
        assert_eq!(xs(bare), xs(marked), "{marked}");
    }
}

#[test]
fn a_reverse_turn_and_a_double_prall_are_played() {
    let q = quarter(r"{ c'4 }");
    let turn: Vec<u8> = played(r"{ c''4\reverseturn }").iter().map(|n| n.0).collect();
    assert_eq!(turn, vec![71, 72, 74, 72], "below, the note, above, the note");
    let prall = played(r"{ c''4\prallprall }");
    assert_eq!(prall.iter().map(|n| n.0).collect::<Vec<_>>(), vec![72, 74, 72, 74, 72]);
    assert_eq!((prall[4].1, prall[4].2), (q / 2, q), "in 32nds, the note held to its end");
}

#[test]
fn each_fermata_holds_as_long_as_it_says() {
    let q = quarter(r"{ c'4 }");
    // \fermata: half the note again, then two seconds (four quarters at 120).
    for (cmd, held, breath) in [
        ("fermata", q / 2, 4 * q),
        ("veryshortfermata", q / 8, q),
        ("shortfermata", q / 4, 2 * q),
        ("henzeshortfermata", q / 4, 2 * q),
        ("longfermata", 3 * q / 4, 6 * q),
        ("henzelongfermata", 3 * q / 4, 6 * q),
        ("verylongfermata", q, 8 * q),
    ] {
        let all = played(&format!(r"{{ c''4\{cmd} d''4 }}"));
        assert_eq!(all[0].2, q + held, "{cmd}: held");
        assert_eq!(all[1].1, q + held + breath, "{cmd}: then the breath");
    }
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

/// The places a glyph is drawn among `all`: each outline with its commands,
/// as (top, bottom) in the page's pixels.
fn glyph_drawn(all: &[Path], name: &str) -> Vec<(f64, f64)> {
    let g = &eui_score::glyphs::glyph(name).fill;
    let codes = |c: &[f64]| -> Vec<f64> {
        let mut out = Vec::new();
        let mut i = 0;
        while i < c.len() {
            out.push(c[i]);
            i += 1 + match c[i] as i64 {
                0 | 1 => 2,
                2 => 4,
                3 => 6,
                _ => 0,
            };
        }
        out
    };
    let want = codes(g);
    all.iter()
        .filter_map(|p| match p {
            Path::Fill(c) if c.len() == g.len() && codes(c) == want => {
                let mut top = f64::MAX;
                let mut bottom = f64::MIN;
                let mut i = 0;
                while i < c.len() {
                    let n = match c[i] as i64 {
                        0 | 1 => 2,
                        2 => 4,
                        3 => 6,
                        _ => 0,
                    };
                    for k in (0..n).step_by(2) {
                        top = top.min(c[i + 2 + k]);
                        bottom = bottom.max(c[i + 2 + k]);
                    }
                    i += 1 + n;
                }
                Some((top, bottom))
            }
            _ => None,
        })
        .collect()
}

#[test]
fn an_arpeggio_arrow_heads_its_line_and_a_down_arrow_rolls_from_the_top() {
    let src = r"{ \arpeggioArrowUp <c' e' g'>4\arpeggio \arpeggioArrowDown <c' e' g'>\arpeggio \arpeggioNormal <c' e' g'>\arpeggio }";
    let p = parse(src).unwrap();
    let styles: Vec<ArpStyle> = p.staves[0].events.iter().filter(|e| e.k == Kind::Note).map(|e| e.arp_style).collect();
    assert_eq!(styles, vec![ArpStyle::ArrowUp, ArpStyle::ArrowDown, ArpStyle::Wave]);
    assert!(warnings(src).unwrap().is_empty());
    for (one, name, up) in [(r"{ \arpeggioArrowUp <c' e' g'>4\arpeggio }", "wiggleArpeggiatoUpArrow", true), (r"{ \arpeggioArrowDown <c' e' g'>4\arpeggio }", "wiggleArpeggiatoDownArrow", false)] {
        let (all, _) = paths(&eng(one), 8.0, 0).unwrap();
        let arrows = glyph_drawn(&all, name);
        let wiggles = glyph_drawn(&all, "wiggleArpeggiatoUp");
        assert_eq!(arrows.len(), 1, "{one}");
        assert!(wiggles.len() >= 2, "{one}");
        let (top, bottom) = arrows[0];
        if up {
            assert!(wiggles.iter().all(|w| top < w.0), "{one}: the head over the line");
        } else {
            assert!(wiggles.iter().all(|w| bottom >= w.1 - 0.01), "{one}: the head under the line");
        }
    }
    assert!(glyph_drawn(&paths(&eng(r"{ <c' e' g'>4\arpeggio }"), 8.0, 0).unwrap().0, "wiggleArpeggiatoUpArrow").is_empty());
    let q = quarter(r"{ c'4 }");
    let up = played(r"{ \arpeggioArrowUp <c' e' g'>4\arpeggio }");
    assert_eq!(up.iter().map(|n| (n.0, n.1)).collect::<Vec<_>>(), vec![(60, 0), (64, q / 16), (67, q / 8)]);
    let mut down = played(r"{ \arpeggioArrowDown <c' e' g'>4\arpeggio }");
    down.sort_by_key(|n| n.1);
    assert_eq!(down.iter().map(|n| (n.0, n.1)).collect::<Vec<_>>(), vec![(67, 0), (64, q / 16), (60, q / 8)], "from the top");
    assert!(down.iter().all(|n| n.2 == q), "all held to the end");
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
    // Between halves and quarters the beams reach the stems; between whole
    // notes, which have none, they stand between the heads, short of both.
    for (src, to_stems) in [(r"{ \repeat tremolo 4 { c''16 a' } r2 }", true), (r"{ \repeat tremolo 2 { c''16 a' } r2. }", true), (r"{ \repeat tremolo 8 { c''16 a' } }", false)] {
        let e = eng(src);
        let g = &e.staves[0][0].beams[0];
        let strokes = beam_strokes(&e);
        assert_eq!(strokes.len(), 2, "{src}");
        for w in strokes {
            let between = (g.x1 - g.x0) * 8.0;
            if to_stems {
                assert!(w > between, "{src}: {w} against {between}");
            } else {
                let items = items(&e);
                let room = (items[1].x - items[0].x - items[0].head_w.unwrap()) * 8.0;
                assert!((w - (room - 0.8 * 8.0)).abs() < 0.5, "{src}: {w} against {room}");
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
fn the_beams_stand_by_the_heads() {
    // Whole notes: the stack of beams centred on the line from one head to
    // the other, within a space of it at either end.
    for src in [r"{ \repeat tremolo 8 { c'16 e' } }", r"{ \repeat tremolo 8 { c''16 a' } }", r"{ \repeat tremolo 8 { <c' e'>16 <a' c''> } }"] {
        let e = eng(src);
        let bar = &e.staves[0][0];
        let g = &bar.beams[0];
        let middle = |ev: &Ev| ev.heads.iter().map(|h| 2.0 - h.pos as f64 / 2.0).sum::<f64>() / ev.heads.len() as f64;
        let (a, b) = (&bar.items[g.notes[0]], &bar.items[g.notes[1]]);
        let stack = 0.48 + (g.n - 1) as f64 * 0.75;
        let dir = if g.up { 1.0 } else { -1.0 };
        for (edge, ev) in [(g.y0, a), (g.y1, b)] {
            let centre = edge + dir * stack / 2.0;
            assert!((centre - middle(ev)).abs() < 1.0, "{src}: the beams' middle {centre} against the heads' {}", middle(ev));
        }
    }
    // Halves, a chord's included: the beams at the stems' ends, a normal
    // stem's length from the heads they start from — not floating apart.
    for src in [r"{ \repeat tremolo 4 { c''16 a' } r2 }", r"{ \repeat tremolo 4 { <c'' e''>16 <a' d''> } r2 }", r"{ \repeat tremolo 4 { g'16 c'' } r2 }"] {
        let e = eng(src);
        let bar = &e.staves[0][0];
        let g = &bar.beams[0];
        for (edge, j) in [(g.y0, g.notes[0]), (g.y1, g.notes[1])] {
            let ev = &bar.items[j];
            assert!((ev.stem_end - edge).abs() < 1e-9, "{src}: the stem reaches the beam");
            let near = if g.up { ev.heads.last() } else { ev.heads.first() }.map(|h| 2.0 - h.pos as f64 / 2.0).unwrap();
            let length = (edge - near).abs();
            assert!((2.5..=4.5).contains(&length), "{src}: {length} spaces from the head");
        }
    }
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
    let src = r"{ c'2\sostenutoOn d'\unaCorda e'\sostenutoOff\sostenutoOn f'\sostenutoOff\treCorde }";
    let p = parse(src).unwrap();
    let notes: Vec<&Ev> = p.staves[0].events.iter().filter(|e| e.k == Kind::Note).collect();
    assert_eq!(notes[0].pedals, vec![(66, true)]);
    assert_eq!(notes[1].pedals, vec![(67, true)]);
    assert_eq!(notes[2].pedals, vec![(66, false), (66, true)], "let go, then pressed again");
    assert_eq!(notes[3].pedals, vec![(66, false), (67, false)]);
    let words = |ev: &Ev| -> Vec<(u8, Vec<String>)> { ev.texts.iter().map(|w| (w.pedal, w.runs.iter().map(|r| r.text.clone()).collect())).collect() };
    assert_eq!(words(notes[0]), vec![(66, vec!["Sost. Ped.".to_owned()])]);
    assert_eq!(words(notes[1]), vec![(67, vec!["una corda".to_owned()])]);
    assert_eq!(words(notes[2]), vec![(66, vec!["Sost. Ped.".to_owned()])], "let go by the bracket, pressed again in words");
    assert_eq!(words(notes[3]), vec![(67, vec!["tre corde".to_owned()])], "no \"*\": the bracket's hook says it");
    assert!(notes[0].texts[0].runs.iter().all(|r| r.weight == eui_score::glyphs::Weight::Italic && !r.glyph), "italic words, LilyPond's font-shape");
    // Drawn under the staff, each at its note.
    let e = eng(src);
    let line = &e.systems[0].lines[0];
    let placed = |t: &str| line.texts.iter().find(|tx| tx.runs.iter().any(|r| r.text == t)).unwrap_or_else(|| panic!("{t} not drawn"));
    let at_notes = items(&e);
    for (t, at) in [("Sost. Ped.", 0), ("una corda", 1), ("tre corde", 3)] {
        let tx = placed(t);
        assert!(tx.y > 4.0, "{t}: under the staff ({})", tx.y);
        assert!((tx.x0 - at_notes[at].x).abs() < 1.0, "{t}: at its note");
    }
    // The sostenuto's 'mixed style: from after its words to the note it is
    // let go with, hooked there; again from the words it is pressed with anew.
    let src = r"{ c'1\sostenutoOn d'1 e'1\sostenutoOff\sostenutoOn f'1 g'1\sostenutoOff }";
    let e = eng(src);
    let line = &e.systems[0].lines[0];
    let notes = items(&e);
    let brackets = &line.pedal_brackets;
    assert_eq!(brackets.len(), 2, "{brackets:?}");
    let sost = line.texts.iter().filter(|tx| tx.runs.iter().any(|r| r.text == "Sost. Ped.")).collect::<Vec<_>>();
    assert!(brackets[0].x0 > sost[0].x1 && (brackets[0].x1 - notes[2].x).abs() < 1e-9 && brackets[0].hook, "{brackets:?}");
    assert!(brackets[1].x0 > sost[1].x1 && (brackets[1].x1 - notes[4].x).abs() < 1e-9 && brackets[1].hook, "{brackets:?}");
    assert!(brackets.iter().all(|b| (b.y - sost[0].y).abs() < 1e-9), "on the words' baseline");
    let (all, _) = paths(&e, 8.0, 0).unwrap();
    let (plain, _) = paths(&eng(r"{ c'1 d'1 e'1 f'1 g'1 }"), 8.0, 0).unwrap();
    assert!(all.len() > plain.len() + 4, "the brackets and their hooks drawn");
}

#[test]
fn each_pedal_keeps_its_own_line_under_the_staff() {
    // Sustain nearest the staff, then the sostenuto, then the soft pedal, each
    // at one height along the line whatever stands over its words.
    let src = r"{ c'4\sustainOn\sostenutoOn\unaCorda d' e'\sustainOff\sustainOn f' | c4\sustainOff\treCorde d'' a,, b'\sostenutoOff }";
    let e = eng(src);
    let line = &e.systems[0].lines[0];
    let ys = |pick: &dyn Fn(&eui_score::marks::TextPlace) -> bool| -> Vec<f64> { line.texts.iter().filter(|tx| pick(tx)).map(|tx| tx.y).collect() };
    let sustain = ys(&|tx| tx.runs.iter().any(|r| r.glyph && r.text.starts_with("keyboardPedal")));
    let sost = ys(&|tx| tx.runs.iter().any(|r| r.text == "Sost. Ped."));
    let corda = ys(&|tx| tx.runs.iter().any(|r| r.text == "una corda" || r.text == "tre corde"));
    assert_eq!(sustain.len(), 3);
    assert_eq!(corda.len(), 2);
    for set in [&sustain, &corda] {
        assert!(set.iter().all(|y| (y - set[0]).abs() < 1e-9), "one height along the line: {set:?}");
    }
    assert!(sustain[0] < sost[0] && sost[0] < corda[0], "{} {} {}", sustain[0], sost[0], corda[0]);
    // Under the low note in the line, not into it.
    let lowest = items(&e).iter().map(|ev| ev.reach_down.unwrap_or(4.0)).fold(4.0, f64::max);
    let top_of_pedals = line.texts.iter().filter(|tx| tx.dir > 0).map(|tx| tx.top).fold(f64::MAX, f64::min);
    assert!(top_of_pedals > lowest, "{top_of_pedals} under {lowest}");
}

#[test]
fn a_sostenuto_held_over_a_line_break_goes_on_from_the_next_line_start() {
    let src = r"{ c'1\sostenutoOn \break d'1 e'1\sostenutoOff }";
    let e = eng(src);
    let first = &e.systems[0].lines[0];
    let second = &e.systems[1].lines[0];
    assert_eq!(first.pedal_brackets.len(), 1);
    assert!(!first.pedal_brackets[0].hook, "not let go on the first line");
    assert_eq!(second.pedal_brackets.len(), 1);
    assert!(second.pedal_brackets[0].hook);
    assert!(second.texts.iter().all(|tx| !tx.runs.iter().any(|r| r.text == "Sost. Ped.")), "the words once, where it is pressed");
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
    let ties = glyph_drawn(&tied, "lyricsElision");
    assert_eq!(ties.len(), 1, "Emmentaler's ties.lyric.default, not a \"~\"");
    assert!(glyph_drawn(&plain, "lyricsElision").is_empty());
    // A single letter between two ties: the short one, twice.
    let (short, _) = paths(&eng(r"{ c'2 d' } \addlyrics { e~a~o la }"), 8.0, 0).unwrap();
    assert_eq!(glyph_drawn(&short, "lyricsElisionNarrow").len(), 2);
    // It takes room: the tie and half a space either side.
    let wide = items(&eng(r"{ c'16 d' } \addlyrics { e~a la }"))[0].right;
    let narrow = items(&eng(r"{ c'16 d' } \addlyrics { ea la }"))[0].right;
    let tie = eui_score::glyphs::glyph("lyricsElision").bbox;
    assert!(wide > narrow + (tie[2] - tie[0]) / 2.0, "{wide} against {narrow}");
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

// ---- A voice that only waits ---------------------------------------------------

/// Score #1039: between `\\\\`, a part of spacers is a voice all the same.
#[test]
fn a_voice_that_only_waits_between_double_backslashes_is_a_voice() {
    // The first voice keeps its number through the bars where the second
    // says nothing: its stems go up there too, as LilyPond's \\voiceOne has them.
    let voices = |src: &str| -> Vec<Option<u8>> { parse(src).unwrap().staves[0].events.iter().filter(|e| e.k == Kind::Note).map(|e| e.voice).collect() };
    assert_eq!(voices(r"{ << { e'4 f' g' a' } \\ { s1 } >> }"), vec![Some(1); 4]);
    assert_eq!(voices(r"{ << { e'4 f' g' a' | b'1 } \\ { s1 | g'1 } >> }"), vec![Some(1), Some(1), Some(1), Some(1), Some(1), Some(2)]);
    let ups = |src: &str| -> Vec<bool> { items(&eng(src)).iter().filter(|ev| ev.k == Kind::Note).map(|ev| ev.up).collect() };
    assert_eq!(ups(r"{ << { c''4 d'' e'' f'' } \\ { s1 } >> }"), vec![true; 4], "stems up, where one voice alone would turn them down");
    assert_eq!(ups(r"{ c''4 d'' e'' f'' }"), vec![false; 4]);
    // It is drawn as nothing and played as nothing.
    assert_eq!(played(r"{ << { c''4 d'' e'' f'' } \\ { s1 } >> }").len(), 4);
    // Without \\\\, what does not sing is still the staff's settings beside one voice.
    assert_eq!(voices(r"{ << { \key g \major s1 } { e'4 f' g' a' } >> }"), vec![None; 4]);
    // Nothing but spacers on both sides draws no voices.
    assert!(parse(r"{ << { s1 } \\ { s1 } >> c'1 }").unwrap().staves[0].events.iter().all(|e| e.voice.is_none()));
}
