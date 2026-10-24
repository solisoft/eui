//! The pages as a PDF (`eui_builders_score.sl`, "PDF"): the paths the
//! screen draws, as vector paths at five points a staff space — LilyPond's
//! 20-point staff — so nothing is scaled, and on every page a footer in
//! grey: the title, and the page's number out of how many, or the footer
//! and the header `\paper` asks for; the copyright at the foot of the first
//! page. ASCII from end to end.
//!
//! Every word drawn is also written as text under its outline, in the
//! invisible rendering mode (3 Tr): a reader selects, copies and searches
//! the title, the lyrics and the marks while the page shows the outlines
//! alone. The text's fonts are named, not embedded — nothing of them is
//! painted — each letter as wide as the outline's, and a ToUnicode map
//! says which character each is.

use std::collections::HashMap;

use crate::draw::{cut_to, foot_paths, print_paths, said, words_paths, Path, Said};
use crate::engrave::Engraved;
use crate::glyphs::{letters, Weight};
use crate::parse::{PageCond, PageLine, PagePart, PaperSpec};
use crate::text::{em, text_width};
use crate::Paper;

/// What a page's headers and footers are made from.
pub(crate) struct Pages<'a> {
    /// The header's fields.
    pub header: &'a HashMap<String, String>,
    /// What `\paper` says of the headers and footers.
    pub spec: &'a PaperSpec,
    /// The words under the outlines.
    pub text: bool,
}

/// The file, from an engraving laid out on `paper` (`Paper::layout`).
pub(crate) fn pdf(eng: &Engraved, title: &str, paper: &Paper, pages: &Pages) -> String {
    let sp = paper.pt;
    let page_w = paper.w * sp;
    let page_h = paper.h * sp;
    let count = eng.pages.len();
    // Each page's drawing, and the words on it.
    let mut drawn: Vec<(String, Vec<Said>)> = Vec::new();
    for pg in &eng.pages {
        let k = pg.index;
        let ((ink, grey), words) = said(|| page_ink(eng, sp, k, count, title, paper, pages));
        let data = format!("0 g\n{}\n0.4 g\n{}", page(&ink, page_h), page(&grey, page_h));
        drawn.push((data, if pages.text { words } else { Vec::new() }));
    }
    let fonts = Fonts::of(drawn.iter().flat_map(|(_, w)| w.iter()));
    let mut bodies: Vec<String> = Vec::new();
    bodies.push("<< /Type /Catalog /Pages 2 0 R >>".into());
    let kids: Vec<String> = (0..count).map(|k| format!("{} 0 R", 4 + 2 * k)).collect();
    bodies.push(format!("<< /Type /Pages /Kids [{}] /Count {count} >>", kids.join(" ")));
    bodies.push(format!("<< /Title {} /Producer (EUI score) >>", text(title)));
    // The fonts' objects come after the pages'.
    let first_font = 4 + 2 * count;
    let resources = fonts.resources(first_font);
    for (k, (data, words)) in drawn.iter().enumerate() {
        let data = if words.is_empty() { data.clone() } else { format!("{data}\n{}", fonts.layer(words, page_h)) };
        bodies.push(format!("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {} {}] /Contents {} 0 R /Resources {resources} >>", num(page_w), num(page_h), 5 + 2 * k));
        bodies.push(format!("<< /Length {} >>\nstream\n{data}\nendstream", data.len()));
    }
    bodies.extend(fonts.objects(first_font));
    let mut out = String::from("%PDF-1.4\n");
    let mut offsets = Vec::new();
    for (k, body) in bodies.iter().enumerate() {
        offsets.push(out.len());
        out.push_str(&format!("{} 0 obj\n{body}\nendobj\n", k + 1));
    }
    let at = out.len();
    out.push_str(&format!("xref\n0 {}\n0000000000 65535 f \n", bodies.len() + 1));
    for off in offsets {
        out.push_str(&format!("{off:010} 00000 n \n"));
    }
    out.push_str(&format!("trailer\n<< /Size {} /Root 1 0 R /Info 3 0 R >>\nstartxref\n{at}\n%%EOF\n", bodies.len() + 1));
    out
}

/// Page `k`'s paths in black — its titles, its music, the copyright — and in
/// grey: its header and its footer.
fn page_ink(eng: &Engraved, sp: f64, k: usize, count: usize, title: &str, paper: &Paper, pages: &Pages) -> (Vec<Path>, Vec<Path>) {
    let mut ink = print_paths(eng, sp, k);
    let mut grey = Vec::new();
    let spec = pages.spec;
    let odd = k.is_multiple_of(2);
    let e = em(-2.0);
    let foot_y = paper.h - paper.foot();
    let say = |parts: &[PagePart]| words_of(parts, pages.header, title, k, count);
    let footer = if odd { spec.odd_footer.as_ref() } else { spec.even_footer.as_ref().or(spec.odd_footer.as_ref()) };
    match footer {
        Some(line) => grey.extend(line_paths(eng, sp, line, foot_y, e, &say)),
        None => grey.extend(foot_paths(eng, sp, k, count, title, foot_y)),
    }
    let header = if odd { spec.odd_header.as_ref() } else { spec.even_header.as_ref().or(spec.odd_header.as_ref()) };
    if let Some(line) = header {
        // Under the top margin, where a screen's pages carry their number.
        let y = paper.top - paper.head_room + 0.72 * e;
        grey.extend(line_paths(eng, sp, line, y, e, &say));
    }
    // The copyright at the foot of the first page, over the footer, centred
    // (LilyPond's `oddFooterMarkup` on its first page) — unless the footer
    // already says it.
    let copyright = pages.header.get("copyright").map_or("", |c| c.trim());
    let in_footer = footer.is_some_and(|line| [&line.left, &line.centre, &line.right].iter().any(|parts| names_field(parts, "copyright")));
    if k == 0 && !copyright.is_empty() && !in_footer {
        let words = cut_to(copyright, eng.width, e);
        let w = text_width(&words, Weight::Regular, e);
        ink.extend(words_paths(sp, &words, eng.left + (eng.width - w) / 2.0, foot_y - 1.1 * e, e));
    }
    (ink, grey)
}

/// Whether `parts` name the header's field `name`.
fn names_field(parts: &[PagePart], name: &str) -> bool {
    parts.iter().any(|p| match p {
        PagePart::Field(f) => f == name,
        PagePart::Only(_, inner) => names_field(inner, name),
        _ => false,
    })
}

/// What `parts` say on page `k` (from 0) of `count`: a field of the header
/// (the title the PDF is given where the header has none), the page's
/// number, the words.
pub(crate) fn words_of(parts: &[PagePart], header: &HashMap<String, String>, title: &str, k: usize, count: usize) -> String {
    let mut out = String::new();
    for p in parts {
        match p {
            PagePart::Text(t) => out.push_str(t),
            PagePart::Field(f) => match header.get(f) {
                Some(v) => out.push_str(v),
                None if f == "title" => out.push_str(title),
                None => {}
            },
            PagePart::Page => out.push_str(&(k + 1).to_string()),
            PagePart::Only(cond, inner) => {
                let on = match cond {
                    PageCond::First => k == 0,
                    PageCond::NotFirst => k > 0,
                    PageCond::Last => k + 1 == count,
                    PageCond::NotLast => k + 1 < count,
                };
                if on {
                    out.push_str(&words_of(inner, header, title, k, count));
                }
            }
        }
    }
    out
}

/// A header's or a footer's line on the baseline `y`: its left words at the
/// staves' left, its middle ones centred, its right ones ending where the
/// staves end.
fn line_paths(eng: &Engraved, sp: f64, line: &PageLine, y: f64, e: f64, say: &dyn Fn(&[PagePart]) -> String) -> Vec<Path> {
    let mut paths = Vec::new();
    let left = say(&line.left);
    let centre = say(&line.centre);
    let right = say(&line.right);
    let (left, centre, right) = (left.trim(), centre.trim(), right.trim());
    if !left.is_empty() {
        paths.extend(words_paths(sp, left, eng.left, y, e));
    }
    if !centre.is_empty() {
        let w = text_width(centre, Weight::Regular, e);
        paths.extend(words_paths(sp, centre, eng.left + (eng.width - w) / 2.0, y, e));
    }
    if !right.is_empty() {
        let w = text_width(right, Weight::Regular, e);
        paths.extend(words_paths(sp, right, eng.left + eng.width - w, y, e));
    }
    paths
}

/// The fonts the words are written in: one a face used, every character
/// numbered once for them all (CID 1, 2…, in the order first written).
struct Fonts {
    /// Each character's number.
    cids: HashMap<char, u16>,
    /// The characters, by number less one.
    chars: Vec<char>,
    /// The faces used, in Regular, Bold, Italic order.
    faces: Vec<Weight>,
}

const FACES: [(Weight, &str); 3] = [(Weight::Regular, "TeXGyreSchola-Regular"), (Weight::Bold, "TeXGyreSchola-Bold"), (Weight::Italic, "TeXGyreSchola-Italic")];

impl Fonts {
    fn of<'a>(words: impl Iterator<Item = &'a Said>) -> Self {
        let mut fonts = Fonts { cids: HashMap::new(), chars: Vec::new(), faces: Vec::new() };
        let mut used = [false; 3];
        for w in words {
            used[face_index(w.weight)] = true;
            for ch in w.text.chars() {
                if !fonts.cids.contains_key(&ch) && fonts.chars.len() < 0xFFFE {
                    fonts.chars.push(ch);
                    fonts.cids.insert(ch, fonts.chars.len() as u16);
                }
            }
        }
        fonts.faces = FACES.iter().enumerate().filter(|(k, _)| used[*k]).map(|(_, f)| f.0).collect();
        fonts
    }

    /// The object number of face `k`'s Type 0 font, the map being `first`.
    fn font_object(first: usize, k: usize) -> usize {
        first + 1 + 3 * k
    }

    /// A page's `/Resources`: `/F1` regular, `/F2` bold, `/F3` italic, those used.
    fn resources(&self, first: usize) -> String {
        if self.faces.is_empty() {
            return "<< >>".into();
        }
        let named: Vec<String> = self.faces.iter().enumerate().map(|(k, w)| format!("/F{} {} 0 R", face_index(*w) + 1, Fonts::font_object(first, k))).collect();
        format!("<< /Font << {} >> >>", named.join(" "))
    }

    /// The words of a page, invisible, each run where its outline is.
    fn layer(&self, words: &[Said], page_h: f64) -> String {
        let mut out = String::from("BT 3 Tr");
        for w in words {
            let mut hex = String::with_capacity(w.text.len() * 4);
            for ch in w.text.chars() {
                if let Some(cid) = self.cids.get(&ch) {
                    hex.push_str(&format!("{cid:04X}"));
                }
            }
            out.push_str(&format!("\n/F{} {} Tf 1 0 0 1 {} {} Tm <{hex}> Tj", face_index(w.weight) + 1, num(w.size), num(w.x), num(page_h - w.y)));
        }
        out.push_str("\nET");
        out
    }

    /// The objects from `first`: the characters' map to Unicode, then a
    /// Type 0 font, its CID font and its descriptor a face.
    fn objects(&self, first: usize) -> Vec<String> {
        if self.faces.is_empty() {
            return Vec::new();
        }
        let mut out = vec![self.to_unicode()];
        for (k, weight) in self.faces.iter().enumerate() {
            let name = FACES[face_index(*weight)].1;
            let at = Fonts::font_object(first, k);
            out.push(format!("<< /Type /Font /Subtype /Type0 /BaseFont /{name} /Encoding /Identity-H /DescendantFonts [{} 0 R] /ToUnicode {first} 0 R >>", at + 1));
            let face = letters().face(*weight);
            let widths: Vec<String> = self.chars.iter().map(|ch| num(face.get(ch).map_or(250.0, |l| l.a))).collect();
            out.push(format!(
                "<< /Type /Font /Subtype /CIDFontType0 /BaseFont /{name} /CIDSystemInfo << /Registry (Adobe) /Ordering (Identity) /Supplement 0 >> /FontDescriptor {} 0 R /DW 250 /W [1 [{}]] >>",
                at + 2,
                widths.join(" ")
            ));
            let (flags, angle) = if *weight == Weight::Italic { (98, -12) } else { (34, 0) };
            out.push(format!("<< /Type /FontDescriptor /FontName /{name} /Flags {flags} /FontBBox [-200 -300 1200 1000] /ItalicAngle {angle} /Ascent 900 /Descent -300 /CapHeight 700 /StemV 80 >>"));
        }
        out
    }

    /// The ToUnicode CMap: each number to its character, a hundred a block.
    fn to_unicode(&self) -> String {
        let mut map = String::from("/CIDInit /ProcSet findresource begin\n12 dict begin\nbegincmap\n");
        map.push_str("/CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def\n/CMapName /Adobe-Identity-UCS def\n/CMapType 2 def\n");
        map.push_str("1 begincodespacerange\n<0000> <FFFF>\nendcodespacerange\n");
        for (block, chars) in self.chars.chunks(100).enumerate() {
            map.push_str(&format!("{} beginbfchar\n", chars.len()));
            for (k, ch) in chars.iter().enumerate() {
                let mut units = String::new();
                for u in ch.encode_utf16(&mut [0; 2]) {
                    units.push_str(&format!("{u:04X}"));
                }
                map.push_str(&format!("<{:04X}> <{units}>\n", block * 100 + k + 1));
            }
            map.push_str("endbfchar\n");
        }
        map.push_str("endcmap\nCMapName currentdict /CMap defineresource pop\nend\nend");
        format!("<< /Length {} >>\nstream\n{map}\nendstream", map.len())
    }
}

fn face_index(w: Weight) -> usize {
    match w {
        Weight::Regular => 0,
        Weight::Bold => 1,
        Weight::Italic => 2,
    }
}

/// Paths as content-stream operators: each filled in the current colour
/// by the non-zero rule, y turned up as PDF has it.
fn page(paths: &[Path], page_h: f64) -> String {
    let mut ops: Vec<String> = Vec::new();
    for pa in paths {
        let pa = match pa {
            Path::Rect([x, y, w, h]) => {
                ops.push(format!("{} {} {} {} re f", num(*x), num(page_h - y - h), num(*w), num(*h)));
                continue;
            }
            Path::Fill(cmds) => cmds,
        };
        let at = |k: usize| pa.get(k).copied().unwrap_or(0.0);
        let (mut cx, mut cy, mut sx, mut sy) = (0.0, 0.0, 0.0, 0.0);
        let mut k = 0;
        while k < pa.len() {
            let c = pa[k] as i64;
            match c {
                0 | 1 => {
                    cx = at(k + 1);
                    cy = at(k + 2);
                    if c == 0 {
                        sx = cx;
                        sy = cy;
                    }
                    ops.push(format!("{} {} {}", num(cx), num(page_h - cy), if c == 0 { "m" } else { "l" }));
                    k += 3;
                }
                2 => {
                    let (qx, qy, ex, ey) = (at(k + 1), at(k + 2), at(k + 3), at(k + 4));
                    let pts = [cx + (qx - cx) * 2.0 / 3.0, cy + (qy - cy) * 2.0 / 3.0, ex + (qx - ex) * 2.0 / 3.0, ey + (qy - ey) * 2.0 / 3.0, ex, ey];
                    let parts: Vec<String> = (0..3).map(|j| format!("{} {}", num(pts[2 * j]), num(page_h - pts[2 * j + 1]))).collect();
                    ops.push(format!("{} c", parts.join(" ")));
                    cx = ex;
                    cy = ey;
                    k += 5;
                }
                3 => {
                    let parts: Vec<String> = (0..3).map(|j| format!("{} {}", num(at(k + 1 + 2 * j)), num(page_h - at(k + 2 + 2 * j)))).collect();
                    ops.push(format!("{} c", parts.join(" ")));
                    cx = at(k + 5);
                    cy = at(k + 6);
                    k += 7;
                }
                _ => {
                    ops.push("h".into());
                    cx = sx;
                    cy = sy;
                    k += 1;
                }
            }
        }
        ops.push("f".into());
    }
    ops.join("\n")
}

/// A number as PDF writes one: two decimals at most.
fn num(v: f64) -> String {
    let r = (v * 100.0 + 0.5).floor() / 100.0;
    if r == r.floor() {
        format!("{}", r as i64)
    } else {
        format!("{r}")
    }
}

/// Text for the document's information: UTF-16, as a hex string.
fn text(words: &str) -> String {
    let mut out = String::from("<FEFF");
    for u in words.encode_utf16() {
        out.push_str(&format!("{u:04X}"));
    }
    out.push('>');
    out
}
