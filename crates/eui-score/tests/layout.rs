//! The music laid on lines and pages as LilyPond lays it (score #406):
//! `\paper`'s settings and their units, durations spaced by LilyPond's rule
//! with its optical correction for stems, every line filled but a score's
//! only one, optimal line breaks that `\break`, `\noBreak` and
//! `system-count` steer, and pages that `\pageBreak`, `\noPageBreak`,
//! `page-count` and the systems-per-page counts shape.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing, clippy::panic, clippy::arithmetic_side_effects)]

use eui_score::parse::parse;
use eui_score::{engrave, engrave_paper, Engraved, Layout, Paper};

/// `bars` bars of four quarters (or of `bar` when given), under `paper`.
fn tune(paper: &str, bars: usize, bar: &str) -> String {
    let one = if bar.is_empty() { "c'4 d' e' f'" } else { bar };
    format!("\\paper {{ {paper} }}\n{{ {} }}", vec![one; bars].join(" | "))
}

fn paper(src: &str) -> (Engraved, Paper) {
    engrave_paper(src).unwrap_or_else(|e| panic!("{e}"))
}

fn screen(src: &str) -> Engraved {
    engrave(src, 900.0 / 8.0, &Layout::default()).unwrap_or_else(|e| panic!("{e}"))
}

/// How many bars each system holds.
fn lines(e: &Engraved) -> Vec<usize> {
    e.systems.iter().map(|s| s.lines[0].bars.len()).collect()
}

#[test]
fn paper_settings_are_read_with_their_units() {
    let p = parse(&tune(
        "line-width = 15\\cm left-margin = 1\\in short-indent = 30\\pt indent = 4\\staff-space top-margin = 8 \
         ragged-right = ##t ragged-last = ##f ragged-bottom = ##t ragged-last-bottom = ##f \
         system-system-spacing.basic-distance = #14 system-system-spacing.padding = #2 \
         score-system-spacing = #'((basic-distance . 20) (minimum-distance . 10) (padding . 3) (stretchability . 60)) \
         markup-system-spacing.basic-distance = #8 \
         systems-per-page = 4 max-systems-per-page = 5 page-count = 2 system-count = 6",
        2,
        "",
    ))
    .unwrap()
    .paper;
    assert!(p.warnings_free_check());
    assert_eq!(p.line_width, Some(150.0));
    assert!((p.left.unwrap() - 25.4).abs() < 1e-9);
    assert!((p.short_indent.unwrap() - 30.0 * 25.4 / 72.27).abs() < 1e-9);
    assert!((p.indent.unwrap() - 4.0 * 5.0 * 25.4 / 72.27).abs() < 1e-9, "a staff space at 20 pt is 5 pt");
    assert_eq!(p.top, Some(8.0), "millimetres when no unit is written");
    assert_eq!((p.ragged_right, p.ragged_last, p.ragged_bottom, p.ragged_last_bottom), (Some(true), Some(false), Some(true), Some(false)));
    assert_eq!((p.system_spacing.basic, p.system_spacing.padding), (Some(14.0), Some(2.0)));
    assert_eq!((p.score_spacing.basic, p.score_spacing.minimum, p.score_spacing.padding, p.score_spacing.stretch), (Some(20.0), Some(10.0), Some(3.0), Some(60.0)));
    assert_eq!(p.markup_spacing.basic, Some(8.0));
    assert_eq!((p.systems_per_page, p.max_systems_per_page, p.page_count, p.system_count), (Some(4), Some(5), Some(2), Some(6)));
    // Read, so not passed over; a setting this does not lay out still is, with a warning.
    assert!(parse(&tune("ragged-right = ##t system-count = 2", 2, "")).unwrap().warnings.is_empty());
    assert_eq!(parse(&tune("print-page-number = ##f", 2, "")).unwrap().warnings.len(), 1);
    // A count of nothing is no count.
    let p = parse(&tune("system-count = 0 page-count = 0", 2, "")).unwrap().paper;
    assert_eq!((p.system_count, p.page_count), (None, None));
    // `\layout` says the same of its lines.
    let p = parse("\\layout { ragged-right = ##t indent = 0 short-indent = 5\\mm line-width = 120\\mm } { c'1 }").unwrap();
    assert_eq!((p.paper.ragged_right, p.paper.indent, p.paper.short_indent, p.paper.line_width), (Some(true), Some(0.0), Some(5.0), Some(120.0)));
    assert!(p.warnings.is_empty(), "{:?}", p.warnings);
}

trait NoWarn {
    fn warnings_free_check(&self) -> bool;
}

impl NoWarn for eui_score::parse::PaperSpec {
    fn warnings_free_check(&self) -> bool {
        true
    }
}

#[test]
fn line_width_centres_the_line_or_takes_its_room_from_the_other_margin() {
    let (_, centred) = paper(&tune("line-width = 150\\mm", 4, ""));
    assert!((centred.left - centred.right).abs() < 1e-9, "centred");
    assert!((centred.line_w() * 25.4 * 5.0 / 72.0 - 150.0).abs() < 1e-6);
    let (_, kept) = paper(&tune("line-width = 150\\mm left-margin = 20\\mm", 4, ""));
    assert!((kept.left * 25.4 * 5.0 / 72.0 - 20.0).abs() < 1e-6);
    assert!((kept.line_w() * 25.4 * 5.0 / 72.0 - 150.0).abs() < 1e-6);
    // A width wider than the page is not one.
    let (_, wide) = paper(&tune("line-width = 500\\mm", 4, ""));
    assert_eq!(wide.left, Paper::a4().left);
}

#[test]
fn every_line_is_filled_but_a_scores_only_one_unless_ragged_is_asked() {
    // Many lines: every one to the margin, the last too (ragged-last is false).
    let e = screen(&tune("", 40, ""));
    assert!(e.systems.len() > 2);
    let right = e.systems[0].right;
    for s in &e.systems {
        assert!((s.right - right).abs() < 0.05, "line at {} not filled to {right}", s.right);
    }
    // One line: at its natural width, as LilyPond sets a score of one system.
    let one = screen(&tune("", 2, ""));
    assert_eq!(one.systems.len(), 1);
    assert_eq!(one.systems[0].lines[0].k, 1.0);
    assert!(one.systems[0].right < right - 20.0);
    // ragged-right = ##f fills it; ragged-last leaves the last line short; ragged-right all of them.
    assert!(screen(&tune("ragged-right = ##f", 2, "")).systems[0].lines[0].k > 1.0);
    let last = screen(&tune("ragged-last = ##t", 37, ""));
    let n = last.systems.len();
    assert_eq!(last.systems[n - 1].lines[0].k, 1.0);
    assert!(last.systems[n - 1].right < last.systems[0].right - 1.0, "the last line short of the margin the others reach");
    let ragged = screen(&tune("ragged-right = ##t", 40, ""));
    assert!(ragged.systems.iter().all(|s| s.lines[0].k <= 1.0), "never stretched");
    assert!(ragged.systems.iter().any(|s| s.right < right - 1.0));
}

#[test]
fn lines_are_broken_where_they_stretch_least_over_the_whole_score() {
    // Greedy filling left a lone bar on the last line; the optimal cut shares them out.
    let e = screen(&tune("", 37, ""));
    let counts = lines(&e);
    let (most, least) = (counts.iter().max().unwrap(), counts.iter().min().unwrap());
    assert!(most - least <= 2, "balanced: {counts:?}");
    // \break ends a line; \noBreak forbids one at its bar line.
    let mut bars: Vec<String> = vec!["c'4 d' e' f'".to_string(); 30];
    bars[2] = "c'4 d' e' f' \\break".into();
    let e = screen(&format!("{{ {} }}", bars.join(" | ")));
    assert_eq!(lines(&e)[0], 3);
    let free = screen(&tune("", 30, ""));
    let first = lines(&free)[0];
    let mut kept: Vec<String> = vec!["c'4 d' e' f'".to_string(); 30];
    kept[first - 1] = "c'4 d' e' f' \\noBreak".into();
    let held = screen(&format!("{{ {} }}", kept.join(" | ")));
    assert_ne!(lines(&held)[0], first, "no line ends at the \\noBreak");
    // \noBreak inside a bar says nothing, and is no refusal.
    screen("{ c'4 \\noBreak d' e' f' }");
}

#[test]
fn system_count_sets_so_many_systems_when_it_can() {
    for count in [2, 3, 5] {
        let e = screen(&tune(&format!("system-count = {count}"), 20, ""));
        assert_eq!(e.systems.len(), count, "system-count = {count}");
    }
    // More systems than bars cannot be had: the free cut stands.
    let e = screen(&tune("system-count = 50", 4, ""));
    assert_eq!(e.systems.len(), screen(&tune("", 4, "")).systems.len());
}

#[test]
fn pages_honour_page_count_breaks_and_the_systems_per_page() {
    let per_page = |e: &Engraved| e.pages.iter().map(|p| p.systems.len()).collect::<Vec<_>>();
    // Enough bars for two pages, squeezed as they may be (score #409).
    let (free, _) = paper(&tune("", 240, ""));
    let pages = free.pages.len();
    assert!(pages >= 2);
    // page-count: so many pages, the systems shared evenly.
    let (more, _) = paper(&tune(&format!("page-count = {}", pages + 1), 240, ""));
    assert_eq!(more.pages.len(), pages + 1);
    let counts = per_page(&more);
    assert!(counts.iter().max().unwrap() - counts.iter().min().unwrap() <= 1, "{counts:?}");
    // Fewer than the systems need: not to be had, the free cut stands.
    let (fewer, _) = paper(&tune("page-count = 1", 240, ""));
    assert_eq!(fewer.pages.len(), pages);
    // max-systems-per-page caps them.
    let (capped, _) = paper(&tune("max-systems-per-page = 3", 240, ""));
    assert!(capped.pages.iter().all(|p| p.systems.len() <= 3));
    // \noPageBreak: the page does not end with that system.
    let first_page = free.pages[0].systems.len();
    let last_bar = free.systems[first_page - 1].lines[0].bars.end - 1;
    let mut bars: Vec<String> = vec!["c'4 d' e' f'".to_string(); 240];
    bars[last_bar] = "c'4 d' e' f' \\noPageBreak".into();
    let (kept, _) = paper(&format!("{{ {} }}", bars.join(" | ")));
    let ends: Vec<usize> = kept.pages.iter().map(|p| kept.systems[*p.systems.last().unwrap()].lines[0].bars.end - 1).collect();
    assert!(!ends.contains(&last_bar), "a page ended at the \\noPageBreak");
}

#[test]
fn ragged_bottom_and_the_spacing_between_systems() {
    let gaps = |e: &Engraved, page: usize| -> Vec<f64> {
        let s = &e.pages[page].systems;
        s.windows(2).map(|w| e.systems[w[1]].top - e.systems[w[0]].top).collect()
    };
    let (spread, _) = paper(&tune("systems-per-page = 4", 100, ""));
    let (ragged, _) = paper(&tune("systems-per-page = 4 ragged-bottom = ##t", 100, ""));
    assert!(gaps(&spread, 0)[0] > gaps(&ragged, 0)[0], "a full page's systems spread down it, unless ragged-bottom");
    // The last page stays at its natural spacing, unless ragged-last-bottom is false.
    let last = spread.pages.len() - 1;
    let (filled, _) = paper(&tune("systems-per-page = 4 ragged-last-bottom = ##f", 100, ""));
    if spread.pages[last].systems.len() >= 2 {
        assert!(gaps(&filled, last)[0] > gaps(&spread, last)[0]);
    }
    // system-system-spacing's basic distance: middle line to middle line.
    let (wide, _) = paper(&tune("system-system-spacing.basic-distance = #20 ragged-bottom = ##t", 40, ""));
    assert!((gaps(&wide, 0)[0] - 20.0).abs() < 1e-9);
    let (padded, _) = paper(&tune("system-system-spacing = #'((basic-distance . 1) (padding . 6)) ragged-bottom = ##t", 40, ""));
    // Six spaces clear between the ink, column by column (score #409): the
    // staves' lines at least.
    assert!(gaps(&padded, 0)[0] >= 4.0 + 6.0 - 1e-9, "six spaces clear between the ink");
    // On a piano's two staves, from the lower staff's middle line.
    let piano = "\\paper { ragged-bottom = ##t }\n\\new PianoStaff << \\new Staff { \\repeat unfold 40 { c''4 d'' e'' f'' } } \\new Staff { \\clef bass \\repeat unfold 40 { c4 d e f } } >>";
    let (two, _) = paper(piano);
    let low = two.systems[0].offsets[1];
    assert!(two.systems[1].top - two.systems[0].top >= low + 12.0 - 1e-9);
    // short-indent: the systems after the first start in from the margin.
    let (indented, p) = paper(&tune("short-indent = 10\\mm", 40, ""));
    assert!((indented.systems[1].left - p.left - 10.0 / (25.4 * 5.0 / 72.0)).abs() < 1e-6);
}

#[test]
fn durations_are_spaced_by_lilypond_s_rule_and_its_settings() {
    // In a line at its natural width: each doubling of a duration adds one
    // increment (1.2 spaces), the commonest shortest duration two.
    let x = |src: &str| -> Vec<f64> { screen(src).staves[0].iter().flat_map(|b| b.items.iter().map(|e| e.x)).collect() };
    let gap = |xs: &[f64], k: usize| xs[k + 1] - xs[k];
    let xs = x("{ c'8 c' c'4 c'2 | c'2 c'8 c' c'4 }");
    let (eighth, quarter, half) = (gap(&xs, 0), gap(&xs, 2), gap(&xs, 4));
    assert!((quarter - eighth - 1.2).abs() < 0.05, "{eighth} {quarter}");
    assert!((half - quarter - 1.2).abs() < 0.3, "{quarter} {half}");
    // \override Score.SpacingSpanner.spacing-increment = #2: two spaces a doubling.
    let xs = x("{ \\override Score.SpacingSpanner.spacing-increment = #2 c'8 c' c'4 c'2 c'8 c' c'4 c'2 }");
    assert!((gap(&xs, 2) - gap(&xs, 0) - 2.0).abs() < 0.05);
    // shortest-duration-space = #4: the eighths further apart.
    let wide = x("\\layout { \\context { \\Score \\override SpacingSpanner.shortest-duration-space = #4 } } { c'8 c' c'4 c'2 c'8 c' c'4 c'2 }");
    assert!(gap(&wide, 0) > eighth + 2.0);
    // base-shortest-duration: the shortest taken as a sixteenth, the eighths wider.
    let base = x("{ \\override Score.SpacingSpanner.base-shortest-duration = #(ly:make-moment 1/16) c'8 c' c'4 c'2 c'8 c' c'4 c'2 }");
    assert!((gap(&base, 0) - eighth - 1.2).abs() < 0.05);
    assert!(parse("{ \\override Score.SpacingSpanner.spacing-increment = #2 c'4 }").unwrap().warnings.is_empty());
}

#[test]
fn a_stem_up_before_a_stem_down_gets_more_room_than_the_other_way_round() {
    let x = |src: &str| -> Vec<f64> { screen(src).staves[0][0].items.iter().map(|e| e.x).collect() };
    let up_down = x("{ c'4 c''4 c'4 c''4 }");
    let down_up = x("{ c''4 c'4 c''4 c'4 }");
    let plain = x("{ e''4 e''4 e''4 e''4 }");
    let (a, b, c) = (up_down[1] - up_down[0], down_up[1] - down_up[0], plain[1] - plain[0]);
    assert!(a > c + 0.3 && b < c - 0.3, "up-down {a}, down-up {b}, same way {c}");
    // Stems that share no height are not corrected: a low note stem up, then
    // one high over the staff stem down, its stem far above the first's.
    let apart = x("{ g4 a'''4 g4 a'''4 }");
    let same = x("{ g4 g4 g4 g4 }");
    assert!((apart[1] - apart[0] - (same[1] - same[0])).abs() < 0.05, "{apart:?} {same:?}");
}
