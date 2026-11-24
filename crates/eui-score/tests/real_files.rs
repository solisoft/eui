//! Files found in the wild, read as they are (score #1031, #1032): what
//! `tests/lilypond/real/*.ly` hold is not edited to suit the engraver — a
//! file opens, and what it says that is not drawn is passed over with a
//! warning, never refused. The tests after them pin each construct the two
//! files needed, on a source of its own. After them, the four scores the
//! site held that its engraver refused (score #1036, #1037): a Telemann aria
//! twice, a set of Scottish tunes and a jig that never prints its music.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing, clippy::panic, clippy::arithmetic_side_effects)]

use std::fs;
use std::path::PathBuf;

use eui_score::parse::{parse, tuning_named, GroupKind, Kind, Parsed, StaffKind};
use eui_score::{engrave, engrave_paper, midi, paths, pdf, warnings, Layout, MidiOptions};

fn real(name: &str) -> String {
    fs::read_to_string(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/lilypond/real").join(format!("{name}.ly"))).unwrap()
}

fn read(src: &str) -> Parsed {
    parse(src).unwrap_or_else(|e| panic!("{src:?}: {e}"))
}

fn said(src: &str) -> Vec<String> {
    warnings(src).unwrap().into_iter().map(|w| w.message).collect()
}

/// A staff's notes as MIDI keys, a chord's lowest first.
fn keys(p: &Parsed, staff: usize) -> Vec<Vec<i64>> {
    p.staves[staff]
        .events
        .iter()
        .filter(|e| e.k == Kind::Note)
        .map(|e| e.pitches.iter().map(|x| 48 + 12 * x.d.div_euclid(7) + [0, 2, 4, 5, 7, 9, 11][x.d.rem_euclid(7) as usize] + x.alter).collect())
        .collect()
}

/// The keys a source's MIDI file strikes, in the order it writes them.
fn struck(src: &str) -> Vec<u8> {
    let bytes = midi(src, &MidiOptions::default()).unwrap();
    bytes.windows(3).filter(|w| w[0] & 0xF0 == 0x90 && w[1] < 128 && w[2] > 0 && w[2] < 128).map(|w| w[1]).collect()
}

/// Every entry point takes the file: a screen, its own paper, MIDI, a PDF.
fn opens(name: &str) -> Parsed {
    let src = real(name);
    let p = read(&src);
    let eng = engrave(&src, 900.0 / 8.0, &Layout::default()).unwrap_or_else(|e| panic!("{name}: {e}"));
    let (drawn, _) = paths(&eng, 8.0, 0).unwrap();
    assert!(drawn.len() > 100, "{name}: {} paths", drawn.len());
    let (paged, _) = engrave_paper(&src).unwrap_or_else(|e| panic!("{name} on paper: {e}"));
    assert!(!paged.pages.is_empty());
    let bytes = midi(&src, &MidiOptions::default()).unwrap_or_else(|e| panic!("{name} as MIDI: {e}"));
    assert_eq!(&bytes[..4], b"MThd", "{name}");
    assert!(pdf(&src, name).unwrap_or_else(|e| panic!("{name} as PDF: {e}")).starts_with("%PDF"));
    p
}

#[test]
fn le_grand_cerf_opens_as_it_is() {
    let p = opens("grand-cerf");
    assert_eq!(p.header["title"], "Le grand cerf");
    // One staff in a choir's group, its voice named, in F and 2/4.
    assert_eq!((p.staves.len(), p.staves[0].kind), (1, StaffKind::Notes));
    assert_eq!(p.staves[0].voices, vec![("V".to_string(), 1)]);
    assert!(p.groups.iter().any(|g| g.kind == GroupKind::Choir));
    let notes = keys(&p, 0);
    assert_eq!(notes.len(), 51, "a note a syllable");
    // r4 c8 f8 | f8. g16 f8 e8 | g4 c8 g8 …, from \relative c' in english.ly's names.
    assert_eq!(notes[..9].concat(), vec![60, 65, 65, 67, 65, 64, 67, 72, 67]);
    // … c4 c8 bf8 | d4 bf8 bf8: english.ly's B flat.
    assert!(notes.concat().contains(&70));
    assert_eq!(notes.last().unwrap(), &vec![65]);
    assert_eq!(p.paper.size.as_deref(), Some("a5"));
    assert_eq!(p.paper.staff_size, Some(18.0));
    // What is passed over, and said.
    assert_eq!(
        said(&real("grand-cerf")),
        vec![
            "the header field 'img' is not one a title page prints",
            "\\epsfile: a picture in a \\markup is not drawn",
            "\\set Score.barNumberVisibility is not drawn",
            "\\override MetronomeMark.stencil in \\layout is not drawn",
            "\\override LyricText.font-name in \\layout is not drawn",
            "\\override LyricText.font-size in \\layout is not drawn",
            "\\override LyricText.minimum-Y-extent in \\layout is not drawn",
            "\\stopStaff is not drawn",
            "\\epsfile: a picture in a \\markup is not drawn",
        ]
    );
    // The song's words, written in a \markup after the score: under the music.
    assert_eq!(
        p.after_lines(),
        [
            "Dans sa maison un grand cerf",
            "regardait par la fenêtre",
            "un lapin venir a lui",
            "et frapper ainsi.",
            "Cerf, cerf,",
            "ouvre moi !",
            "ou le chasseur me tuera !",
            "Lapin lapin",
            "entre et viens,",
            "me serrer la main."
        ]
    );
    let (paged, _) = engrave_paper(&real("grand-cerf")).unwrap();
    assert_eq!((paged.pages.len(), paged.after.len()), (1, 10));
}

#[test]
fn le_grand_cerf_sings_once_its_lyrics_line_is_uncommented() {
    // The file keeps `\new Lyrics \lyricsto "V" { \LI }` commented out, the
    // words in a \markup under the music: with the line back, they are sung.
    let src = real("grand-cerf").replace("%    \\new Lyrics", "\\new Lyrics");
    let p = read(&src);
    assert_eq!(p.staves.len(), 1);
    let words = p.staves[0].lyrics.as_ref().unwrap();
    assert_eq!(words.len(), 51);
    assert_eq!((words[0].text.as_str(), words[3].text.as_str(), words[50].text.as_str()), ("Dans", "son", "main."));
    engrave_paper(&src).unwrap();
}

#[test]
fn blues_en_sib_opens_as_it_is() {
    let p = opens("blues-en-sib");
    assert_eq!((p.header["title"].as_str(), p.header["composer"].as_str()), ("Blues en Sib", "Bireli Lagrene"));
    let kinds: Vec<StaffKind> = p.staves.iter().map(|s| s.kind).collect();
    assert_eq!(kinds, vec![StaffKind::ChordNames, StaffKind::Notes, StaffKind::TabStrings(4)]);
    // Four choruses of twelve bars and the last chord: the chord names, a
    // bar shorter, go on in silence.
    let whole = |s: usize| p.staves[s].events.iter().filter(|e| matches!(e.k, Kind::Note | Kind::Rest)).map(|e| e.ticks).sum::<i64>();
    assert_eq!((whole(0), whole(1), whole(2)), (49 * eui_score::WHOLE, 49 * eui_score::WHOLE, 49 * eui_score::WHOLE));
    // The group: a square bracket, the bass's name beside it.
    let g = p.groups.iter().find(|g| g.kind == GroupKind::Bracket).unwrap();
    assert_eq!((g.first, g.last, g.name.as_str()), (1, 2, "Basse électrique"));
    assert_eq!(p.staves[1].events.iter().find_map(|e| e.look.sym("systemStartDelimiter").map(str::to_owned)).as_deref(), Some("SystemStartSquare"));
    // The staff is written an octave over the tablature (\transpose c c')
    // and sounds where the tablature is (\transposition c).
    let (staff, tab) = (keys(&p, 1), keys(&p, 2));
    assert_eq!(staff.len(), tab.len());
    assert!(staff.iter().zip(&tab).all(|(a, b)| a.iter().zip(b).all(|(x, y)| x - 12 == *y)));
    assert_eq!(p.staves[1].props.transposition, -12);
    assert_eq!(p.staves[2].props.string_tunings, vec![43, 38, 33, 28]);
    // d'4 r g bf r4: \relative c, and its grace notes (cs' before, fs before g).
    assert_eq!(tab[..3].concat(), vec![62, 55, 58]);
    // The frets, on a bass's four strings: d' the G string's 19th, g its
    // 12th, b flat its 15th; the last bar's chord <d af' c> a string each.
    let eng = engrave(&real("blues-en-sib"), 900.0 / 8.0, &Layout::default()).unwrap();
    let frets: Vec<Vec<(i64, i64)>> = eng.staves[2].iter().flat_map(|b| b.items.iter()).filter(|e| e.k == Kind::Note).map(|e| e.tab.clone()).collect();
    assert_eq!(frets[..3], [vec![(1, 19)], vec![(1, 12)], vec![(1, 15)]]);
    assert!(frets.iter().flatten().all(|&(s, f)| (1..=4).contains(&s) && (0..=24).contains(&f)), "a fret off the neck");
    let lowest = tab.iter().flatten().min().unwrap();
    assert_eq!(*lowest, 28, "the open E string");
    assert!(frets.iter().any(|f| f == &vec![(4, 0)]));
    // Chord names: B♭7 E♭7 B♭7 (a bar of nothing) E♭7 E° B♭7 G7 Cm7 F7 …
    let names: Vec<String> = p.staves[0].events.iter().filter(|e| !e.chord_name.is_empty()).take(9).map(|e| e.chord_name.iter().map(|r| r.text.as_str()).collect::<String>()).collect();
    assert_eq!(names.len(), 9);
    assert!(names[0].starts_with('B') && names[1].starts_with('E') && names[4].starts_with('E') && names[7].starts_with('C'), "{names:?}");
    // \midi { \tempo 4 = 100 }: 600 000 microseconds a quarter.
    let bytes = midi(&real("blues-en-sib"), &MidiOptions::default()).unwrap();
    assert!(bytes.windows(6).any(|w| w == [0xFF, 0x51, 0x03, 0x09, 0x27, 0xC0]));
    // Nothing is passed over.
    assert_eq!(said(&real("blues-en-sib")), Vec::<String>::new());
}

#[test]
fn a_context_is_named_by_a_bare_word_and_a_voice_makes_its_staff() {
    // \context Voice = V in a \new Voice in a ChoirStaff, the variable
    // defined again from what it was.
    let src = "m = \\relative c' { c4 d e f }\nm = \\context ChoirStaff = \"V\" << \\new Voice << \\context Voice = V { << { \\key f \\major } \\m >> \\stopStaff } >> >>\n\\score { \\m }";
    let p = read(src);
    assert_eq!(p.staves.len(), 1);
    assert_eq!(p.staves[0].voices, vec![("V".to_string(), 1)]);
    assert_eq!(keys(&p, 0).concat(), vec![60, 62, 64, 65]);
    assert_eq!(said(src), vec!["\\stopStaff is not drawn"]);
    engrave(src, 100.0, &Layout::default()).unwrap();
    // None of it is the catalogue's subset: an older client is sent paths.
    for src in [
        "\\new Staff << \\context Voice = V { c'4 } >>",
        "{ c'4 4 }",
        "{ \\xNote c'4 }",
        "\\new TabStaff \\with { stringTunings = #bass-tuning } { e,,4 }",
        "<< \\new Staff { c'1 } \\new Staff { c1 c1 } >>",
    ] {
        assert_eq!(eui_score::protocol(src), Ok(11), "{src}");
    }
}

#[test]
fn a_voice_holds_its_lyrics_and_lyricsto_takes_a_variable_in_braces() {
    let src = "w = \\lyricmode { la li -- lo }\n\\new ChoirStaff << \\new Voice << \\context Voice = V { c'4 d' e' } \\new Lyrics \\lyricsto \"V\" { \\w } >> >>";
    let p = read(src);
    let words: Vec<&str> = p.staves[0].lyrics.as_ref().unwrap().iter().map(|s| s.text.as_str()).collect();
    assert_eq!(words, vec!["la", "li", "lo"]);
    // The same with \addlyrics, and with no braces round the variable.
    for src in ["w = \\lyricmode { la li -- lo }\n{ c'4 d' e' } \\addlyrics { \\w }", "w = \\lyricmode { la li -- lo }\n<< \\new Voice = \"V\" { c'4 d' e' } \\new Lyrics \\lyricsto \"V\" \\w >>"] {
        assert_eq!(read(src).staves[0].lyrics.as_ref().unwrap().len(), 3, "{src}");
    }
    // Words in the braces are still words; lyrics to a voice nobody named
    // are left out, and said.
    assert_eq!(read("<< \\new Voice = \"V\" { c'4 } \\new Lyrics \\lyricsto \"V\" { la } >>").staves[0].lyrics.as_ref().unwrap().len(), 1);
    let lost = "<< \\new Voice = \"V\" { c'4 } \\new Lyrics \\lyricsto \"W\" { la } >>";
    assert!(read(lost).staves[0].lyrics.is_none());
    assert!(said(lost)[0].starts_with("\\lyricsto \"W\" names no voice"));
}

#[test]
fn markup_this_does_not_draw_keeps_its_words() {
    let p = read("\\header { title = \\markup { \\override #'(font-name . \"petitamedium\") \"Le grand cerf\" } img = \\markup { \\epsfile #X #30 #\"cerf.eps\" } } { c'4 }");
    assert_eq!(p.header["title"], "Le grand cerf");
    // Over a note: a command of nobody's is said, its words drawn; a picture left out.
    let src = "{ c'4^\\markup { \\typewriter \"dolce\" \\epsfile #X #3 #\"a.eps\" \\fontsize #-3 \"e\" } d'4 }";
    let p = read(src);
    let over: Vec<&str> = p.staves[0].events[0].texts.iter().flat_map(|w| w.runs.iter().map(|r| r.text.as_str())).collect();
    assert_eq!(over, vec!["dolce", "e"]);
    assert_eq!(said(src), vec!["\\typewriter is not markup this draws", "\\epsfile: a picture in a \\markup is not drawn"]);
}

#[test]
fn words_after_the_music_are_set_under_it() {
    use eui_score::bands;
    let plain = "\\book { \\score { { c'1 } \\layout {} } }";
    let book = "\\book { \\score { { c'1 } \\layout {} } \\markup { \\vspace #1 \\hspace #10 \\override #'(font-name . \"x\") \\fontsize #-1 \\column { \"one two\" \"    \" \"three\" } \\column { \" \" { \\hspace #5 \\epsfile #X #10 #\"l.eps\" } } } }";
    // A column's items a line each, a line of spaces an empty one; the picture said.
    assert_eq!(read(book).after_lines(), ["one two", "three"]);
    assert_eq!(said(book), vec!["\\epsfile: a picture in a \\markup is not drawn"]);
    // On a screen: under the last system, the score that much taller.
    let (with, without) = (engrave(book, 100.0, &Layout::default()).unwrap(), engrave(plain, 100.0, &Layout::default()).unwrap());
    let lines: Vec<(&str, f64)> = with.after.iter().map(|w| (w.text.as_str(), w.y)).collect();
    assert_eq!(lines.iter().map(|l| l.0).collect::<Vec<_>>(), ["one two", "three"]);
    // Two lines down, a `\fontsize #-1`'s lines: a step nearer one another than three spaces.
    let skip = 3.0 * (-1.0_f64 / 6.0).exp2();
    assert!(lines[0].1 > with.systems[0].bottom && (lines[1].1 - lines[0].1 - 2.0 * skip).abs() < 1e-9, "{lines:?}");
    // `\hspace #10` before the column: that far from where the staves start, and more.
    assert!(with.after.iter().all(|w| w.x > with.left + 10.0 && w.em < eui_score::text::em(0.0)));
    assert!(with.height > without.height + 6.0 && with.height > lines[1].1);
    assert!(without.after.is_empty());
    let (drawn, bare) = (paths(&with, 8.0, 0).unwrap().0, paths(&without, 8.0, 0).unwrap().0);
    assert!(drawn.len() > bare.len());
    assert_eq!(drawn[..bare.len()], bare[..], "the music is where it was");
    // In the bands, the last one's, which reaches the foot.
    let total = (with.height * 8.0).ceil() as i64;
    let cut = bands(&with, 8.0, 0, total).unwrap();
    assert_eq!(cut.iter().map(|b| b.h).sum::<i64>(), total);
    let all: usize = cut.iter().map(|b| b.paths.len()).sum();
    let all_bare: usize = bands(&without, 8.0, 0, (without.height * 8.0).ceil() as i64).unwrap().iter().map(|b| b.paths.len()).sum();
    assert_eq!(all - all_bare, drawn.len() - bare.len());
    // On paper: on the last page where there is room, on pages of their own past it.
    let (paged, _) = engrave_paper(book).unwrap();
    assert_eq!((paged.pages.len(), paged.after.iter().filter(|w| w.page == 0).count()), (1, 2));
    let long = format!("{{ c'1 }} \\markup {{ \\column {{ {} }} }}", "\"la la la\" ".repeat(150));
    let (paged, _) = engrave_paper(&long).unwrap();
    assert!(paged.pages.len() >= 2 && paged.after.len() == 150);
    assert!(paged.after.iter().any(|w| w.page == paged.pages.len() - 1));
    for page in 0..paged.pages.len() {
        paths(&paged, 5.0, page).unwrap();
    }
    assert!(pdf(&long, "t").unwrap().starts_with("%PDF"));
    // Words on one line, and two blocks, the second from its own line on.
    assert_eq!(read("{ c'1 } \\markup \\bold \"Fin\"").after_lines(), ["Fin"]);
    let two = read("{ c'1 } \\markup { one \\italic two } \\markup { \\line { three four } }");
    assert_eq!((two.after_lines(), two.blocks.len()), (vec!["one two".to_owned(), "three four".to_owned()], 2));
    // Before the music, or between two scores, they are words of the page too (score #1025).
    for src in ["\\markup { \"Preface\" } { c'1 }", "\\score { { c'1 } } \\markup { \"Interlude\" } \\score { { d'1 } }"] {
        assert_eq!((read(src).blocks.len(), read(src).blocks[0].before), (1, usize::from(src.contains("Interlude"))), "{src}");
        assert!(said(src).is_empty(), "{src}");
    }
}

#[test]
fn a_duration_alone_repeats_the_note_before_it() {
    let p = read("\\relative c' { c4~ 8 r8 8 <c e>4 2 \\tuplet 3/2 { d8 8 8 } }");
    assert_eq!(keys(&p, 0), vec![vec![60], vec![60], vec![60], vec![60, 64], vec![60, 64], vec![62], vec![62], vec![62]]);
    let ticks: Vec<i64> = p.staves[0].events.iter().filter(|e| e.k == Kind::Note).map(|e| e.ticks * 8 / eui_score::WHOLE).collect();
    assert_eq!(ticks[..5], [2, 1, 1, 2, 4]);
    // `r16fs`: no space wanted; a duration with no note before it is refused.
    assert_eq!(keys(&read("{ r16fis' g'8 }"), 0), vec![vec![66], vec![67]]);
    assert!(parse("{ 4 c' }").unwrap_err().message.contains("repeats the note before it"));
    // A chord's names keep their own reading of a figure.
    assert_eq!(read("\\chordmode { c1:7 }").staves[0].events.iter().filter(|e| e.k == Kind::Note).count(), 1);
}

#[test]
fn x_notes_and_harmonic_notes_change_their_heads_only() {
    let p = read("\\relative c' { c4 \\xNote d \\harmonicNote <e g>8.~ \\harmonicNote q16 \\deadNote { f8 g } a4 }");
    let styles: Vec<Option<String>> = p.staves[0].events.iter().filter(|e| e.k == Kind::Note).map(|e| e.look.sym("NoteHead.style").map(str::to_owned)).collect();
    let (x, h) = (Some("cross".to_string()), Some("harmonic".to_string()));
    assert_eq!(styles, vec![None, x.clone(), h.clone(), h, x.clone(), x, None]);
    // Drawn with those heads, and played as any note.
    let src = "{ c'4 \\xNote d' }";
    let heads = |s: &str| paths(&engrave(s, 100.0, &Layout::default()).unwrap(), 8.0, 0).unwrap().0;
    assert_ne!(heads(src), heads("{ c'4 d' }"));
    assert_eq!(struck(src), vec![60, 62]);
    assert!(parse("{ \\xNote }").is_err());
}

#[test]
fn a_staff_that_stops_early_goes_on_in_silence() {
    let src = "<< \\new ChordNames \\chordmode { c1 g:7 } \\new Staff { c'1 d' e' } >>";
    let p = read(src);
    let last = p.staves[0].events.last().unwrap();
    assert!(last.k == Kind::Rest && last.spacer && last.ticks == eui_score::WHOLE);
    let eng = engrave(src, 100.0, &Layout::default()).unwrap();
    assert_eq!((eng.staves[0].len(), eng.staves[1].len()), (3, 3));
    // Two staves of notes: the shorter one's last bars are empty.
    let two = engrave("\\new PianoStaff << \\new Staff { c'1 } \\new Staff { c1 c1 } >>", 100.0, &Layout::default()).unwrap();
    assert_eq!(two.staves[0].len(), 2);
    assert!(two.staves[0][1].items.iter().all(|e| e.k != Kind::Note));
}

#[test]
fn a_tablature_has_the_strings_its_tuning_gives_it() {
    let frets = |src: &str| -> Vec<Vec<(i64, i64)>> {
        let eng = engrave(src, 100.0, &Layout::default()).unwrap_or_else(|e| panic!("{src}: {e}"));
        eng.staves[0].iter().flat_map(|b| b.items.iter()).filter(|e| e.k == Kind::Note).map(|e| e.tab.clone()).collect()
    };
    // A bass: E1 A1 D2 G2, the open strings and a note up the G string.
    let bass = "\\new TabStaff \\with { stringTunings = #bass-tuning } { e,,4 a,, d, g, | f,,4 c, d' <e,, g,>4 }";
    assert_eq!(read(bass).staves[0].kind, StaffKind::TabStrings(4));
    assert_eq!(frets(bass), vec![vec![(4, 0)], vec![(3, 0)], vec![(2, 0)], vec![(1, 0)], vec![(4, 1)], vec![(3, 3)], vec![(1, 19)], vec![(1, 0), (4, 0)]]);
    // Four lines and a smaller sign: not a guitar's picture.
    let six = "\\new TabStaff { e,4 a, d g }";
    let lines = |src: &str| paths(&engrave(src, 100.0, &Layout::default()).unwrap(), 8.0, 0).unwrap().0.len();
    assert!(lines(bass) != lines(six));
    assert_eq!(read(six).staves[0].kind, StaffKind::Tab);
    assert_eq!(frets(six), vec![vec![(6, 0)], vec![(5, 0)], vec![(4, 0)], vec![(3, 0)]]);
    // Six strings tuned otherwise stay a guitar's staff: drop D's low string.
    let drop = "\\new TabStaff \\with { stringTunings = #guitar-drop-d-tuning } { d,4 e, a, e' }";
    assert_eq!(read(drop).staves[0].kind, StaffKind::Tab);
    assert_eq!(frets(drop), vec![vec![(6, 0)], vec![(6, 2)], vec![(5, 0)], vec![(1, 0)]]);
    assert_eq!(frets("\\new TabStaff \\with { stringTunings = #guitar-tuning } { e,4 }"), vec![vec![(6, 0)]]);
    // A tuning written out, lowest string first, as LilyPond writes it; from a variable too.
    let own = "\\new TabStaff \\with { stringTunings = \\stringTuning <b,,, e,, a,, d, g,> } { b,,,4 g, }";
    assert_eq!(read(own).staves[0].props.string_tunings, tuning_named("bass-five-string-tuning").unwrap());
    assert_eq!(frets(own), vec![vec![(5, 0)], vec![(1, 0)]]);
    let var = "uke = \\stringTuning <g' c' e' a'>\n\\new TabStaff \\with { stringTunings = \\uke } { a'4 c' }";
    assert_eq!(read(var).staves[0].kind, StaffKind::TabStrings(4));
    assert_eq!(frets(var), vec![vec![(1, 0)], vec![(3, 0)]]);
    // One nobody knows: a guitar's, and said.
    let odd = "\\new TabStaff \\with { stringTunings = #sitar-tuning } { e,4 }";
    assert_eq!(said(odd), vec!["stringTunings = #sitar-tuning is not a tuning this knows: the tablature is a guitar's"]);
    assert_eq!(frets(odd), vec![vec![(6, 0)]]);
    // A note under the lowest string has no fret, and nothing fails.
    assert_eq!(frets("\\new TabStaff \\with { stringTunings = #bass-tuning } { c,,4 }"), vec![vec![]]);
    // On paper and as MIDI.
    engrave_paper(bass).unwrap();
    assert_eq!(struck(bass)[..4], [28, 33, 38, 43]);
}

#[test]
fn a_group_is_joined_by_the_sign_it_names() {
    let with = |sign: &str| {
        format!("\\new StaffGroup \\with {{ systemStartDelimiter = #'{sign} \\consists \"Instrument_name_engraver\" instrumentName = \"Bass\" }} << \\new Staff {{ c'1 }} \\new Staff {{ c1 }} >>")
    };
    let drawn = |src: &str| paths(&engrave(src, 100.0, &Layout::default()).unwrap(), 8.0, 0).unwrap().0;
    let plain = "\\new StaffGroup \\with { instrumentName = \"Bass\" } << \\new Staff { c'1 } \\new Staff { c1 } >>";
    let (square, bracket, bar, brace) = (drawn(&with("SystemStartSquare")), drawn(&with("SystemStartBracket")), drawn(&with("SystemStartBar")), drawn(&with("SystemStartBrace")));
    assert_eq!(bracket, drawn(plain));
    assert!(square != bracket && bar != bracket && brace != bracket && square != bar && brace != square);
    // A bracket's line and its two hooks; a square's and its two; a bar, none of them.
    assert_eq!(square.len(), bracket.len());
    assert_eq!(bar.len() + 3, square.len());
    assert_eq!(said(&with("SystemStartSquare")), Vec::<String>::new());
    assert_eq!(read(&with("SystemStartSquare")).groups[0].name, "Bass");
    // Another engraver is still passed over, and said.
    assert_eq!(said("\\new StaffGroup \\with { \\consists \"Ambitus_engraver\" } << \\new Staff { c'1 } \\new Staff { c1 } >>"), vec!["\\consists in \\with is not read"]);
}

#[test]
fn a_transposition_is_played_and_the_midi_block_gives_the_tempo() {
    // Written middle C sounds the pitch named: a bass's octave, a B flat clarinet's tone.
    assert_eq!(struck("{ \\transposition c c'4 }"), vec![48]);
    assert_eq!(struck("{ \\transposition bes c'4 d' }"), vec![58, 60]);
    assert_eq!(struck("\\language \"english\" { \\transposition bf c'4 }"), vec![58]);
    assert_eq!(struck("{ c'4 }"), vec![60]);
    assert_eq!(said("{ \\transposition bes c'4 }"), Vec::<String>::new());
    // Drawn as written.
    assert_eq!(keys(&read("{ \\transposition c c'4 }"), 0), vec![vec![60]]);
    // \midi { \tempo 4 = 90 }: 666 666 microseconds a quarter; a dotted beat; the host's speed otherwise.
    let tempo = |src: &str, opts: &MidiOptions| -> Vec<u32> {
        let b = midi(src, opts).unwrap();
        b.windows(6).filter(|w| w[..3] == [0xFF, 0x51, 0x03]).map(|w| u32::from(w[3]) << 16 | u32::from(w[4]) << 8 | u32::from(w[5])).collect()
    };
    let host = MidiOptions { bpm: 60.0, ..MidiOptions::default() };
    assert_eq!(tempo("\\score { { c'4 } \\midi { \\tempo 4 = 90 } }", &MidiOptions::default()), vec![666_666]);
    assert_eq!(tempo("\\score { { c'4 } \\midi { \\tempo 4 = 90 } }", &host), vec![666_666]);
    assert_eq!(tempo("\\score { { c'4 } \\midi { \\tempo 4. = 60 } }", &host), vec![666_666]);
    assert_eq!(tempo("\\score { { c'4 } \\midi { } }", &host), vec![1_000_000]);
    assert_eq!(said("\\score { { c'4 } \\midi { \\tempo 4 = 90 } }"), Vec::<String>::new());
    // The music's own \tempo comes after it, and wins from where it is written.
    assert_eq!(tempo("\\score { { \\tempo 4 = 120 c'4 } \\midi { \\tempo 4 = 90 } }", &MidiOptions::default()), vec![666_666, 500_000]);
}

// ---- Score #1036, #1037: the four the site held and could not open ---------

const TELEMANN_SAID: [&str; 30] = [
    "\\paper's two-sided is not read",
    "\\paper's inner-margin is not read",
    "the long s and the ligatures of an old print (ſ, ﬅ, ß …) are set in plain letters: the words' face has none",
    "\\stopStaff is not drawn",
    "a figure's stroke (6\\\\, 6/) is not drawn: the figure is set plain",
    "implicitBassFigures in \\with is not read",
    "\\override StaffSymbol.line-thickness in \\layout is not drawn",
    "\\override Slur.thickness in \\layout is not drawn",
    "\\override Tie.minimum-length in \\layout is not drawn",
    "\\override Stem.thickness in \\layout is not drawn",
    "\\override Hairpin.thickness in \\layout is not drawn",
    "\\override BarLine.hair-thickness in \\layout is not drawn",
    "\\override BarNumber.break-align-symbols in \\layout is not drawn",
    "\\override BarNumber.X-offset in \\layout is not drawn",
    "\\override BarNumber.Y-offset in \\layout is not drawn",
    "\\override BarNumber.font-name in \\layout is not drawn",
    "\\override BarNumber.font-size in \\layout is not drawn",
    "\\override LyricText.font-name in \\layout is not drawn",
    "\\override LyricText.font-size in \\layout is not drawn",
    "\\override InstrumentName.font-name in \\layout is not drawn",
    "\\override InstrumentName.font-size in \\layout is not drawn",
    "\\override BarNumber.padding in \\layout is not drawn",
    "figuredBassPlusStrokedAlist in \\layout is not read",
    "figuredBassPlusDirection in \\layout is not read",
    "\\override BassFigure.font-name in \\layout is not drawn",
    "\\override BassFigure.font-size in \\layout is not drawn",
    "\\override NoteHead.style in \\layout is not drawn",
    "\\override Custos.style in \\layout is not drawn",
    "\\consists in \\layout is not read",
    "\\accepts in \\layout is not read",
];

fn telemann(name: &str) {
    let p = opens(name);
    assert_eq!(p.header["title"], "telemann");
    // The flute, the soprano, the continuo with its figures over it and
    // its ossia under it.
    let kinds: Vec<StaffKind> = p.staves.iter().map(|s| s.kind).collect();
    assert_eq!(kinds, vec![StaffKind::Notes, StaffKind::Notes, StaffKind::Figures, StaffKind::Notes, StaffKind::Ossia]);
    assert!(p.staves[2].props.above && p.staves[4].props.below);
    assert_eq!(p.staves[3].name.as_deref(), Some("bc"));
    // Forty-six bars of 4/4 with their pickup, every staff.
    let whole = |s: usize| p.staves[s].events.iter().filter(|e| matches!(e.k, Kind::Note | Kind::Rest)).map(|e| e.ticks * e.count.max(1)).sum::<i64>();
    assert!((0..5).all(|s| whole(s) == 46 * eui_score::WHOLE), "{:?}", (0..5).map(whole).collect::<Vec<_>>());
    // \clef french: a G clef on the first line.
    let clef = p.staves[0].events.iter().find(|e| e.k == Kind::Clef).unwrap().clef;
    assert_eq!(Some(clef), eui_score::parse::Clef::named("french"));
    // The flute starts on g'', the soprano on g', the bass on c.
    assert_eq!((keys(&p, 0)[0][0], keys(&p, 1)[0][0], keys(&p, 3)[0][0]), (79, 67, 48));
    // The thorough bass: <6 4>8 <5 3> … twelve figures, the first two stacked.
    let figures: Vec<Vec<String>> =
        p.staves[2].events.iter().filter(|e| !e.figures.is_empty()).map(|e| e.figures.iter().map(|l| l.iter().map(|r| r.text.as_str()).collect::<String>()).collect()).collect();
    assert_eq!(figures.len(), 12);
    assert_eq!(figures[..4], [vec!["6", "4"], vec!["5", "3"], vec!["7"], vec!["7"]]);
    // The ossia: a quarter a lasting a sixteenth, "Orig." over it, in bar 11.
    let ossia: Vec<&eui_score::parse::Ev> = p.staves[4].events.iter().filter(|e| e.k == Kind::Note).collect();
    assert_eq!(ossia.len(), 1);
    assert_eq!((ossia[0].base, ossia[0].ticks, ossia[0].texts[0].text.as_str()), (4, eui_score::WHOLE / 16, "Orig."));
    // The words, the old print's letters written out: "sein Geist steht".
    let words: Vec<&str> = p.staves[1].lyrics.as_ref().unwrap().iter().map(|s| s.text.as_str()).collect();
    for word in ["Geist", "steht", "sein", "biss", "sie", "ist"] {
        assert!(words.contains(&word), "{word} in {words:?}");
    }
    // "Da Capo.", written in the bass: the score's, said by its first staff.
    assert!(p.staves[0].events.iter().any(|e| e.k == Kind::Mark && e.texts.iter().any(|w| w.runs.iter().any(|r| r.text == "Da Capo."))));
    assert!(!p.staves[3].events.iter().any(|e| e.k == Kind::Mark));
    // \paper: 18.5 cm lines, pages numbered from 2, a header each side.
    assert_eq!((p.paper.line_width, p.paper.first_page_number, p.paper.staff_size), (Some(185.0), Some(2), Some(16.0)));
    assert!(p.paper.odd_header.is_some() && p.paper.even_header.is_some());
    assert_eq!(said(&real(name)), TELEMANN_SAID);
    // Engraved: every syllable has its note — a beam written by hand holds
    // one, as a slur does — so the last word is under the last bar.
    let eng = engrave(&real(name), 900.0 / 8.0, &Layout::default()).unwrap();
    let sung: Vec<(usize, String)> =
        eng.staves[1].iter().enumerate().flat_map(|(b, bar)| bar.items.iter().filter_map(move |e| e.lyric.as_ref().map(|l| (b, l.text.clone())))).filter(|(_, t)| !t.is_empty()).collect();
    assert_eq!(sung.last().unwrap(), &(eng.staves[1].len() - 1, "neu.".to_string()));
    assert_eq!(sung.len(), words.iter().filter(|w| !w.is_empty()).count());
    let (paged, _) = engrave_paper(&real(name)).unwrap();
    assert_eq!(paged.pages.len(), 3);
}

#[test]
fn telemann_opens_as_it_is() {
    telemann("telemann");
}

#[test]
fn telemann_as_the_site_stored_it_opens_too() {
    telemann("telemann-prod");
}

#[test]
fn set_2_antiquities_opens_as_it_is() {
    let p = opens("set-2-antiquities");
    // The book's titles; each score's own title is its piece's.
    assert_eq!((p.header["title"].as_str(), p.header["subtitle"].as_str(), p.header["piece"].as_str()), ("Set 2", "Antiquities of Scottish Music", "I. Johnnie Cope"));
    assert_eq!(p.staves.len(), 1);
    let pieces: Vec<String> = p.staves[0].events.iter().filter(|e| e.k == Kind::Movement).map(|e| e.texts[0].text.clone()).collect();
    assert_eq!(pieces, ["II. Miss Hog, Newliston", "III. Mrs Morthland", "IV. As a Thòiseach"]);
    // Each tune's number beside its first line: the circle round it is not drawn.
    let names: Vec<String> = eui_score::look::changes(&p.staves[0].events, eui_score::look::INSTRUMENT_NAME)
        .into_iter()
        .filter_map(|c| match c.1 {
            eui_score::look::Val::Sym(n) => Some(n),
            _ => None,
        })
        .collect();
    assert_eq!(names, ["1", "2", "3", "4"]);
    assert_eq!((p.paper.left, p.paper.right, p.paper.top, p.paper.staff_size), (Some(15.0), Some(15.0), Some(10.0), Some(18.0)));
    assert_eq!(
        said(&real("set-2-antiquities")),
        vec![
            "\\paper's print-all-headers is not read",
            "the header field 'page-count' is not one a title page prints",
            "\\draw-circle is not markup this draws",
            "\\draw-circle is not markup this draws",
            "\\draw-circle is not markup this draws",
            "\\draw-circle is not markup this draws",
        ]
    );
    // On its paper: one page, the first tune's number beside its first line.
    let (paged, _) = engrave_paper(&real("set-2-antiquities")).unwrap();
    assert_eq!(paged.pages.len(), 1);
    assert_eq!(paged.systems[0].names, ["1"]);
}

#[test]
fn a_jig_that_never_prints_its_music_shows_the_last_it_defines() {
    for name in ["untitled-jig", "untitled-jig-versioned"] {
        let p = opens(name);
        assert_eq!((p.header["title"].as_str(), p.header["composer"].as_str()), ("Untitled Jig", "from a recording by Seamus Egan"));
        assert_eq!(p.staves.len(), 1);
        // \fixed c' { … e8 e e g fis e …: 24 bars of 6/8.
        assert_eq!(keys(&p, 0)[..6].concat(), vec![64, 64, 64, 67, 66, 64]);
        let whole: i64 = p.staves[0].events.iter().filter(|e| matches!(e.k, Kind::Note | Kind::Rest)).map(|e| e.ticks).sum();
        assert_eq!(whole, 24 * eui_score::WHOLE * 6 / 8);
        assert_eq!(said(&real(name)), vec!["the header field 'encoder' is not one a title page prints", "this file never prints its music: \\PAVA, the last it defines, is shown"]);
        // The repeat keeps its sign where \section follows it.
        let eng = engrave(&real(name), 900.0 / 8.0, &Layout::default()).unwrap();
        assert_eq!((eng.staves[0][7].bar.as_str(), eng.staves[0][15].bar.as_str()), (":|.", "||"));
    }
}

#[test]
fn a_file_with_its_music_printed_is_not_given_another() {
    // A variable left unused beside a score: the score is the music.
    let p = read("spare = { g'1 }\n{ c'1 }");
    assert_eq!(keys(&p, 0), vec![vec![60]]);
    assert!(p.warnings.is_empty());
    // Nothing that holds music: still no music.
    assert_eq!(parse("\\header { title = \"x\" }\nwords = \\lyricmode { la la }").unwrap_err().message, "score: there is no music (at character 0)");
}

#[test]
fn a_scheme_comment_is_passed_over() {
    let src = "\\paper {\n  #(define fonts\n    (set-global-fonts\n      ; #:music \"beethoven\"\n      ;  #:factor (/ staff-height pt 20)\n  ))\n  line-width = 18.5 \\cm\n}\n\\layout { \\context { \\Score \\override VerticalAxisGroup.default-staff-staff-spacing =\n #'((basic-distance . 0)\n (padding . -0 ) ;ajustez pour l'ossia\n ) } }\n{ c'1 }";
    let p = read(src);
    assert_eq!(p.paper.line_width, Some(185.0));
    // Outside Scheme a semicolon is still not notation.
    assert!(parse("{ c'1 ; }").unwrap_err().message.contains("';' is not part of the notation"));
}

#[test]
fn a_variable_keeps_the_words_added_to_its_staff() {
    let src = "part = \\new Staff { c'4 d' e' f' }\n\\addlyrics { do re mi fa }\nfigs = \\figuremode { <6>1 }\n\\score { << \\part \\new Staff { \\clef bass << { c1 } \\figs >> } >> }";
    let p = read(src);
    assert_eq!(p.staves.iter().map(|s| s.kind).collect::<Vec<_>>(), vec![StaffKind::Notes, StaffKind::Figures, StaffKind::Notes]);
    assert_eq!(p.staves[0].lyrics.as_ref().unwrap().len(), 4);
    assert!(p.staves[1].props.above);
    engrave_paper(src).unwrap();
    // \bassFigureStaffAlignmentDown: under their staff.
    let p = read("{ \\clef bass << { c1 } \\figuremode { \\bassFigureStaffAlignmentDown <6>1 } >> }");
    assert_eq!(p.staves.iter().map(|s| s.kind).collect::<Vec<_>>(), vec![StaffKind::Notes, StaffKind::Figures]);
    assert!(p.staves[1].props.below);
}

#[test]
fn a_rest_holds_a_fermata_and_a_note_lasts_a_part_of_itself() {
    let p = read("{ c'4 r4\\fermata a4*1/4 s8. r4*2 R1*3/4 }");
    let timed: Vec<&eui_score::parse::Ev> = p.staves[0].events.iter().filter(|e| matches!(e.k, Kind::Note | Kind::Rest)).collect();
    assert_eq!(timed[1].scripts[0].name, "fermata");
    let w = eui_score::WHOLE;
    assert_eq!((timed[2].base, timed[2].ticks, timed[2].scale), (4, w / 16, Some((1, 4))));
    assert_eq!((timed[4].base, timed[4].ticks), (4, w / 2));
    assert_eq!(timed[5].ticks, w * 3 / 4);
    assert_eq!(parse("{ c'4*0 }").unwrap_err().message, "score: a duration is multiplied by a number or a fraction, like 4*2/3 (at character 5)");
}

#[test]
fn a_bar_line_and_a_jump_written_in_one_staff_are_the_scores() {
    // The last bar stops short in the first staff only: every staff's.
    let src = "<< \\new Staff { \\partial 8 c'8 c'1 c'2.. \\bar \"||\" } \\new Staff { \\partial 8 c8 c1 c2.. \\jump \"Da Capo.\" } >>";
    let p = read(src);
    assert!(p.staves[1].events.iter().any(|e| e.k == Kind::Bar && e.v == "||"));
    assert!(p.staves[0].events.iter().any(|e| e.k == Kind::Mark) && !p.staves[1].events.iter().any(|e| e.k == Kind::Mark));
    let eng = engrave(src, 900.0 / 8.0, &Layout::default()).unwrap();
    assert_eq!(eng.staves[0].len(), eng.staves[1].len());
    // A staff that holds a note across it keeps its bar: said, not hidden.
    let err = engrave("<< \\new Staff { c'2 \\bar \"||\" c'2 } \\new Staff { c1 } >>", 900.0 / 8.0, &Layout::default()).unwrap_err();
    assert!(err.message.contains("every staff has the same bars") || err.message.contains("is not as long"), "{}", err.message);
}

#[test]
fn a_context_the_layout_defines_is_an_ossia_under_its_staff() {
    let layout = "\\layout { \\context { \\Score \\accepts \"Ossia\" } \\context { \\Staff \\name \"Ossia\" alignBelowContext = #\"bc\" \\alias \"Staff\" \\omit TimeSignature \\magnifyStaff #2/3 firstClef = ##f } }";
    let src =
        format!("\\score {{ << \\new Staff {{ c''1 c''1 }} \\new Staff = \"bc\" {{ \\clef bass c2 << {{ d4 }} \\context Ossia \\with {{ \\clef bass }} {{ e4^\"Orig.\" }} >> f4 c1 }} >> {layout} }}");
    let p = read(&src);
    assert_eq!(p.staves.iter().map(|s| s.kind).collect::<Vec<_>>(), vec![StaffKind::Notes, StaffKind::Notes, StaffKind::Ossia]);
    assert!(p.staves[2].props.below);
    // The layout's block is the ossia's, not every staff's: only it is smaller.
    let eng = engrave(&src, 900.0 / 8.0, &Layout::default()).unwrap();
    let scales: Vec<Option<f64>> = eng.systems[0].lines.iter().map(|l| l.scale).collect();
    assert!(scales[0].is_none() && scales[1].is_none() && scales[2].is_some_and(|s| (s - 2.0 / 3.0).abs() < 0.01), "{scales:?}");
    // Under its staff, and the staff after it no further than without it.
    let offsets = &eng.systems[0].offsets;
    assert!(offsets[2] > offsets[1] + 4.0, "{offsets:?}");
    // Over its staff where it says so, as before.
    let p = read("{ c'1 << { d'1 } \\new Staff \\with { alignAboveContext = \"m\" } { e'1 } >> }");
    assert_eq!(p.staves.iter().map(|s| s.kind).collect::<Vec<_>>(), vec![StaffKind::Ossia, StaffKind::Notes]);
    assert!(!p.staves[0].props.below);
}

#[test]
fn an_ossia_under_its_staff_takes_no_room_where_it_is_silent() {
    let with = "<< \\new Staff = \"a\" { c'1 << { d'1 } \\new Staff \\with { alignBelowContext = \"a\" } { e'1 } >> \\break c'1 c'1 } \\new Staff { c1 c1 c1 c1 } >>";
    let without = "<< \\new Staff = \"a\" { c'1 d'1 \\break c'1 c'1 } \\new Staff { c1 c1 c1 c1 } >>";
    let (a, b) = (engrave(with, 900.0 / 8.0, &Layout::default()).unwrap(), engrave(without, 900.0 / 8.0, &Layout::default()).unwrap());
    assert_eq!(a.systems.len(), 2);
    // The second system: the lower staff where it is without the ossia.
    assert!((a.systems[1].offsets[2] - b.systems[1].offsets[1]).abs() < 1e-6, "{:?} {:?}", a.systems[1].offsets, b.systems[1].offsets);
    // The first: the ossia between the two.
    assert!(a.systems[0].offsets[1] > 4.0 && a.systems[0].offsets[2] > a.systems[0].offsets[1] + 2.0);
}

#[test]
fn a_beam_written_by_hand_holds_its_syllable() {
    let src = "{ \\autoBeamOff c'8[ d'] e'8 f'8[ g' a'] b'4 } \\addlyrics { one two three four }";
    let eng = engrave(src, 900.0 / 8.0, &Layout::default()).unwrap();
    let sung: Vec<(i64, String)> = eng.staves[0].iter().flat_map(|b| b.items.iter()).filter_map(|e| e.lyric.as_ref().map(|l| (e.start, l.text.clone()))).collect();
    let e = eui_score::WHOLE / 8;
    assert_eq!(sung, [(0, "one".to_string()), (2 * e, "two".to_string()), (3 * e, "three".to_string()), (6 * e, "four".to_string())]);
}

#[test]
fn markup_takes_a_number_after_a_space_and_one_over_another() {
    // `\fontsize # 4`, `#.2`, `\combine a b`, `\number`.
    let src = "\\header { title = \\markup { \\override #'(font-name . \"Copperplate\") \\fontsize # 4 \\bold \"Set 2\" } }\n\\new Staff { \\set Staff.instrumentName = \\markup { \\combine \\draw-circle #2.2 #.2 ##f \\vcenter \\halign #CENTER \\huge \\number \"1\" } c'1 }";
    let p = read(src);
    assert_eq!(p.header["title"], "Set 2");
    assert_eq!(said(src), vec!["\\draw-circle is not markup this draws"]);
    // A lone staff's name is drawn beside it, its line indented for it.
    let eng = engrave(src, 900.0 / 8.0, &Layout::default()).unwrap();
    assert_eq!(eng.systems[0].names, ["1"]);
    // Outside the music too.
    read("\\markup { \\combine \\draw-circle #2.2 #.2 ##f \"A\" }\n{ c'1 }");
}

#[test]
fn the_first_scores_header_is_its_own_under_the_files_titles() {
    let src = "\\header { title = \"Set\" subtitle = \"Tunes\" }\n\\score { { c'1 } \\header { title = \"I. One\" subtitle = ##f opus = ##f } }\n\\score { { d'1 } \\header { title = \"II. Two\" } }";
    let p = read(src);
    assert_eq!((p.header["title"].as_str(), p.header["subtitle"].as_str(), p.header["piece"].as_str()), ("Set", "Tunes", "I. One"));
    assert!(p.warnings.is_empty(), "{:?}", p.warnings);
    // With no title before it, a score's header titles the file, as it did.
    let p = read("\\score { { c'1 } \\header { title = \"Alone\" } }");
    assert_eq!(p.header["title"], "Alone");
    assert!(!p.header.contains_key("piece"));
}

#[test]
fn letters_the_face_lacks_are_made_or_written_out() {
    use eui_score::glyphs::{letters, Weight};
    let face = letters().face(Weight::Regular);
    // ò: o's body and à's grave, as wide as o.
    let (o, grave) = (&face[&'o'], &face[&'ò']);
    assert_eq!(o.a, grave.a);
    assert!(grave.f.len() > o.f.len());
    for ch in "ìòáíóúýÌÒÁÍÓÚÝñſ".chars() {
        assert!(face.contains_key(&ch), "{ch}");
    }
    // A syllable's ligatures, in plain letters.
    let p = read("{ c'4 d' e' f' } \\addlyrics { Gei -- ﬅe biß \u{FB08}e }");
    let words: Vec<&str> = p.staves[0].lyrics.as_ref().unwrap().iter().map(|s| s.text.as_str()).collect();
    assert_eq!(words, ["Gei", "ste", "biss", "sie"]);
    assert_eq!(p.warnings.len(), 1);
}
