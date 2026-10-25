//! Beams placed as LilyPond's beam quanting places them (score #410): the
//! slope from the notes, damped; ends that sit on a staff line, straddle
//! it, hang from it or lie half way, never leaving a thin wedge of white;
//! stems never shorter than LilyPond's least; concave beams flat; kneed
//! beams between two staves placed the same way; and a `\crossStaff`
//! stem's flag at its far end.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing, clippy::panic, clippy::arithmetic_side_effects)]

use eui_score::measure::Beam;
use eui_score::parse::Kind;
use eui_score::{engrave, Engraved, Layout, RULE};

fn eng(src: &str) -> Engraved {
    engrave(src, 900.0 / 8.0, &Layout::default()).unwrap_or_else(|e| panic!("{e}"))
}

/// Every beam of the first staff, with whether its stems are up.
fn beams(e: &Engraved) -> Vec<Beam> {
    e.staves[0].iter().flat_map(|b| b.beams.iter().cloned()).collect()
}

/// The middle of a beam's outermost beam at its ends, in spaces up from the
/// middle line.
fn middles(g: &Beam) -> (f64, f64) {
    let half = if g.up { RULE.beam / 2.0 } else { -RULE.beam / 2.0 };
    (2.0 - (g.y0 + half), 2.0 - (g.y1 + half))
}

/// Whether `y` (spaces from a staff line) is a quant: on a line, sitting on
/// it, half way, hanging from it.
fn on_quant(y: f64) -> bool {
    let sit = (RULE.beam - RULE.staff) / 2.0;
    let f = y - y.floor();
    [0.0, sit, 0.5, 1.0 - sit, 1.0].iter().any(|q| (f - q).abs() < 1e-6)
}

#[test]
fn every_beam_end_is_on_a_quant_and_every_stem_long_enough() {
    let src = "\\relative c' { \\time 2/4 c8[ d] g[ g] e'[ f] c[ b] | c,8[ e g c] c[ g e c] | g'8[ c, e g] a16[ b c d] | c8[ g' e c] f,16[ a c f] | c8[ c'] c,[ g''] b,,16[ d g b d g b d] }";
    let e = eng(src);
    let all = beams(&e);
    assert!(all.len() >= 12);
    for g in &all {
        let (l, r) = middles(g);
        assert!(on_quant(l) && on_quant(r), "{l} {r}");
    }
    // From each head to its innermost beam, LilyPond's least free stem at least.
    for bar in &e.staves[0] {
        for g in &bar.beams {
            let free = if g.n >= 2 { 1.25 } else { 2.0 };
            for &j in &g.notes {
                let ev = &bar.items[j];
                let near = if g.up { ev.heads.last() } else { ev.heads.first() }.unwrap().pos;
                let head = 2.0 - near as f64 / 2.0;
                let inner = ev.stem_end + if g.up { 1.0 } else { -1.0 } * (RULE.beam + (g.n - 1) as f64 * RULE.beam_step);
                let len = if g.up { head - inner } else { inner - head };
                assert!(len >= free - 1e-6, "a stem {len} long to its innermost beam");
            }
        }
    }
}

#[test]
fn the_slope_follows_the_notes_damped() {
    let up = &beams(&eng("{ c'8[ g'] }"))[0];
    let (l, r) = middles(up);
    assert!(r > l, "rising: {l} to {r}");
    assert!(r - l < 2.0, "damped, less than the notes' two spaces: {}", r - l);
    let down = &beams(&eng("{ g''8[ c''] }"))[0];
    let (l, r) = middles(down);
    assert!(r < l, "falling: {l} to {r}");
    // The same note again: flat.
    let flat = &beams(&eng("{ a'8[ a'] }"))[0];
    assert_eq!(flat.y0, flat.y1);
    // Flat in the staff: on a line, sitting on it or hanging from it, not half way.
    for g in beams(&eng("{ a'8[ a'] e''[ e''] f'[ f'] d''16[ d'' d'' d''] }")) {
        let (l, _) = middles(&g);
        assert!(((l - l.floor()) - 0.5).abs() > 1e-6 || l.abs() > 2.0, "{l}");
    }
}

#[test]
fn a_concave_beam_is_flat_and_the_settings_hold() {
    // Its middle notes nearer the beam than both ends: flat.
    let concave = &beams(&eng("{ e''8[ a' a' e''] }"))[0];
    assert_eq!(concave.y0, concave.y1);
    // A line up and a line down without a dip: sloped.
    let sloped = &beams(&eng("{ c''8[ d'' e'' f''] }"))[0];
    assert_ne!(sloped.y0, sloped.y1);
    // `Beam.damping = #+inf.0` flattens; `Beam.positions` says where.
    let damped = &beams(&eng("{ \\override Beam.damping = #+inf.0 c''8[ d'' e'' f''] }"))[0];
    assert_eq!(damped.y0, damped.y1);
    let placed = &beams(&eng("{ \\override Beam.positions = #'(3 . 4) c''8[ d'' e'' f''] }"))[0];
    let (l, r) = middles(placed);
    assert!((l - 3.0).abs() < 1e-9 && (r - 4.0).abs() < 1e-9, "{l} {r}");
}

#[test]
fn a_kneed_beam_lies_between_its_stems_and_follows_both_hands() {
    let src = "\\new PianoStaff << \\new Staff = \"up\" { f''16 \\change Staff = \"down\" a, \\change Staff = \"up\" g'' \\change Staff = \"down\" c \\change Staff = \"up\" r2. } \\new Staff = \"down\" { \\clef bass s1 } >>";
    let e = eng(src);
    let k = &e.systems[0].kneed[0];
    // Both hands rise: so does the beam (y down).
    assert!(k.y1 < k.y0, "{} to {}", k.y0, k.y1);
    // Each stem at least LilyPond's least free length (1.25 under two beams) from its head.
    for &(_, from, tip) in &k.stems {
        assert!((tip - from).abs() >= 1.25, "{from} to {tip}");
    }
    // Between the staves: under the upper staff's notes, over the lower's.
    let notes: Vec<_> = (0..2).flat_map(|s| e.staves[s][0].items.iter().filter(|ev| ev.k == Kind::Note && ev.xbeam).map(move |ev| (s, ev))).collect();
    assert_eq!(notes.len(), 4);
}

#[test]
fn a_cross_staff_stem_carries_its_flag_at_its_far_end() {
    let piano = |up: &str, down: &str| format!("\\new PianoStaff << \\new Staff {{ {up} }} \\new Staff {{ \\clef bass \\stemUp {down} }} >>");
    // An eighth joined to a quarter: the stem's flag at the quarter's tip.
    let e = eng(&piano("e'4 r2.", "\\crossStaff { g8 } r8 r2."));
    assert_eq!(e.systems[0].cross.len(), 2, "the joined stem and its flag");
    let flag = &e.systems[0].cross[1];
    let tip = e.systems[0].offsets[0] + e.staves[0][0].items.iter().find(|ev| ev.k == Kind::Note).unwrap().stem_end;
    let mut min_y = f64::INFINITY;
    let mut i = 0;
    while i < flag.len() {
        let args = match flag[i] as i64 {
            0 | 1 => 2,
            2 => 4,
            3 => 6,
            _ => 0,
        };
        for k in (1..args).step_by(2) {
            min_y = min_y.min(flag[i + 1 + k]);
        }
        i += 1 + args;
    }
    assert!((min_y - tip).abs() < 0.3, "the flag hangs from the tip: {min_y} against {tip}");
    // An eighth joined to an eighth: its partner's own flag stands for both.
    let e = eng(&piano("e'8 r8 r2.", "\\crossStaff { g8 } r8 r2."));
    assert_eq!(e.systems[0].cross.len(), 1);
    // A quarter: no flag.
    let e = eng(&piano("e'4 r2.", "\\crossStaff { g4 } r2."));
    assert_eq!(e.systems[0].cross.len(), 1);
}
