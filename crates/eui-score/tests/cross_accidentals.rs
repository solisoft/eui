//! The accidentals of notes drawn on another staff than their own —
//! `\change Staff`, `\autoChange` — counted in the bar of the staff they
//! are drawn on, as LilyPond's Accidental_engraver counts them (score
//! #411): an alteration that staff has shown in the bar is not shown
//! again, one it has not is; and the staff a note left no longer counts it.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing, clippy::panic, clippy::arithmetic_side_effects)]

use eui_score::parse::Kind;
use eui_score::{engrave, Engraved, Layout};

fn eng(src: &str) -> Engraved {
    engrave(src, 900.0 / 8.0, &Layout::default()).unwrap_or_else(|e| panic!("{e}"))
}

fn piano(up: &str, down: &str) -> String {
    format!("\\new PianoStaff <<\n  \\new Staff = \"up\" {{ {up} }}\n  \\new Staff = \"down\" {{ \\clef bass {down} }}\n>>")
}

/// Each note a staff draws in a bar, in time order: its start, and the
/// accidentals its heads show, low to high.
fn shown(e: &Engraved, staff: usize, bar: usize) -> Vec<(i64, Vec<Option<i64>>)> {
    let mut out: Vec<(i64, Vec<Option<i64>>)> = e.staves[staff][bar].items.iter().filter(|ev| ev.k == Kind::Note).map(|ev| (ev.start, ev.heads.iter().map(|h| h.acc).collect())).collect();
    out.sort_by_key(|n| n.0);
    out
}

const Q: i64 = 20160;

#[test]
fn a_note_moved_down_takes_the_accidental_the_lower_staff_already_showed() {
    // The left hand's fis on beat one; the right hand's fis drawn there on
    // beat two: the sharp is not repeated.
    let e = eng(&piano(r#"r4 \change Staff = "down" fis4 \change Staff = "up" r2"#, "fis4 r2."));
    assert_eq!(shown(&e, 1, 0), vec![(0, vec![Some(1)]), (Q, vec![None])]);
    // Another octave is another line: shown.
    let e = eng(&piano(r#"r4 \change Staff = "down" fis,4 \change Staff = "up" r2"#, "fis4 r2."));
    assert_eq!(shown(&e, 1, 0), vec![(0, vec![Some(1)]), (Q, vec![Some(1)])]);
}

#[test]
fn a_note_moved_down_shows_what_the_lower_staff_needs() {
    // In G major the left hand's f natural is shown; the right hand's fis
    // drawn after it on that staff needs its sharp back, though the key has it.
    let e = eng(&piano(r#"\key g \major r4 \change Staff = "down" fis4 \change Staff = "up" r2"#, r"\key g \major f4 r2."));
    assert_eq!(shown(&e, 1, 0), vec![(0, vec![Some(0)]), (Q, vec![Some(1)])]);
    // And what the lower staff plays after it is counted after it: its own
    // fis on beat three shows nothing more.
    let e = eng(&piano(r#"r4 \change Staff = "down" fis4 \change Staff = "up" r2"#, "f4 r fis r"));
    assert_eq!(shown(&e, 1, 0), vec![(0, vec![None]), (Q, vec![Some(1)]), (2 * Q, vec![None])]);
}

#[test]
fn the_staff_a_note_left_no_longer_counts_it() {
    // The first cis' goes down; the second, on the upper staff, is that
    // staff's first in the bar: its sharp is shown.
    let e = eng(&piano(r#"\change Staff = "down" cis'4 \change Staff = "up" cis'4 r2"#, "s1"));
    assert_eq!(shown(&e, 0, 0), vec![(Q, vec![Some(1)])]);
    assert_eq!(shown(&e, 1, 0), vec![(0, vec![Some(1)])]);
    // The lower staff's own cis' after the one that came to it: not repeated.
    let e = eng(&piano(r#"\change Staff = "down" cis'4 \change Staff = "up" r2."#, "r4 cis'4 r2"));
    assert_eq!(shown(&e, 1, 0), vec![(0, vec![Some(1)]), (Q, vec![None])]);
    // The next bar starts afresh.
    let e = eng(&piano(r#"\change Staff = "down" cis'4 \change Staff = "up" r2. | r1"#, "s1 | cis'4 r2."));
    assert_eq!(shown(&e, 1, 1), vec![(0, vec![Some(1)])]);
}

#[test]
fn ties_reminders_and_chords_keep_their_rules_on_the_other_staff() {
    // A note tied into the next bar shows no accidental there, on whichever
    // staff it lands.
    let e = eng(&piano(r#"r2. \change Staff = "down" cis'4~ | cis'4 \change Staff = "up" r2."#, "s1 | s1"));
    assert_eq!(shown(&e, 1, 0), vec![(3 * Q, vec![Some(1)])]);
    assert_eq!(shown(&e, 1, 1), vec![(0, vec![None])]);
    // `cis!` and `cis?` show theirs whatever the staff has shown.
    let e = eng(&piano(r#"r4 \change Staff = "down" cis'!4 cis'?4 \change Staff = "up" r4"#, "cis'4 r2."));
    let lower = &e.staves[1][0].items;
    let notes: Vec<_> = lower.iter().filter(|ev| ev.k == Kind::Note).collect();
    assert_eq!(notes.len(), 3);
    assert!(notes.iter().all(|n| n.heads[0].acc == Some(1)), "{:?}", notes.iter().map(|n| n.heads[0].acc).collect::<Vec<_>>());
    assert!(notes.iter().any(|n| n.heads[0].paren));
    // A chord moved down: each head against the lower staff's bar.
    let e = eng(&piano(r#"r4 \change Staff = "down" <fis bes>4 \change Staff = "up" r2"#, "fis4 r2."));
    assert_eq!(shown(&e, 1, 0), vec![(0, vec![Some(1)]), (Q, vec![None, Some(-1)])]);
    // Notes starting together on the lower staff are read against the bar
    // before them, and on one line altered differently both show theirs.
    let e = eng(&piano(r#"\change Staff = "down" fis4 \change Staff = "up" r2."#, "f4 r2."));
    assert_eq!(shown(&e, 1, 0).iter().map(|n| n.1.clone()).collect::<Vec<_>>(), vec![vec![Some(0)], vec![Some(1)]]);
}

#[test]
fn auto_change_counts_on_the_staff_each_note_is_drawn_on() {
    // bes under middle C, twice in the bar: on the lower staff, the flat once.
    let e = eng(r"\new PianoStaff \autoChange { bes4 c'' bes c'' }");
    assert_eq!(shown(&e, 1, 0), vec![(0, vec![Some(-1)]), (2 * Q, vec![None])]);
    assert_eq!(shown(&e, 0, 0), vec![(Q, vec![None]), (3 * Q, vec![None])]);
    // Grace notes come with their note and count there.
    let e = eng(&piano(r#"r4 \change Staff = "down" \grace fis8 fis4 \change Staff = "up" r2"#, "f4 r2."));
    let moved = e.staves[1][0].items.iter().find(|ev| ev.k == Kind::Note && ev.start == Q).unwrap();
    assert_eq!(moved.graces[0].heads[0].acc, Some(1), "the grace's sharp, after the f natural");
    assert_eq!(moved.heads[0].acc, None, "the note after its grace: shown already");
}
