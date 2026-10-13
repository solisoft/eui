//! The A4 pages as a PDF (`eui_builders_score.sl`, "PDF"): the paths the
//! screen draws, as vector paths at five points a staff space — LilyPond's
//! 20-point staff — so nothing is scaled and no font is embedded, and on
//! every page a footer in grey: the title, and the page's number out of
//! how many. ASCII from end to end.

use crate::draw::{foot_paths, print_paths, Path};
use crate::engrave::Engraved;
use crate::Paper;

/// The file, from an engraving laid out on `paper` (`Paper::layout`).
pub(crate) fn pdf(eng: &Engraved, title: &str, paper: &Paper) -> String {
    let sp = 5.0;
    let page_w = paper.w * sp;
    let page_h = paper.h * sp;
    let mut bodies: Vec<String> = Vec::new();
    bodies.push("<< /Type /Catalog /Pages 2 0 R >>".into());
    let count = eng.pages.len();
    let kids: Vec<String> = (0..count).map(|k| format!("{} 0 R", 4 + 2 * k)).collect();
    bodies.push(format!("<< /Type /Pages /Kids [{}] /Count {count} >>", kids.join(" ")));
    bodies.push(format!("<< /Title {} /Producer (EUI score) >>", text(title)));
    for pg in &eng.pages {
        let k = pg.index;
        let foot = foot_paths(eng, sp, k, count, title, paper.h - paper.foot());
        let data = format!("0 g\n{}\n0.4 g\n{}", page(&print_paths(eng, sp, k), page_h), page(&foot, page_h));
        bodies.push(format!("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {} {}] /Contents {} 0 R /Resources << >> >>", num(page_w), num(page_h), 5 + 2 * k));
        bodies.push(format!("<< /Length {} >>\nstream\n{data}\nendstream", data.len()));
    }
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
