//! The page around the music (score #388): `\paper`'s headers and footers,
//! `systems-per-page`, the header's copyright at the foot of the first page,
//! and a PDF whose words are text a reader selects.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing, clippy::panic, clippy::arithmetic_side_effects)]

use std::collections::HashMap;

use eui_score::parse::{parse, PageCond, PageLine, PagePart};
use eui_score::{engrave_paper, pdf, pdf_outlines, warnings};

/// A tune of `bars` bars of quarters, under `paper`.
fn tune(paper: &str, header: &str, bars: usize) -> String {
    let music = vec!["c'4 d' e' f'"; bars].join(" | ");
    format!("\\header {{ {header} }}\n\\paper {{ {paper} }}\n{{ {music} }}")
}

/// The words of each page of `doc`, as a reader copies them: each `Tj`'s
/// characters read back through the ToUnicode map.
fn page_words(doc: &str) -> Vec<Vec<String>> {
    let mut map: HashMap<String, char> = HashMap::new();
    if let Some(start) = doc.find("beginbfchar") {
        for line in doc[start..].lines() {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() == 2 && parts[0].starts_with('<') && parts[1].starts_with('<') {
                let cid = parts[0].trim_matches(|c| c == '<' || c == '>').to_string();
                let units: Vec<u16> = parts[1].trim_matches(|c| c == '<' || c == '>').as_bytes().chunks(4).map(|h| u16::from_str_radix(std::str::from_utf8(h).unwrap(), 16).unwrap()).collect();
                map.insert(cid, String::from_utf16(&units).unwrap().chars().next().unwrap());
            }
        }
    }
    let mut pages = Vec::new();
    for stream in doc.split("stream\n").skip(1) {
        let body = stream.split("endstream").next().unwrap_or("");
        if !body.starts_with("0 g") {
            continue;
        }
        let mut words = Vec::new();
        for line in body.lines().filter(|l| l.ends_with("Tj")) {
            let hex = line.rsplit('<').next().unwrap().trim_end_matches("> Tj");
            let text: String = hex.as_bytes().chunks(4).map(|h| map[std::str::from_utf8(h).unwrap()]).collect();
            words.push(text);
        }
        pages.push(words);
    }
    pages
}

#[test]
fn systems_per_page_sets_at_most_so_many_systems_on_a_page() {
    let free = tune("", "", 60);
    let two = tune("systems-per-page = 2", "", 60);
    let parsed = parse(&two).unwrap();
    assert_eq!(parsed.paper.systems_per_page, Some(2));
    assert!(parsed.warnings.is_empty(), "read, not passed over: {:?}", parsed.warnings);
    let (eng_free, _) = engrave_paper(&free).unwrap();
    let (eng_two, _) = engrave_paper(&two).unwrap();
    assert!(eng_free.pages[0].systems.len() > 2, "the free page holds more");
    assert!(eng_two.pages.iter().all(|p| p.systems.len() <= 2));
    assert!(eng_two.pages.iter().take(eng_two.pages.len() - 1).all(|p| p.systems.len() == 2), "each page but the last is full");
    assert_eq!(eng_two.pages.len(), eng_two.systems.len().div_ceil(2));
    // Spread evenly, too; and LilyPond's max-systems-per-page caps them as well.
    let (even, _) = engrave_paper(&tune("systems-per-page = 3 page-breaking = #ly:optimal-breaking", "", 60)).unwrap();
    assert!(even.pages.iter().all(|p| p.systems.len() <= 3));
    assert_eq!(parse(&tune("max-systems-per-page = 4", "", 4)).unwrap().paper.max_systems_per_page, Some(4));
    // Nothing for zero.
    assert_eq!(parse(&tune("systems-per-page = 0", "", 4)).unwrap().paper.systems_per_page, None);
}

#[test]
fn headers_and_footers_are_read_as_lilypond_writes_them() {
    let src = tune(
        r#"oddHeaderMarkup = \markup \fill-line { \fromproperty #'header:title "" \on-the-fly #print-page-number-check-first \fromproperty #'page:page-number-string }
           evenHeaderMarkup = \markup \fill-line { \fromproperty #'page:page-number-string \fromproperty #'header:composer }
           oddFooterMarkup = \markup { \fill-line { \concat { "Page " \fromproperty #'page:page-number-string } } }
           evenFooterMarkup = \markup \line { \bold "Draft" copy }"#,
        r#"title = "Sonata" composer = "A. Composer""#,
        4,
    );
    let parsed = parse(&src).unwrap();
    assert!(parsed.warnings.is_empty(), "{:?}", parsed.warnings);
    let p = parsed.paper;
    assert_eq!(
        p.odd_header,
        Some(PageLine { left: vec![PagePart::Field("title".into())], centre: vec![PagePart::Text(String::new())], right: vec![PagePart::Only(PageCond::NotFirst, vec![PagePart::Page])] })
    );
    assert_eq!(p.even_header, Some(PageLine { left: vec![PagePart::Page], centre: vec![], right: vec![PagePart::Field("composer".into())] }));
    assert_eq!(p.odd_footer, Some(PageLine { centre: vec![PagePart::Text("Page ".into()), PagePart::Page], ..PageLine::default() }));
    // No \fill-line: at the left; \line puts a space between its words.
    assert_eq!(p.even_footer, Some(PageLine { left: vec![PagePart::Text("Draft".into()), PagePart::Text(" ".into()), PagePart::Text("copy".into())], ..PageLine::default() }));
    // A command that is not words is passed over, said, and the rest still read.
    let odd = tune(r#"oddFooterMarkup = \markup \fill-line { "Here" \epsfile #X #10 #"a.eps" }"#, "", 2);
    let parsed = parse(&odd).unwrap();
    assert_eq!(parsed.paper.odd_footer.unwrap().left, vec![PagePart::Text("Here".into())]);
    assert_eq!(warnings(&odd).unwrap().len(), 1);
    // An unclosed \fill-line is refused with its place.
    assert!(parse(&tune(r#"oddFooterMarkup = \markup \fill-line { "a" "#, "", 2)).is_err());
}

#[test]
fn the_pdf_draws_the_headers_footers_and_copyright_and_its_words_are_text() {
    let src = tune(
        r#"systems-per-page = 2
           oddHeaderMarkup = \markup \fill-line { \fromproperty #'header:title \fromproperty #'page:page-number-string }
           evenHeaderMarkup = \markup \fill-line { \fromproperty #'page:page-number-string "Even page" }
           oddFooterMarkup = \markup \fill-line { \concat { "Page " \fromproperty #'page:page-number-string } }"#,
        r#"title = "Sonata" composer = "A. Composer" piece = "Allegro" opus = "Op. 3" copyright = "© 2026 A. Composer""#,
        60,
    );
    let doc = pdf(&src, "").unwrap();
    assert!(doc.is_ascii());
    assert!(doc.contains("/ToUnicode") && doc.contains("3 Tr"), "invisible text with its characters' map");
    let pages = page_words(&doc);
    assert!(pages.len() >= 3);
    let first = &pages[0];
    for want in ["Sonata", "A. Composer", "Allegro", "Op. 3", "© 2026 A. Composer", "Page 1", "1"] {
        assert!(first.iter().any(|w| w == want), "page 1 says {want:?}: {first:?}");
    }
    let second = &pages[1];
    for want in ["2", "Even page", "Page 2"] {
        assert!(second.iter().any(|w| w == want), "page 2 says {want:?}: {second:?}");
    }
    // The copyright is the first page's alone; the default footer is gone.
    assert!(!second.iter().any(|w| w.contains('©')));
    assert!(!first.iter().any(|w| w.contains(" / ")), "no 'n / N' footer under a footer of the source's");
    // The third page is odd: the odd header again.
    assert!(pages[2].iter().any(|w| w == "Sonata") && pages[2].iter().any(|w| w == "3"));
}

#[test]
fn a_header_makes_room_and_nothing_said_draws_as_before() {
    let plain = tune("", r#"title = "Sonata""#, 8);
    let headed = tune(r#"oddHeaderMarkup = \markup "Proof""#, r#"title = "Sonata""#, 8);
    let (a, _) = engrave_paper(&plain).unwrap();
    let (b, _) = engrave_paper(&headed).unwrap();
    assert!(b.systems[0].top > a.systems[0].top + 2.9, "the music moves down under the header");
    // The default footer, title and "1 / 1", in text as in outline.
    let words = page_words(&pdf(&plain, "").unwrap());
    assert!(words[0].iter().any(|w| w == "1 / 1") && words[0].iter().any(|w| w == "Sonata"));
    // Without the words, the drawing is the same as the PDF's with them.
    let outlines = pdf_outlines(&plain, "").unwrap();
    assert!(!outlines.contains("Tj") && !outlines.contains("/Font"));
    let with = pdf(&plain, "").unwrap();
    let stream = |doc: &str| doc.split("stream\n").nth(1).unwrap().split("\nBT").next().unwrap().split("\nend").next().unwrap().to_string();
    assert_eq!(stream(&with), stream(&outlines));
    // No title: the footer's number is the only text, in the regular face alone.
    let bare = pdf("{ c'1 }", "").unwrap();
    assert_eq!(page_words(&bare), vec![vec!["1 / 1".to_string()]]);
    assert!(bare.contains("/F1 ") && !bare.contains("/F2 ") && !bare.contains("TeXGyreSchola-Bold"));
}
