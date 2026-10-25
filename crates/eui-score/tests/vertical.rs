//! A page's vertical spacing as LilyPond's page layout sets it (score
//! #409): the spacing alists read whole (`basic-distance`,
//! `minimum-distance`, `padding`, `stretchability`) for the page's top and
//! bottom, between systems and between staves; a page too full squeezed
//! towards the least distances before a system goes to the next page; a
//! page with room stretched in proportion to the stretchabilities, unless
//! it is ragged; the ink kept apart column by column, as skylines keep it.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing, clippy::panic, clippy::arithmetic_side_effects)]

use eui_score::parse::parse;
use eui_score::{engrave, engrave_paper, Engraved, Layout};

/// `bars` bars of quarters, each `per` bars a system, under `paper` (and
/// the titles `header` says).
fn tune(header: &str, paper: &str, bars: usize, per: usize) -> String {
    let mut music = String::new();
    for b in 0..bars {
        music.push_str("c'4 d' e' f' ");
        if b + 1 < bars {
            music.push_str(if (b + 1) % per == 0 { "\\break " } else { "| " });
        }
    }
    format!("\\header {{ {header} }}\n\\paper {{ {paper} }}\n{{ {music} }}")
}

fn paper(src: &str) -> Engraved {
    engrave_paper(src).unwrap_or_else(|e| panic!("{e}")).0
}

/// The distances from each system's top line to the next one's, page by page.
fn steps(e: &Engraved) -> Vec<Vec<f64>> {
    e.pages.iter().map(|p| p.systems.windows(2).map(|w| e.systems[w[1]].top - e.systems[w[0]].top).collect()).collect()
}

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-6
}

#[test]
fn the_spacing_alists_are_read_whole_and_key_by_key() {
    let p = parse(&tune(
        "",
        "top-system-spacing = #'((basic-distance . 3) (minimum-distance . 1) (padding . 4) (stretchability . 7)) \
         last-bottom-spacing.padding = #2 last-bottom-spacing.stretchability = #0",
        2,
        2,
    ))
    .unwrap();
    assert!(p.warnings.is_empty(), "read, not passed over: {:?}", p.warnings);
    let top = p.paper.top_spacing;
    assert_eq!((top.basic, top.minimum, top.padding, top.stretch), (Some(3.0), Some(1.0), Some(4.0), Some(7.0)));
    let bottom = p.paper.last_bottom_spacing;
    assert_eq!((bottom.basic, bottom.padding, bottom.stretch), (None, Some(2.0), Some(0.0)));
    // A staff's and a staff group's spacing, in `\layout`, `\with` or the music.
    let src = "\\layout { \\context { \\Staff \\override VerticalAxisGroup.staff-staff-spacing = #'((basic-distance . 14) (padding . 2)) } }\n\
               << \\new StaffGroup \\with { \\override StaffGrouper.staffgroup-staff-spacing.basic-distance = #15 } << \\new Staff { c'1 } \\new Staff { c'1 } >> \\new Staff { c'1 } >>";
    assert!(parse(src).unwrap().warnings.is_empty(), "{:?}", parse(src).unwrap().warnings);
    // An alist's key it does not know, or a property it has no hook for, still warns.
    let odd = parse("\\new Staff \\with { \\override VerticalAxisGroup.nonstaff-relatedstaff-spacing.padding = #2 } { c'1 }").unwrap();
    assert_eq!(odd.warnings.len(), 1, "{:?}", odd.warnings);
}

#[test]
fn a_crowded_page_squeezes_towards_the_least_distance_before_a_system_moves_on() {
    // Twenty spaces apart at rest, ten at the least: about seven systems a
    // page at rest, more when squeezed.
    let spacing = |least: f64| format!("system-system-spacing = #'((basic-distance . 20) (minimum-distance . {least}) (padding . 1) (stretchability . 60))");
    let squeezable = paper(&tune("", &spacing(10.0), 40, 2));
    let rigid = paper(&tune("", &spacing(20.0), 40, 2));
    let at_rest = rigid.pages[0].systems.len();
    assert!(squeezable.pages[0].systems.len() > at_rest, "{} systems against {at_rest}", squeezable.pages[0].systems.len());
    assert!(squeezable.pages.len() < rigid.pages.len());
    // Squeezed, never past the least distance; the rigid ones at rest, or stretched.
    let pages = steps(&squeezable);
    for step in &pages[0] {
        assert!(*step >= 10.0 - 1e-6 && *step < 20.0, "{step}");
    }
    assert!(steps(&rigid).iter().flatten().all(|s| *s >= 20.0 - 1e-6));
    // Every page fits: no system's ink past the bottom margin.
    for e in [&squeezable, &rigid] {
        let floor = engrave_paper(&tune("", "", 1, 1)).unwrap().1;
        let bottom = floor.h - floor.bottom;
        assert!(e.systems.iter().all(|s| s.bottom <= bottom + 1e-6));
    }
}

#[test]
fn a_page_with_room_stretches_each_distance_by_its_stretchability() {
    // Three systems, then a page break: the first page has room.
    let src = |paper: &str| format!("\\paper {{ {paper} }}\n{{ c'4 d' e' f' \\break c'4 d' e' f' \\break c'4 d' e' f' \\pageBreak c'4 d' e' f' \\break c'4 d' e' f' }}");
    let free = paper(&src(""));
    assert_eq!(free.pages.len(), 2);
    let page1 = &steps(&free)[0];
    assert_eq!(page1.len(), 2);
    assert!(page1.iter().all(|s| *s > 12.0 + 1.0), "stretched past the basic distance: {page1:?}");
    assert!(near(page1[0], page1[1]), "evenly: {page1:?}");
    // The last page is ragged: at rest, twelve spaces between its systems.
    assert!(steps(&free)[1].iter().all(|s| near(*s, 12.0)), "{:?}", steps(&free)[1]);
    // The systems take sixty parts of the stretch each, the bottom thirty
    // and the top one: the last system is not on the bottom margin.
    let last = &free.systems[free.pages[0].systems[2]];
    let first = &free.systems[0];
    let floor = engrave_paper(&src("")).unwrap().1;
    let room_below = floor.h - floor.bottom - last.bottom;
    let stretched = page1[0] - 12.0;
    assert!(room_below > stretched * 0.4 && room_below < stretched * 0.6, "{room_below} under, {stretched} between");
    assert!(first.top > 0.0);
    // `ragged-bottom`: no page stretched.
    let ragged = paper(&src("ragged-bottom = ##t"));
    assert!(steps(&ragged).iter().flatten().all(|s| near(*s, 12.0)));
    // `ragged-last-bottom = ##f`: the last page too.
    let filled = paper(&src("ragged-last-bottom = ##f"));
    assert!(steps(&filled)[1].iter().all(|s| *s > 13.0), "{:?}", steps(&filled)[1]);
    // No stretchability between systems: they stay at rest, the room goes
    // to the top and the bottom.
    let stiff = paper(&src("system-system-spacing.stretchability = #0"));
    assert!(steps(&stiff)[0].iter().all(|s| near(*s, 12.0)), "{:?}", steps(&stiff)[0]);
    assert!(stiff.systems[0].top > free.systems[0].top + 1.0);
}

#[test]
fn the_top_and_the_bottom_of_a_page_keep_their_padding() {
    let base = paper(&tune("", "", 2, 1));
    let padded = paper(&tune("", "top-system-spacing.padding = #5", 2, 1));
    assert!(near(padded.systems[0].top - base.systems[0].top, 4.0), "{} then {}", base.systems[0].top, padded.systems[0].top);
    // A bottom padding that leaves no room for a fourth system on the page.
    let many = paper(&tune("", "", 40, 2));
    let per = many.pages[0].systems.len();
    let tight = paper(&tune("", "last-bottom-spacing = #'((basic-distance . 60) (minimum-distance . 60) (padding . 1))", 40, 2));
    assert!(tight.pages[0].systems.len() < per, "{} against {per}", tight.pages[0].systems.len());
}

/// Each staff's top line from the system's first, on the screen.
fn offsets(src: &str) -> Vec<f64> {
    engrave(src, 900.0 / 8.0, &Layout::default()).unwrap_or_else(|e| panic!("{e}")).systems[0].offsets.clone()
}

#[test]
fn staves_are_spaced_by_their_group_and_their_settings() {
    // In no group: nine spaces; in a group: nine; from a group's last
    // staff to a staff outside it: ten and a half (LilyPond's
    // `staffgroup-staff-spacing`).
    let src = "<< \\new StaffGroup << \\new Staff { c''1 } \\new Staff { c''1 } >> \\new Staff { c''1 } \\new PianoStaff << \\new Staff { c''1 } \\new Staff { \\clef bass c1 } >> >>";
    let o = offsets(src);
    let steps: Vec<f64> = o.windows(2).map(|w| w[1] - w[0]).collect();
    assert_eq!(steps, vec![9.0, 10.5, 9.0, 9.0]);
    // The group's settings, and a staff's own over them.
    let set = "<< \\new StaffGroup \\with { \\override StaffGrouper.staff-staff-spacing.basic-distance = #12 } << \\new Staff { c''1 } \\new Staff \\with { \\override VerticalAxisGroup.staff-staff-spacing = #'((basic-distance . 16)) } { c''1 } \\new Staff { c''1 } >> >>";
    let o = offsets(set);
    assert_eq!((o[1] - o[0], o[2] - o[1]), (12.0, 16.0));
    // In `\layout`, for every staff.
    let laid = "\\layout { \\context { \\Staff \\override VerticalAxisGroup.staff-staff-spacing.basic-distance = #13 } } << \\new Staff { c''1 } \\new Staff { c''1 } >>";
    assert_eq!(offsets(laid)[1], 13.0);
}

#[test]
fn ink_is_kept_apart_column_by_column() {
    // A low note in the upper staff's first bar, a high one in the lower
    // staff's second: nothing of the one is over the other, the staves at
    // rest, nine spaces apart.
    let apart = offsets("<< \\new Staff { g1 | r1 } \\new Staff { r1 | c'''1 } >>");
    assert_eq!(apart[1], 9.0);
    // Over one another: pushed apart, a space between their ink at least.
    let over = offsets("<< \\new Staff { g1 | r1 } \\new Staff { c'''1 | r1 } >>");
    // g under the treble staff, in the space under its second ledger line:
    // its head's foot 7 spaces down from the top line; c''' on its second
    // ledger line over the staff: its head's top 2.5 up; a space between.
    assert!(over[1] >= 7.0 + 1.0 + 2.5 - 0.1, "{}", over[1]);
    assert!(over[1] <= 7.0 + 1.0 + 2.5 + 0.6, "no more than the ink asks: {}", over[1]);
    assert!(over[1] > 9.0);
}

#[test]
fn a_crowded_page_squeezes_the_staves_of_its_systems_too() {
    let music = "c'4 d' e' f' | ".repeat(60);
    let src = |paper: &str| format!("\\paper {{ {paper} }} \\new StaffGroup << \\new Staff {{ {music} }} \\new Staff {{ {music} }} \\new Staff {{ {music} }} >>");
    let free = paper(&src(""));
    let first = &free.systems[0];
    let pressed = free.pages.iter().take(free.pages.len() - 1).any(|p| p.systems.iter().any(|&s| free.systems[s].offsets[1] < 9.0 - 1e-6));
    assert!(pressed, "a squeezed page draws its staves closer: {:?}", first.offsets);
    // Never closer than the least distance (seven in a group).
    assert!(free.systems.iter().all(|s| s.offsets.windows(2).all(|w| w[1] - w[0] >= 7.0 - 1e-6)));
    // The brackets' bar lines follow the staves where the page put them.
    for sys in &free.systems {
        for (s, line) in sys.lines.iter().enumerate().take(2) {
            assert!(near(line.connect_down, sys.offsets[s + 1] - sys.offsets[s] - 4.0), "{} {:?}", line.connect_down, sys.offsets);
        }
    }
}
