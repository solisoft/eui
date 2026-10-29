//! Each word's paths, by the note it is written on (score #434): what an
//! editor hits to open a chord diagram, an expression or a syllable where
//! it is drawn.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing, clippy::panic, clippy::arithmetic_side_effects)]

use eui_score::{engrave, paths_heads, paths_words, Layout};

#[test]
fn every_word_is_drawn_in_a_range_of_its_note() {
    let src = r#"{ c''4^\markup \fret-diagram "6-x;5-3;4-2;3-o;2-1;1-o;" d''4_\markup \italic "dolce" e''4^"solo"^"Am7" f''4 }
\addlyrics { la li lo lu }"#;
    let eng = engrave(src, 80.0, &Layout::default()).unwrap();
    let (paths, ranges, heads, words) = paths_words(&eng, 7.0, 0).unwrap();
    let texts: Vec<(usize, usize)> = words.iter().filter(|w| w.1 == "text").map(|w| (w.0, w.2)).collect();
    assert_eq!(texts, [(0, 0), (1, 0), (2, 0), (2, 1)], "the diagram, dolce, and the two words on the third note");
    let lyrics: Vec<(usize, usize)> = words.iter().filter(|w| w.1 == "lyric").map(|w| (w.0, w.2)).collect();
    assert_eq!(lyrics, [(0, 0), (1, 0), (2, 0), (3, 0)], "one syllable a note, verse 1");
    assert!(words.iter().all(|w| !w.3.is_empty() && w.3.end <= paths.len()), "each drawn, among the page's paths");
    // The words are none of the notes' paths.
    for w in &words {
        assert!(ranges.iter().all(|r| r.1.end <= w.3.start || r.1.start >= w.3.end), "{w:?} is apart from the notes");
    }
    // The same page as paths_heads draws it.
    let (same, same_ranges, same_heads) = paths_heads(&eng, 7.0, 0).unwrap();
    assert_eq!(same.len(), paths.len());
    assert_eq!((same_ranges, same_heads), (ranges, heads));
}

#[test]
fn a_page_with_no_words_has_none() {
    let eng = engrave("{ c''4 d'' e'' f'' }", 80.0, &Layout::default()).unwrap();
    assert!(paths_words(&eng, 7.0, 0).unwrap().3.is_empty());
}
