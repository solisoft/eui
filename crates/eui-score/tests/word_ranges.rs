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
    let (paths, ranges, heads, words, grids) = paths_words(&eng, 7.0, 0).unwrap();
    let texts: Vec<(usize, usize)> = words.iter().filter(|w| w.1 == "text").map(|w| (w.0, w.2)).collect();
    assert_eq!(texts, [(0, 0), (1, 0), (2, 0), (2, 1)], "the diagram, dolce, and the two words on the third note");
    let lyrics: Vec<(usize, usize)> = words.iter().filter(|w| w.1 == "lyric").map(|w| (w.0, w.2)).collect();
    assert_eq!(lyrics, [(0, 0), (1, 0), (2, 0), (3, 0)], "one syllable a note, verse 1");
    assert!(words.iter().all(|w| !w.3.is_empty() && w.3.end <= paths.len()), "each drawn, among the page's paths");
    // The words are none of the notes' paths.
    for w in &words {
        assert!(ranges.iter().all(|r| r.1.end <= w.3.start || r.1.start >= w.3.end), "{w:?} is apart from the notes");
    }
    // The one chord diagram's grid: on the first note, its first word, inside the word's box.
    assert_eq!(grids.len(), 1);
    let (item, index, g) = grids[0];
    assert_eq!((item, index), (0, 0));
    let word = words.iter().find(|w| w.1 == "text" && w.0 == 0).unwrap();
    let ink = paths[word.3.clone()].iter().map(|p| match p {
        eui_score::Path::Rect(r) => [r[0], r[1], r[0] + r[2], r[1] + r[3]],
        eui_score::Path::Fill(c) => [c[1], c[2], c[1], c[2]],
    });
    let (mut lo, mut hi) = ([f64::MAX; 2], [f64::MIN; 2]);
    for b in ink {
        lo = [lo[0].min(b[0]), lo[1].min(b[1])];
        hi = [hi[0].max(b[2]), hi[1].max(b[3])];
    }
    assert!(g[0] > lo[0] - 0.5 && g[2] < hi[0] + 0.5 && g[2] > g[0], "the strings within the diagram: {g:?} in {lo:?}..{hi:?}");
    assert!(g[1] > lo[1] && g[3] <= hi[1] + 0.5 && g[3] > g[1], "the nut under the x and o, the last fret at the bottom");
    assert_eq!((g[4], g[5]), (4.0, 1.0), "four frets from the first");
    // The same page as paths_heads draws it.
    let (same, same_ranges, same_heads) = paths_heads(&eng, 7.0, 0).unwrap();
    assert_eq!(same.len(), paths.len());
    assert_eq!((same_ranges, same_heads), (ranges, heads));
}

#[test]
fn a_page_with_no_words_has_none() {
    let eng = engrave("{ c''4 d'' e'' f'' }", 80.0, &Layout::default()).unwrap();
    let (_, _, _, words, grids) = paths_words(&eng, 7.0, 0).unwrap();
    assert!(words.is_empty() && grids.is_empty());
}

#[test]
fn a_diagram_up_the_neck_says_its_first_fret() {
    let src = r#"{ c''1^\markup \fret-diagram "6-x;5-5;4-7;3-7;2-6;1-5;" }"#;
    let eng = engrave(src, 80.0, &Layout::default()).unwrap();
    let grids = paths_words(&eng, 7.0, 0).unwrap().4;
    assert_eq!(grids.len(), 1);
    assert_eq!((grids[0].2[4], grids[0].2[5]), (4.0, 5.0), "frets 5 to 8");
}
