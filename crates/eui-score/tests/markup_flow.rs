//! Words between the scores (score #1025): a top-level `\markup` is an item
//! of the page, as a system is — before the first score, between two, after
//! the last — spaced by `markup-system-spacing`, `score-markup-spacing`,
//! `markup-markup-spacing` and `top-markup-spacing`, flowing from a page to
//! the next line by line, and a page of words alone is a title page.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing, clippy::panic, clippy::arithmetic_side_effects)]

use eui_score::text::em;
use eui_score::{engrave, engrave_paper, warnings, Engraved, Layout};

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-6
}

fn paged(src: &str) -> Engraved {
    engrave_paper(src).unwrap_or_else(|e| panic!("{src}: {e}")).0
}

/// A line of words' top, over its baseline.
fn ascent() -> f64 {
    0.72 * em(0.0)
}

fn word<'a>(e: &'a Engraved, text: &str) -> &'a eui_score::engrave::AfterWords {
    e.after.iter().find(|w| w.text == text).unwrap_or_else(|| panic!("no {text:?} among {:?}", e.after.iter().map(|w| &w.text).collect::<Vec<_>>()))
}

#[test]
fn words_before_the_first_score_stand_over_it() {
    let src = |paper: &str| format!("\\paper {{ ragged-bottom = ##t {paper} }} \\markup {{ \"Before\" }} {{ c'1 }}");
    assert!(warnings(&src("")).unwrap().is_empty(), "{:?}", warnings(&src("")));
    let plain = paged(&src(""));
    let w = word(&plain, "Before");
    assert_eq!(w.page, 0);
    // `markup-system-spacing`: the staff's middle line five spaces under the words' top.
    let top = |e: &Engraved| word(e, "Before").y - ascent();
    assert!(close(plain.systems[0].top + 2.0, top(&plain) + 5.0), "{} {}", plain.systems[0].top, top(&plain));
    let far = paged(&src("markup-system-spacing.basic-distance = #9"));
    assert!(close(far.systems[0].top + 2.0, top(&far) + 9.0));
    // The words where they were: `top-markup-spacing` puts them.
    assert!(close(top(&far), top(&plain)));
    let lower = paged(&src("top-markup-spacing.basic-distance = #5"));
    assert!(close(top(&lower), top(&plain) + 4.0));
    // On a screen too.
    let screen = engrave(&src(""), 100.0, &Layout::default()).unwrap();
    assert!(word(&screen, "Before").y < screen.systems[0].top);
}

#[test]
fn words_between_two_scores_stand_between_them() {
    let src = |paper: &str| format!("\\paper {{ ragged-bottom = ##t {paper} }} \\score {{ {{ c'1 }} }} \\markup {{ \"Between\" }} \\score {{ {{ d'1 }} }}");
    assert!(warnings(&src("")).unwrap().is_empty(), "{:?}", warnings(&src("")));
    let top = |e: &Engraved| word(e, "Between").y - ascent();
    let plain = paged(&src(""));
    assert_eq!(plain.systems.len(), 2);
    // `score-markup-spacing` under the first score, `markup-system-spacing` over the second.
    assert!(close(top(&plain), plain.systems[0].top + 2.0 + 12.0), "{} {}", top(&plain), plain.systems[0].top);
    assert!(close(plain.systems[1].top + 2.0, top(&plain) + 5.0));
    let set = paged(&src("score-markup-spacing.basic-distance = #7 markup-system-spacing.basic-distance = #10"));
    assert!(close(top(&set), set.systems[0].top + 2.0 + 7.0));
    assert!(close(set.systems[1].top + 2.0, top(&set) + 10.0));
    // Two blocks between them: `markup-markup-spacing` from one to the next.
    let two = paged("\\paper { ragged-bottom = ##t markup-markup-spacing.basic-distance = #6 } { c'1 } \\markup { \"one\" } \\markup { \"two\" } { d'1 }");
    assert!(close(word(&two, "two").y, word(&two, "one").y + 6.0));
    assert!(two.systems[1].top > word(&two, "two").y);
}

#[test]
fn words_between_two_scores_flow_from_a_page_to_the_next() {
    let lines: Vec<String> = (0..150).map(|k| format!("\"line {k}\"")).collect();
    let src = format!("{{ c'1 }} \\markup {{ \\column {{ {} }} }} {{ d'1 }}", lines.join(" "));
    let (e, paper) = engrave_paper(&src).unwrap();
    assert!(e.pages.len() >= 2, "{}", e.pages.len());
    assert_eq!(e.after.len(), 150);
    // Each line on its page, in order, above its foot.
    let floor = paper.h - paper.bottom;
    for pair in e.after.windows(2) {
        assert!(pair[1].page > pair[0].page || (pair[1].page == pair[0].page && pair[1].y > pair[0].y));
    }
    assert!(e.after.iter().all(|w| w.y < floor));
    assert_eq!(word(&e, "line 0").page, 0);
    // The second score after the last line.
    let last = e.after.last().unwrap();
    let second = &e.systems[1];
    assert!(second.page > last.page || (second.page == last.page && second.top > last.y));
    assert_eq!(e.systems[0].page, 0);
}

#[test]
fn a_page_of_words_before_a_page_break_is_a_title_page() {
    for src in [
        "\\book { \\markup { \"Sonata\" } \\pageBreak \\score { { c'1 } } }",
        "\\bookpart { \\markup { \"Sonata\" } } \\bookpart { \\score { { c'1 } } }",
        "\\markup { \"Sonata\" } \\pageBreak { c'1 }",
    ] {
        assert!(warnings(src).unwrap().is_empty(), "{src}: {:?}", warnings(src));
        let e = paged(src);
        assert_eq!(e.pages.len(), 2, "{src}");
        assert_eq!(word(&e, "Sonata").page, 0, "{src}");
        assert!(e.pages[0].systems.is_empty(), "{src}");
        assert_eq!(e.systems[0].page, 1, "{src}");
    }
    // Without the break, the words and the music share the page.
    assert_eq!(paged("\\markup { \"Sonata\" } { c'1 }").pages.len(), 1);
}

#[test]
fn a_page_break_goes_where_it_is_written_among_words_and_scores() {
    // Before the words: they start the second page, the second score under them.
    let before = paged("{ c'1 } \\pageBreak \\markup { \"Words\" } { d'1 }");
    assert_eq!((before.pages.len(), word(&before, "Words").page, before.systems[1].page), (2, 1, 1));
    // After them: they end the first page.
    let after = paged("{ c'1 } \\markup { \"Words\" } \\pageBreak { d'1 }");
    assert_eq!((after.pages.len(), word(&after, "Words").page, after.systems[1].page), (2, 0, 1));
    // After the last score.
    let last = paged("{ c'1 } \\pageBreak \\markup { \"Words\" }");
    assert_eq!((last.pages.len(), word(&last, "Words").page), (2, 1));
    // Between two blocks.
    let split = paged("{ c'1 } \\markup { \"one\" } \\pageBreak \\markup { \"two\" }");
    assert_eq!((split.pages.len(), word(&split, "one").page, word(&split, "two").page), (2, 0, 1));
}

#[test]
fn two_columns_stand_side_by_side() {
    let e = paged("{ c'1 } \\markup { \\column { \"a\" \"b\" } \\hspace #5 \\column { \"c\" \"d\" } }");
    let (a, b, c, d) = (word(&e, "a"), word(&e, "b"), word(&e, "c"), word(&e, "d"));
    assert!(close(a.y, c.y) && close(b.y, d.y) && close(b.y, a.y + 3.0), "{:?}", e.after);
    assert!(close(a.x, b.x) && close(c.x, d.x));
    assert!(c.x > a.x + 5.0, "{} {}", c.x, a.x);
}

#[test]
fn words_are_drawn_in_their_faces_and_sizes() {
    use eui_score::glyphs::Weight;
    use eui_score::text::text_width;
    assert!(warnings("{ c'1 } \\markup { \\bold B \\italic I \\fontsize #2 big \\small little \\huge \\bold tall plain }").unwrap().is_empty());
    let e = paged("{ c'1 } \\markup { \\bold B \\italic I \\fontsize #2 big \\small little \\huge \\bold tall plain }");
    let faces: Vec<(&str, Weight)> = e.after.iter().map(|w| (w.text.as_str(), w.weight)).collect();
    assert_eq!(faces, [("B", Weight::Bold), ("I", Weight::Italic), ("big", Weight::Regular), ("little", Weight::Regular), ("tall", Weight::Bold), ("plain", Weight::Regular)]);
    let sizes: Vec<f64> = e.after.iter().map(|w| w.em).collect();
    assert!(close(sizes[0], em(0.0)) && close(sizes[2], em(2.0)) && close(sizes[3], em(-1.0)) && close(sizes[4], em(2.0)) && close(sizes[5], em(0.0)), "{sizes:?}");
    // One line: one baseline, each word after the one before.
    assert!(e.after.windows(2).all(|p| close(p[0].y, p[1].y) && p[1].x > p[0].x + text_width(&p[0].text, p[0].weight, p[0].em)));
    // Words of one face written with a space between them are one piece.
    let one = paged("{ c'1 } \\markup { one two \\bold { three four } }");
    assert_eq!(one.after.iter().map(|w| w.text.as_str()).collect::<Vec<_>>(), ["one two", "three four"]);
    // `\concat` joins, `\super` raises and `\sub` lowers, smaller.
    let c = paged("{ c'1 } \\markup \\concat { H \\sub 2 O \\super x }");
    let (h, two, o, x) = (word(&c, "H"), word(&c, "2"), word(&c, "O"), word(&c, "x"));
    assert!(close(two.x, h.x + text_width("H", h.weight, h.em)) && close(o.x, two.x + text_width("2", two.weight, two.em)));
    assert!(two.y > h.y && x.y < h.y && close(o.y, h.y) && two.em < h.em && close(x.em, two.em));
    // `\raise` and `\lower`, in staff spaces.
    let r = paged("{ c'1 } \\markup { a \\raise #2 b \\lower #1.5 c }");
    assert!(close(word(&r, "b").y, word(&r, "a").y - 2.0) && close(word(&r, "c").y, word(&r, "a").y + 1.5));
    // `\fontsize` spaces a column's lines with its words; `\vspace` adds room.
    let col = paged("{ c'1 } \\markup \\fontsize #6 \\column { a b }");
    assert!(close(word(&col, "b").y - word(&col, "a").y, 6.0));
    let room = paged("{ c'1 } \\markup \\column { a \\vspace #2 b }");
    assert!(word(&room, "b").y - word(&room, "a").y > 9.0);
}

#[test]
fn a_fill_line_spreads_its_words_over_the_line() {
    use eui_score::text::text_width;
    let wide = |w: &eui_score::engrave::AfterWords| text_width(&w.text, w.weight, w.em);
    let e = paged("\\markup \\fill-line { \\fontsize #6 \\bold \"Sonata\" } \\markup \\fill-line { left middle right } \\markup \\fill-line { one two } { c'1 }");
    let (left, right) = (e.left, e.left + e.width);
    let title = word(&e, "Sonata");
    assert!(close(title.x + wide(title) / 2.0, (left + right) / 2.0), "one cell is centred");
    assert!(close(word(&e, "left").x, left) && close(word(&e, "right").x + wide(word(&e, "right")), right));
    assert!(close(word(&e, "middle").x + wide(word(&e, "middle")) / 2.0, (left + right) / 2.0));
    assert!(close(word(&e, "one").x, left) && close(word(&e, "two").x + wide(word(&e, "two")), right));
    // `\null` is a cell: the words after it at the right, between two in the middle.
    let n = paged("\\markup \\fill-line { \\null flush } \\markup \\fill-line { \\null mid \\null } { c'1 }");
    assert!(close(word(&n, "flush").x + wide(word(&n, "flush")), n.left + n.width));
    assert!(close(word(&n, "mid").x + wide(word(&n, "mid")) / 2.0, n.left + n.width / 2.0));
    // A centred column, each line centred on the same line.
    let c = paged("\\markup \\fill-line { \\center-column { \"a long line\" short } } { c'1 }");
    let (long, short) = (word(&c, "a long line"), word(&c, "short"));
    assert!(close(long.x + wide(long) / 2.0, short.x + wide(short) / 2.0) && close(short.y, long.y + 3.0));
    assert!(close(long.x + wide(long) / 2.0, (c.left + c.left + c.width) / 2.0));
    // `\wordwrap` breaks its words where the line ends.
    let words = vec!["word"; 80].join(" ");
    let w = paged(&format!("{{ c'1 }} \\markup \\wordwrap {{ {words} }}"));
    assert!(w.after.len() >= 2 && w.after.iter().all(|p| p.x + wide(p) <= w.left + w.width + 1e-6));
    assert_eq!(w.after.iter().map(|p| p.text.split(' ').count()).sum::<usize>(), 80);
    // A header's field.
    let f = paged("\\header { composer = \"Anon.\" } \\markup { by \\fromproperty #'header:composer } { c'1 }");
    assert!(f.after.iter().any(|p| p.text == "by Anon."), "{:?}", f.after);
}

#[test]
fn the_titles_stand_on_the_page_of_the_book_part_they_title() {
    let src = "\\bookpart { \\markup { \"Cover\" } } \\bookpart { \\header { title = \"Sonata\" } \\markup { \"Preface\" } { c'1 } }";
    let e = paged(src);
    assert_eq!((e.pages.len(), e.head_page, word(&e, "Cover").page, word(&e, "Preface").page, e.systems[0].page), (2, 1, 0, 1, 1));
    assert!(e.pages[1].part_first && e.pages[1].part == 1 && e.pages[0].part == 0);
    // The words under the titles, the music under the words.
    let preface = word(&e, "Preface");
    assert!(preface.y > e.header.lines[0].y && e.systems[0].top > preface.y);
    // The titles are drawn there, and nowhere else.
    let bare = paged("\\bookpart { \\markup { \"Cover\" } } \\bookpart { \\markup { \"Preface\" } { c'1 } }");
    let count = |e: &Engraved, page: usize| eui_score::paths(e, 5.0, page).unwrap().0.len();
    assert_eq!(count(&e, 0), count(&bare, 0));
    assert!(count(&e, 1) > count(&bare, 1));
    // A header written with the cover titles the cover.
    let cover = paged("\\bookpart { \\header { title = \"Sonata\" } \\markup { \"Cover\" } } \\bookpart { { c'1 } }");
    assert_eq!((cover.head_page, word(&cover, "Cover").page), (0, 0));
    assert!(cover.pages[1].part_first && cover.pages[1].part == 1, "a part that starts with its score");
    assert!(word(&cover, "Cover").y > cover.header.lines[0].y);
    // On a screen, one page: the titles first.
    let screen = engrave(src, 100.0, &Layout::default()).unwrap();
    assert_eq!((screen.pages.len(), screen.head_page), (1, 0));
    assert!(word(&screen, "Cover").y > screen.header.lines[0].y && screen.systems[0].top > word(&screen, "Preface").y);
}

#[test]
fn a_title_page_is_not_spread_down_its_page() {
    // A book part's last page is ragged, as the last of all: its words stay at the top.
    let e = paged("\\bookpart { \\markup { \"Cover\" } \\markup { \"Subtitle\" } } \\bookpart { { c'1 } }");
    let plain = paged("\\paper { ragged-bottom = ##t } \\bookpart { \\markup { \"Cover\" } \\markup { \"Subtitle\" } } \\bookpart { { c'1 } }");
    assert!(close(word(&e, "Subtitle").y, word(&plain, "Subtitle").y));
    // The words of a block keep their distances wherever a page is spread.
    let lines: Vec<String> = (0..20).map(|k| format!("\"line {k}\"")).collect();
    let src = format!("{{ c'1 \\break d'1 \\break e'1 }} \\markup {{ \\column {{ {} }} }} \\pageBreak {{ f'1 }}", lines.join(" "));
    let spread = paged(&src);
    assert!(spread.after.windows(2).all(|p| close(p[1].y, p[0].y + 3.0)));
    assert!(spread.systems[1].top - spread.systems[0].top > 14.0, "the page is spread");
}

#[test]
fn words_between_systems_are_drawn_in_the_bands_they_fall_in() {
    use eui_score::bands;
    let with = engrave("\\markup { \"Before\" } { c'1 } \\markup { \"Between\" } { d'1 } \\markup { \"After\" }", 100.0, &Layout::default()).unwrap();
    let without = engrave("\\score { { c'1 } } \\score { { d'1 } }", 100.0, &Layout::default()).unwrap();
    assert!(with.height > without.height + 6.0);
    let total = |e: &Engraved| (e.height * 8.0).ceil() as i64;
    let (cut, bare) = (bands(&with, 8.0, 0, total(&with)).unwrap(), bands(&without, 8.0, 0, total(&without)).unwrap());
    assert_eq!(cut.len(), bare.len());
    assert_eq!(cut.iter().map(|b| b.h).sum::<i64>(), total(&with));
    // The titles' band takes the words before the music; each system's, those under it.
    let more: Vec<usize> = cut.iter().zip(&bare).map(|(a, b)| a.paths.len() - b.paths.len()).collect();
    assert!(more.iter().all(|n| *n > 0), "{more:?}");
    // Each word inside the band it is drawn in.
    for w in &with.after {
        let (top, bottom) = ((w.y - 0.72 * w.em) * 8.0, (w.y + 0.25 * w.em) * 8.0);
        assert!(cut.iter().any(|b| b.y as f64 <= top && bottom <= (b.y + b.h) as f64), "{w:?}");
    }
}

#[test]
fn words_count_in_what_a_source_writes_and_a_wrong_one_is_refused() {
    assert!(engrave_paper("\\markup { \\column { a b } { c'1 }").is_err(), "never closed");
    assert!(engrave_paper("\\markup").is_err());
    assert!(engrave_paper("\\markup { \"only words\" }").unwrap_err().message.contains("there is no music"));
    // What a page's words do not draw is said, the words around it kept.
    let said: Vec<String> = warnings("{ c'1 } \\markup { a \\musicglyph \"scripts.segno\" \\draw-line #'(4 . 0) b }").unwrap().into_iter().map(|w| w.message).collect();
    assert_eq!(said, ["\\musicglyph in a \\markup outside the music is not drawn", "\\draw-line is not markup this draws"]);
    assert_eq!(paged("{ c'1 } \\markup { a \\musicglyph \"scripts.segno\" \\draw-line #'(4 . 0) b }").after.iter().map(|w| w.text.as_str()).collect::<Vec<_>>(), ["a b"]);
    // A list of markups is still passed over.
    assert_eq!(warnings("{ c'1 } \\markuplist { a b }").unwrap().len(), 1);
}
