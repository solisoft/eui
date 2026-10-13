//! `cargo run -p xtask -- smufl <Bravura.otf> [OFL.txt]`: the glyph table
//! the catalogue's `score()` engraves with, written to standard output as
//! Soli — a function, `smufl_glyph_table()`, because the spec tooling copies
//! definitions and never a module's constants. The licence, when given, is
//! carried in the file as comments, so the outlines never travel without it.
//!
//! A canvas fill (03 §1.1, kind `5`) is the only way a server can put a
//! notehead on the screen at the size and the place the engraving wants:
//! `text` comes in eight sizes and on a coarse grid. So the outlines travel
//! as fills, and this is where they come from — read once from a SMuFL font,
//! in **staff spaces** (SMuFL's em is four of them), y pointing down the
//! screen, rounded to a thousandth. The face itself never ships.
//!
//! The stem anchors are copied from the font's own metadata
//! (`Bravura.json`, `glyphsWithAnchors`) rather than parsed from it: there
//! are nine, they are what the engraver joins a stem to, and a parser for a
//! file this task reads once would be more code than the numbers.
//!
//! A face that is not SMuFL but names its glyphs — Emmentaler, LilyPond's
//! own, which is what the catalogue ships since it simply looked better
//! beside LilyPond — is read by those names ([`NAMED`]) and written under the
//! SMuFL ones, so the engraver cannot tell. Its time-signature digits stand
//! on their baseline where SMuFL's are centred, so they are moved down a
//! space; and having no anchors, its noteheads get the engraver's: the stem
//! meets the head at its edge, 0.168 of a space off centre.

use std::fmt::Write as _;

/// A named point on a glyph, in staff spaces, y up as SMuFL writes it.
type Anchor = (&'static str, f32, f32);

/// SMuFL name, codepoint, and the stem anchors the engraver needs.
const GLYPHS: &[(&str, char, &[Anchor])] = &[
    ("noteheadWhole", '\u{E0A2}', &[]),
    ("noteheadHalf", '\u{E0A3}', &[("stemUpSE", 1.18, 0.168), ("stemDownNW", 0.0, -0.168)]),
    ("noteheadBlack", '\u{E0A4}', &[("stemUpSE", 1.18, 0.168), ("stemDownNW", 0.0, -0.168)]),
    ("gClef", '\u{E050}', &[]),
    ("cClef", '\u{E05C}', &[]),
    ("fClef", '\u{E062}', &[]),
    ("timeSig0", '\u{E080}', &[]),
    ("timeSig1", '\u{E081}', &[]),
    ("timeSig2", '\u{E082}', &[]),
    ("timeSig3", '\u{E083}', &[]),
    ("timeSig4", '\u{E084}', &[]),
    ("timeSig5", '\u{E085}', &[]),
    ("timeSig6", '\u{E086}', &[]),
    ("timeSig7", '\u{E087}', &[]),
    ("timeSig8", '\u{E088}', &[]),
    ("timeSig9", '\u{E089}', &[]),
    ("timeSigCommon", '\u{E08A}', &[]),
    ("timeSigCutCommon", '\u{E08B}', &[]),
    ("augmentationDot", '\u{E1E7}', &[]),
    ("flag8thUp", '\u{E240}', &[("stemUpNW", 0.0, -0.04)]),
    ("flag8thDown", '\u{E241}', &[("stemDownSW", 0.0, 0.132)]),
    ("flag16thUp", '\u{E242}', &[("stemUpNW", 0.0, -0.088)]),
    ("flag16thDown", '\u{E243}', &[("stemDownSW", 0.0, 0.128)]),
    ("flag32ndUp", '\u{E244}', &[("stemUpNW", 0.0, 0.376)]),
    ("flag32ndDown", '\u{E245}', &[("stemDownSW", 0.0, -0.448)]),
    ("accidentalFlat", '\u{E260}', &[]),
    ("accidentalNatural", '\u{E261}', &[]),
    ("accidentalSharp", '\u{E262}', &[]),
    ("accidentalDoubleSharp", '\u{E263}', &[]),
    ("accidentalDoubleFlat", '\u{E264}', &[]),
    ("restWhole", '\u{E4E3}', &[]),
    ("restHalf", '\u{E4E4}', &[]),
    ("restQuarter", '\u{E4E5}', &[]),
    ("rest8th", '\u{E4E6}', &[]),
    ("rest16th", '\u{E4E7}', &[]),
    ("rest32nd", '\u{E4E8}', &[]),
    // Dynamics and articulations.
    ("dynamicPiano", '\u{E520}', &[]),
    ("dynamicMezzo", '\u{E521}', &[]),
    ("dynamicForte", '\u{E522}', &[]),
    ("dynamicRinforzando", '\u{E523}', &[]),
    ("dynamicSforzando", '\u{E524}', &[]),
    ("dynamicZ", '\u{E525}', &[]),
    ("dynamicNiente", '\u{E526}', &[]),
    ("articStaccatoAbove", '\u{E4A2}', &[]),
    ("articAccentAbove", '\u{E4A0}', &[]),
    ("articTenutoAbove", '\u{E4A4}', &[]),
    ("articMarcatoAbove", '\u{E4AC}', &[]),
    ("articMarcatoBelow", '\u{E4AD}', &[]),
    ("articStaccatissimoAbove", '\u{E4A6}', &[]),
    ("articStaccatissimoBelow", '\u{E4A7}', &[]),
    ("articTenutoStaccatoAbove", '\u{E4B2}', &[]),
    ("articTenutoStaccatoBelow", '\u{E4B3}', &[]),
    ("fermataAbove", '\u{E4C0}', &[]),
    ("fermataBelow", '\u{E4C1}', &[]),
    // The ends of a staff group's bracket.
    ("bracketTop", '\u{E003}', &[]),
    ("bracketBottom", '\u{E004}', &[]),
];

/// The same glyphs in a face that names them (Emmentaler): the SMuFL name,
/// the face's own, and how far the outline moves down, in staff spaces.
const NAMED: &[(&str, &str, f32)] = &[
    ("noteheadWhole", "noteheads.s0", 0.0),
    ("noteheadHalf", "noteheads.s1", 0.0),
    ("noteheadBlack", "noteheads.s2", 0.0),
    ("gClef", "clefs.G", 0.0),
    ("cClef", "clefs.C", 0.0),
    ("fClef", "clefs.F", 0.0),
    ("timeSig0", "zero", 1.0),
    ("timeSig1", "one", 1.0),
    ("timeSig2", "two", 1.0),
    ("timeSig3", "three", 1.0),
    ("timeSig4", "four", 1.0),
    ("timeSig5", "five", 1.0),
    ("timeSig6", "six", 1.0),
    ("timeSig7", "seven", 1.0),
    ("timeSig8", "eight", 1.0),
    ("timeSig9", "nine", 1.0),
    ("timeSigCommon", "timesig.C44", 0.0),
    ("timeSigCutCommon", "timesig.C22", 0.0),
    ("augmentationDot", "dots.dot", 0.0),
    ("flag8thUp", "flags.u3", 0.0),
    ("flag8thDown", "flags.d3", 0.0),
    ("flag16thUp", "flags.u4", 0.0),
    ("flag16thDown", "flags.d4", 0.0),
    ("flag32ndUp", "flags.u5", 0.0),
    ("flag32ndDown", "flags.d5", 0.0),
    ("accidentalFlat", "accidentals.flat", 0.0),
    ("accidentalNatural", "accidentals.natural", 0.0),
    ("accidentalSharp", "accidentals.sharp", 0.0),
    ("accidentalDoubleSharp", "accidentals.doublesharp", 0.0),
    ("accidentalDoubleFlat", "accidentals.flatflat", 0.0),
    ("restWhole", "rests.0", 0.0),
    ("restHalf", "rests.1", 0.0),
    ("restQuarter", "rests.2", 0.0),
    ("rest8th", "rests.3", 0.0),
    ("rest16th", "rests.4", 0.0),
    ("rest32nd", "rests.5", 0.0),
    // Dynamics, the face's own letters, and articulations.
    ("dynamicPiano", "p", 0.0),
    ("dynamicMezzo", "m", 0.0),
    ("dynamicForte", "f", 0.0),
    ("dynamicRinforzando", "r", 0.0),
    ("dynamicSforzando", "s", 0.0),
    ("dynamicZ", "z", 0.0),
    ("dynamicNiente", "n", 0.0),
    ("articStaccatoAbove", "scripts.staccato", 0.0),
    ("articAccentAbove", "scripts.sforzato", 0.0),
    ("articTenutoAbove", "scripts.tenuto", 0.0),
    ("articMarcatoAbove", "scripts.umarcato", 0.0),
    ("articMarcatoBelow", "scripts.dmarcato", 0.0),
    ("articStaccatissimoAbove", "scripts.ustaccatissimo", 0.0),
    ("articStaccatissimoBelow", "scripts.dstaccatissimo", 0.0),
    ("articTenutoStaccatoAbove", "scripts.dportato", 0.0),
    ("articTenutoStaccatoBelow", "scripts.uportato", 0.0),
    ("fermataAbove", "scripts.ufermata", 0.0),
    ("fermataBelow", "scripts.dfermata", 0.0),
    ("bracketTop", "brackettips.up", 0.0),
    ("bracketBottom", "brackettips.down", 0.0),
];

/// Where a named face's flag meets its stem: its origin is a little in from
/// the stem's outer edge.
const NAMED_FLAG_X: f32 = -0.064;

/// How far off centre a stem meets a notehead, for a face with no anchors.
const NAMED_STEM_Y: f32 = 0.168;

/// The outline as canvas fill commands (03 §1.1): `0` move, `1` line, `2`
/// quad, `3` cubic, `4` close — in staff spaces, y down, moved down by
/// `shift`.
struct Fill {
    per_space: f32,
    shift: f32,
    out: Vec<f32>,
}

impl Fill {
    fn pt(&mut self, x: f32, y: f32) {
        self.out.push(x / self.per_space);
        self.out.push(-y / self.per_space + self.shift);
    }
}

impl ttf_parser::OutlineBuilder for Fill {
    fn move_to(&mut self, x: f32, y: f32) {
        self.out.push(0.0);
        self.pt(x, y);
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.out.push(1.0);
        self.pt(x, y);
    }
    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        self.out.push(2.0);
        self.pt(x1, y1);
        self.pt(x, y);
    }
    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        self.out.push(3.0);
        self.pt(x1, y1);
        self.pt(x2, y2);
        self.pt(x, y);
    }
    fn close(&mut self) {
        self.out.push(4.0);
    }
}

/// A number as Soli reads it, to a thousandth of a staff space: an integer
/// prints bare, which is what keeps a command code an `Int` on the wire.
fn num(v: f32) -> String {
    let r = (f64::from(v) * 1000.0).round() / 1000.0;
    if r == r.trunc() {
        format!("{}", r as i64)
    } else {
        format!("{r}")
    }
}

fn list(vs: &[f32]) -> String {
    vs.iter().map(|v| num(*v)).collect::<Vec<_>>().join(", ")
}

/// The raw metrics of a few glyphs, by name: how a face that is not SMuFL
/// is measured before its table is written.
pub(crate) fn probe(path: &str, names: &[String]) -> Result<String, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{path}: {e}"))?;
    let face = ttf_parser::Face::parse(&bytes, 0).map_err(|e| format!("{path}: {e}"))?;
    let mut s = format!("units per em {}\n", face.units_per_em());
    for name in names {
        let Some(id) = face.glyph_index_by_name(name) else {
            let _ = writeln!(s, "{name}: no such glyph");
            continue;
        };
        let bbox = face.glyph_bounding_box(id);
        let advance = face.glyph_hor_advance(id);
        let _ = writeln!(s, "{name}: bbox {bbox:?} advance {advance:?}");
    }
    Ok(s)
}

/// Write the table for the face at `path`, or say why not.
/// The braces kept from a brace face: every tenth of LilyPond's 576, from
/// one about seven spaces tall to one about forty-eight — a piano staff's
/// range; the engraver takes the nearest and stretches it the rest.
const BRACES: std::ops::RangeInclusive<u32> = 10..=45;

pub(crate) fn run(path: &str, licence: Option<&str>, braces: Option<&str>) -> Result<String, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{path}: {e}"))?;
    let face = ttf_parser::Face::parse(&bytes, 0).map_err(|e| format!("{path}: {e}"))?;
    let per_space = f32::from(face.units_per_em()) / 4.0;
    let name = face.names().into_iter().find(|n| n.name_id == ttf_parser::name_id::FULL_NAME && n.is_unicode()).and_then(|n| n.to_string()).unwrap_or_else(|| "the face".to_owned());
    let mut s = String::new();
    let _ = writeln!(s, "# SMuFL glyph outlines for `score()`, in staff spaces, y down.");
    let _ = writeln!(s, "#");
    let _ = writeln!(s, "# GENERATED by `cargo run -p xtask -- smufl <font.otf> <OFL.txt> [<brace.otf>]` from");
    let _ = writeln!(s, "# {name}. Do not edit by hand.");
    let _ = writeln!(s, "#");
    let _ = writeln!(s, "# Each glyph: `fill` (canvas fill commands, 03 §1.1 kind 5, without the");
    let _ = writeln!(s, "# kind and the colour), `advance`, `bbox` [x0, y0, x1, y1], and the stem");
    let _ = writeln!(s, "# anchors the font's metadata gives it.");
    let named = face.glyph_index_by_name("noteheads.s2").is_some();
    if named {
        let _ = writeln!(s, "#");
        let _ = writeln!(s, "# The face is not SMuFL: its glyphs are found by their own names and keyed");
        let _ = writeln!(s, "# here by the SMuFL ones, its time-signature digits are moved to sit");
        let _ = writeln!(s, "# centred on their origin as SMuFL's do, and the stem anchors are the");
        let _ = writeln!(s, "# engraver's, at each notehead's edge.");
    }
    if let Some(licence) = licence {
        let text = std::fs::read_to_string(licence).map_err(|e| format!("{licence}: {e}"))?;
        let _ = writeln!(s, "#");
        let _ = writeln!(s, "# The outlines are the font's, and come under its licence:");
        let _ = writeln!(s, "#");
        for line in text.lines() {
            let line = line.trim_end();
            let _ = writeln!(s, "{}", if line.is_empty() { "#".to_owned() } else { format!("#   {line}") });
        }
    }
    let _ = writeln!(s, "def smufl_glyph_table()");
    let _ = writeln!(s, "  {{");
    let mut rows = Vec::new();
    for (glyph, cp, anchors) in GLYPHS {
        let (id, shift) = if named {
            let &(_, own, shift) = NAMED.iter().find(|entry| entry.0 == *glyph).ok_or_else(|| format!("no name for {glyph}"))?;
            (face.glyph_index_by_name(own).ok_or_else(|| format!("{path}: no glyph named {own}"))?, shift)
        } else {
            (face.glyph_index(*cp).ok_or_else(|| format!("{path}: no glyph for {glyph} (U+{:04X})", u32::from(*cp)))?, 0.0)
        };
        let mut fill = Fill { per_space, shift, out: Vec::new() };
        let bbox = face.outline_glyph(id, &mut fill).ok_or_else(|| format!("{path}: {glyph} has no outline"))?;
        let advance = face.glyph_hor_advance(id).map_or(0.0, |a| f32::from(a) / per_space);
        let bbox = [f32::from(bbox.x_min) / per_space, -f32::from(bbox.y_max) / per_space + shift, f32::from(bbox.x_max) / per_space, -f32::from(bbox.y_min) / per_space + shift];
        let own_anchors: Vec<Anchor> = if !named {
            anchors.to_vec()
        } else if glyph.starts_with("notehead") && !anchors.is_empty() {
            vec![("stemUpSE", advance, NAMED_STEM_Y), ("stemDownNW", 0.0, -NAMED_STEM_Y)]
        } else if glyph.starts_with("flag") {
            anchors.iter().map(|(k, _, _)| (*k, NAMED_FLAG_X, 0.0)).collect()
        } else {
            Vec::new()
        };
        let anchors = own_anchors.iter().map(|(k, x, y)| format!("\"{k}\": [{}, {}]", num(*x), num(-*y))).collect::<Vec<_>>().join(", ");
        rows.push(format!("    \"{glyph}\": {{\"advance\": {}, \"bbox\": [{}], \"anchors\": {{{anchors}}}, \"fill\": [{}]}}", num(advance), list(&bbox), list(&fill.out)));
    }
    // A piano staff's braces, from the face that holds nothing else, keyed
    // `brace<n>` by their place in it.
    if let Some(brace_path) = braces {
        let brace_bytes = std::fs::read(brace_path).map_err(|e| format!("{brace_path}: {e}"))?;
        let brace_face = ttf_parser::Face::parse(&brace_bytes, 0).map_err(|e| format!("{brace_path}: {e}"))?;
        let brace_space = f32::from(brace_face.units_per_em()) / 4.0;
        for tenth in BRACES {
            let own = format!("brace{}", tenth * 10);
            let id = brace_face.glyph_index_by_name(&own).ok_or_else(|| format!("{brace_path}: no glyph named {own}"))?;
            let mut fill = Fill { per_space: brace_space, shift: 0.0, out: Vec::new() };
            let bbox = brace_face.outline_glyph(id, &mut fill).ok_or_else(|| format!("{brace_path}: {own} has no outline"))?;
            let advance = brace_face.glyph_hor_advance(id).map_or(0.0, |a| f32::from(a) / brace_space);
            let bbox = [f32::from(bbox.x_min) / brace_space, -f32::from(bbox.y_max) / brace_space, f32::from(bbox.x_max) / brace_space, -f32::from(bbox.y_min) / brace_space];
            rows.push(format!("    \"{own}\": {{\"advance\": {}, \"bbox\": [{}], \"anchors\": {{}}, \"fill\": [{}]}}", num(advance), list(&bbox), list(&fill.out)));
        }
    }
    let _ = writeln!(s, "{}", rows.join(",\n"));
    let _ = writeln!(s, "  }}");
    let _ = writeln!(s, "end");
    Ok(s)
}

/// The characters a score's titles and tempo marks may use: printable
/// ASCII, the letters French needs, and the typographer's quotes and dashes.
const LETTERS_EXTRA: &str = "àâäçéèêëîïôöùûüÿœæÀÂÄÇÉÈÊËÎÏÔÖÙÛÜŒÆ«»’‘“”–—…°";

/// Every character the regular and the bold faces carry: printable ASCII
/// and the accented letters and punctuation of a French title.
fn text_chars() -> impl Iterator<Item = char> {
    (0x20u8..=0x7e).map(char::from).chain(LETTERS_EXTRA.chars())
}

/// A letter's outline in thousandths of an em, y down, as whole numbers:
/// the file carries no "0." twice a point.
struct LetterFill {
    per_em: f32,
    out: Vec<i64>,
}

/// Rounds half up, as JavaScript's `Math.round` does, so this and the
/// script the table was first made with write the same numbers.
fn round_half_up(v: f32) -> i64 {
    (f64::from(v) + 0.5).floor() as i64
}

impl LetterFill {
    fn pt(&mut self, x: f32, y: f32) {
        self.out.push(round_half_up(x * 1000.0 / self.per_em));
        self.out.push(round_half_up(-y * 1000.0 / self.per_em));
    }
}

impl ttf_parser::OutlineBuilder for LetterFill {
    fn move_to(&mut self, x: f32, y: f32) {
        self.out.push(0);
        self.pt(x, y);
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.out.push(1);
        self.pt(x, y);
    }
    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        self.out.push(2);
        self.pt(x1, y1);
        self.pt(x, y);
    }
    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        self.out.push(3);
        self.pt(x1, y1);
        self.pt(x2, y2);
        self.pt(x, y);
    }
    fn close(&mut self) {
        self.out.push(4);
    }
}

fn letter_rows(path: &str, chars: &mut dyn Iterator<Item = char>) -> Result<Vec<String>, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{path}: {e}"))?;
    let face = ttf_parser::Face::parse(&bytes, 0).map_err(|e| format!("{path}: {e}"))?;
    let per_em = f32::from(face.units_per_em());
    let mut rows = Vec::new();
    for ch in chars {
        let Some(id) = face.glyph_index(ch).filter(|id| id.0 != 0) else { continue };
        let mut fill = LetterFill { per_em, out: Vec::new() };
        // A space has an advance and no outline: kept, with nothing to fill.
        let _ = face.outline_glyph(id, &mut fill);
        let advance = face.glyph_hor_advance(id).map_or(0, |a| round_half_up(f32::from(a) * 1000.0 / per_em));
        let key = match ch {
            '"' => "\\\"".to_owned(),
            '\\' => "\\\\".to_owned(),
            c => c.to_string(),
        };
        let numbers = fill.out.iter().map(i64::to_string).collect::<Vec<_>>().join(",");
        rows.push(format!("      \"{key}\": {{\"a\": {advance}, \"f\": [{numbers}]}}"));
    }
    Ok(rows)
}

/// `cargo run -p xtask -- letters <regular.otf> <bold.otf> <italic.otf> [licence]`:
/// the letters `score()` sets titles and tempo marks in, written as Soli —
/// `letter_glyph_table()`, a regular and a bold face keyed by character,
/// and the italic's digits, which number a tuplet.
pub(crate) fn letters(regular: &str, bold: &str, italic: &str, licence: Option<&str>) -> Result<String, String> {
    let mut s = String::new();
    let _ = writeln!(s, "# Letter outlines for the text `score()` prints: titles, composers, tempo");
    let _ = writeln!(s, "# marks. In thousandths of an em, y down, the baseline at 0; `a` is the");
    let _ = writeln!(s, "# advance and `f` the canvas fill commands (03 §1.1, kind 5, without the");
    let _ = writeln!(s, "# kind and the colour). Whole numbers, so the file carries no \"0.\" twice a point.");
    let _ = writeln!(s, "#");
    let _ = writeln!(s, "# GENERATED by `cargo run -p xtask -- letters <regular.otf> <bold.otf> <italic.otf> <licence>`");
    let _ = writeln!(s, "# from TeX Gyre Schola, the face LilyPond 2.24 sets its text in. Do not");
    let _ = writeln!(s, "# edit by hand. This is a derived work of that font: a subset of its");
    let _ = writeln!(s, "# letters, re-encoded, under a name of its own as the GUST Font License");
    let _ = writeln!(s, "# asks of derived works.");
    if let Some(licence) = licence {
        let text = std::fs::read_to_string(licence).map_err(|e| format!("{licence}: {e}"))?;
        let _ = writeln!(s, "#");
        let _ = writeln!(s, "# The outlines are the font's, and come under its licence:");
        let _ = writeln!(s, "#");
        for line in text.lines() {
            let line = line.trim_end();
            let line = line.strip_prefix('%').map_or(line, |rest| rest.strip_prefix(' ').unwrap_or(rest));
            let _ = writeln!(s, "{}", if line.is_empty() { "#".to_owned() } else { format!("#   {line}") });
        }
    }
    let _ = writeln!(s, "def letter_glyph_table()");
    let _ = writeln!(s, "  {{");
    let _ = writeln!(s, "    \"regular\": {{");
    let _ = writeln!(s, "{}", letter_rows(regular, &mut text_chars())?.join(",\n"));
    let _ = writeln!(s, "    }},");
    let _ = writeln!(s, "    \"bold\": {{");
    let _ = writeln!(s, "{}", letter_rows(bold, &mut text_chars())?.join(",\n"));
    let _ = writeln!(s, "    }},");
    let _ = writeln!(s, "    \"italic\": {{");
    let _ = writeln!(s, "{}", letter_rows(italic, &mut "0123456789".chars())?.join(",\n"));
    let _ = writeln!(s, "    }}");
    let _ = writeln!(s, "  }}");
    let _ = writeln!(s, "end");
    Ok(s)
}
