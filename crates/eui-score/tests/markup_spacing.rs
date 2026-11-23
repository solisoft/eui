//! `\paper`'s spacing around what is not music (score #1025):
//! `top-markup-spacing`, from the top of the page to the titles;
//! `score-markup-spacing`, from the last system to the words written after
//! the music; `markup-markup-spacing`, from one block of them to the next.
//! Each as LilyPond measures it: a system from the middle line of its
//! nearest staff, a title or a block of words from its top.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing, clippy::panic, clippy::arithmetic_side_effects)]

use eui_score::parse::parse;
use eui_score::text::em;
use eui_score::{engrave, engrave_paper, warnings, Engraved, Layout};

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-6
}

fn titled(paper: &str) -> String {
    format!("\\paper {{ {paper} }} \\header {{ title = \"Air\" composer = \"Anon.\" }} {{ c'1 | d'1 }}")
}

fn worded(paper: &str) -> String {
    format!("\\paper {{ {paper} }} {{ c'1 | d'1 }} \\markup {{ \\column {{ \"one\" \"two\" }} }} \\markup {{ \"three\" }}")
}

fn paged(src: &str) -> Engraved {
    engrave_paper(src).unwrap_or_else(|e| panic!("{src}: {e}")).0
}

/// A line of words' top and bottom, from its baseline: the title block's
/// own measure of a line.
fn ink() -> (f64, f64) {
    (0.72 * em(0.0), 0.25 * em(0.0))
}

#[test]
fn the_three_spacings_are_read_and_no_longer_passed_over() {
    let src = titled(
        "top-markup-spacing = #'((basic-distance . 4) (minimum-distance . 2) (padding . 3)) \
         markup-markup-spacing.basic-distance = #6 markup-markup-spacing.padding = #2 \
         score-markup-spacing = #'((basic-distance . 15) (padding . 1.5) (stretchability . 60))",
    );
    let p = parse(&src).unwrap();
    assert!(p.warnings.is_empty(), "{:?}", p.warnings);
    let (top, between, under) = (p.paper.top_markup_spacing, p.paper.markup_markup_spacing, p.paper.score_markup_spacing);
    assert_eq!((top.basic, top.minimum, top.padding), (Some(4.0), Some(2.0), Some(3.0)));
    assert_eq!((between.basic, between.minimum, between.padding), (Some(6.0), None, Some(2.0)));
    assert_eq!((under.basic, under.padding, under.stretch), (Some(15.0), Some(1.5), Some(60.0)));
    // A spacing this does not lay out is still said.
    assert_eq!(warnings(&titled("nonstaff-nonstaff-spacing.padding = #2")).unwrap().len(), 1);
}

#[test]
fn the_titles_stand_as_far_from_the_top_as_top_markup_spacing_says() {
    let plain = paged(&titled(""));
    let title = |e: &Engraved| e.header.lines[0].y;
    // LilyPond's own values change nothing.
    let same = paged(&titled("top-markup-spacing = #'((basic-distance . 0) (minimum-distance . 0) (padding . 1))"));
    assert!(close(title(&same), title(&plain)) && close(same.systems[0].top, plain.systems[0].top));
    // The title's top five spaces under the margin where it was one: four
    // lower, the other titles and the music with it.
    let lower = paged(&titled("top-markup-spacing.basic-distance = #5"));
    assert!(close(title(&lower), title(&plain) + 4.0), "{} {}", title(&lower), title(&plain));
    assert!(close(lower.header.lines[1].y, plain.header.lines[1].y + 4.0));
    assert!(close(lower.systems[0].top, plain.systems[0].top + 4.0));
    assert!(close(lower.header.bottom, plain.header.bottom + 4.0));
    // The least distance where it is the greater; the padding likewise.
    assert!(close(title(&paged(&titled("top-markup-spacing.minimum-distance = #3"))), title(&plain) + 2.0));
    assert!(close(title(&paged(&titled("top-markup-spacing.padding = #2.5"))), title(&plain) + 1.5));
    // No padding: the title a space higher, and no further.
    assert!(close(title(&paged(&titled("top-markup-spacing.padding = #0"))), title(&plain) - 1.0));
    // On a screen too.
    let screen = |paper: &str| engrave(&titled(paper), 100.0, &Layout::default()).unwrap();
    assert!(close(title(&screen("top-markup-spacing.basic-distance = #5")), title(&screen("")) + 4.0));
    // A page with no titles has none to move.
    let bare = |paper: &str| paged(&format!("\\paper {{ {paper} }} {{ c'1 }}")).systems[0].top;
    assert!(close(bare("top-markup-spacing.basic-distance = #5"), bare("")));
}

#[test]
fn the_words_stand_under_the_music_as_score_markup_spacing_says() {
    let (ascent, _) = ink();
    let first = |e: &Engraved| e.after[0].y;
    // The middle line of the last system's staff.
    let middle = |e: &Engraved| e.systems.last().unwrap().top + 2.0;
    // LilyPond's own: the words' top twelve spaces under the middle line.
    let plain = paged(&worded(""));
    assert!(close(first(&plain), middle(&plain) + 12.0 + ascent), "{} {}", first(&plain), middle(&plain));
    let same = paged(&worded("score-markup-spacing = #'((basic-distance . 12) (padding . 0.5) (stretchability . 60))"));
    assert!(close(first(&same), first(&plain)));
    // Another basic distance; a least distance over it.
    let near = paged(&worded("score-markup-spacing.basic-distance = #7"));
    assert!(close(first(&near), middle(&near) + 7.0 + ascent));
    let least = paged(&worded("score-markup-spacing.basic-distance = #7 score-markup-spacing.minimum-distance = #9"));
    assert!(close(first(&least), middle(&least) + 9.0 + ascent));
    // Never nearer the music's ink than the padding.
    let tight = paged(&worded("score-markup-spacing.basic-distance = #0 score-markup-spacing.padding = #1.5"));
    assert!(close(first(&tight), tight.systems.last().unwrap().bottom + 1.5 + ascent), "{} {}", first(&tight), tight.systems.last().unwrap().bottom);
    // The music is where it was.
    assert!(close(near.systems[0].top, plain.systems[0].top));
    // On a screen too, the score as tall as its words.
    let screen = |paper: &str| engrave(&worded(paper), 100.0, &Layout::default()).unwrap();
    let (far, usual) = (screen("score-markup-spacing.basic-distance = #20"), screen(""));
    assert!(close(first(&far), first(&usual) + 8.0));
    assert!(close(far.height, usual.height + 8.0));
}

#[test]
fn a_block_of_words_follows_the_one_before_as_markup_markup_spacing_says() {
    let (ascent, descent) = ink();
    // Two blocks: a column of two lines, then a line.
    assert_eq!(parse(&worded("")).unwrap().after_blocks, vec![2]);
    let ys = |paper: &str| -> Vec<f64> { paged(&worded(paper)).after.iter().map(|w| w.y).collect() };
    let plain = ys("");
    // A block's own lines, three spaces from baseline to baseline.
    assert!(close(plain[1], plain[0] + 3.0));
    // LilyPond's own: the next block's top half a space under the one
    // before's ink — their tops are more than a space apart anyway.
    assert!(close(plain[2], plain[1] + descent + 0.5 + ascent), "{plain:?}");
    // The padding.
    assert!(close(ys("markup-markup-spacing.padding = #4")[2], plain[1] + descent + 4.0 + ascent));
    // The basic distance, from top to top: the first block's top is its first line's.
    let far = ys("markup-markup-spacing.basic-distance = #10");
    assert!(close(far[2], far[0] + 10.0), "{far:?}");
    // The least distance where it is the greater.
    let least = ys("markup-markup-spacing.basic-distance = #4 markup-markup-spacing.minimum-distance = #9");
    assert!(close(least[2], least[0] + 9.0));
    // The first block is where it was.
    assert!(close(far[0], plain[0]) && close(far[1], plain[1]));
}

#[test]
fn words_a_page_has_no_room_for_come_nearer_the_music_before_they_leave_it() {
    // A page nearly full: the words fit under the music, not twelve spaces from it.
    let (_, descent) = ink();
    let src = |bars: usize| format!("\\paper {{ ragged-bottom = ##t }} {{ {} }} \\markup {{ \"Fin\" }}", vec!["c'1 \\break"; bars].join(" "));
    let mut squeezed = 0;
    for bars in 8..40 {
        let e = paged(&src(bars));
        let (paper, last) = (engrave_paper(&src(bars)).unwrap().1, e.systems.last().unwrap().clone());
        let floor = paper.h - paper.bottom;
        let words = &e.after[0];
        let natural = last.top + 2.0 + 12.0;
        if words.page == last.page && words.y - 0.72 * em(0.0) < natural - 1e-6 {
            squeezed += 1;
            // As far from the music as the page lets them be, and clear of its ink.
            assert!(close(words.y + descent, floor), "{bars}: {} {}", words.y, floor);
            assert!(words.y - 0.72 * em(0.0) >= last.bottom + 0.5 - 1e-6);
        }
    }
    assert!(squeezed > 0, "no page was nearly full");
}
