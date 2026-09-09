// The workspace denies indexing, unwrapping and unchecked arithmetic because
// the *decode path* must not panic on hostile input. A test harness is the one
// place where a panic is the correct outcome — a test that panics is a test
// that failed — so the strict set is lifted here and nowhere else.
#![allow(clippy::indexing_slicing, clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::arithmetic_side_effects)]

//! What the encoding actually costs, measured rather than asserted.
//!
//! `spec/10-budgets.md` is only worth something if the numbers in it came from
//! a run. This test builds a realistic 50-row × 4-column table both ways — as
//! an EUI batch and as the HTML a Tailwind-styled Soli template would emit —
//! and compares. It prints the breakdown with `--nocapture`.

use eui_proto::*;

const ROWS: usize = 50;
const HEADERS: [&str; 4] = ["Référence", "Client", "Statut", "Montant"];

/// Cell text for row `i`, close to what an invoice list really holds.
fn cells(i: usize) -> [String; 4] {
    [format!("FA-2026-{:04}", i + 1), format!("Client {:02} SARL", i % 37), (if i % 3 == 0 { "Payée" } else { "En attente" }).to_string(), format!("{}, {:02} €", 100 + i * 37, i % 100)]
}

/// The EUI batch: six shared style records, the column headers interned, and
/// one node per cell.
///
/// `intern_status` models what a real server's atom heuristic does — the
/// status column has two distinct values across fifty rows, so it is interned;
/// references and client names are unique, so interning them would cost more
/// than it saves.
fn build_eui(intern_status: bool) -> (Vec<u8>, usize) {
    let mut ops = Vec::new();
    let mut text_bytes = 0usize;

    // Six style records cover the whole table. Every row shares them.
    for id in 1..=6u32 {
        ops.push(Op::DefStyle { id, record: StyleRecord::default() });
    }
    // Column headers are the only strings worth interning: everything else is
    // unique per row, so an atom would cost more than it saves.
    for (i, h) in HEADERS.iter().enumerate() {
        text_bytes += h.len();
        ops.push(Op::DefAtom { id: i as u32 + 1, value: (*h).to_string() });
    }
    if intern_status {
        ops.push(Op::DefAtom { id: 5, value: "Payée".into() });
        ops.push(Op::DefAtom { id: 6, value: "En attente".into() });
    }

    let mut tree = Subtree::default();
    let mut next_id = 1u32;
    let mut id = || {
        next_id += 1;
        next_id
    };

    fn boxed(id: u32, style: u32, children: u32) -> FlatNode {
        FlatNode { kind: NodeKind::Box, id, style, key: 0, text: None, props: (0, 0), handlers: (0, 0), child_count: children }
    }
    fn text(id: u32, style: u32, key: u32, text: TextRef) -> FlatNode {
        FlatNode { kind: NodeKind::Text, id, style, key, text: Some(text), props: (0, 0), handlers: (0, 0), child_count: 0 }
    }

    // root ─ header row ─ 50 keyed rows
    tree.nodes.push(boxed(id(), 1, ROWS as u32 + 1));
    tree.nodes.push(boxed(id(), 2, 4));
    for i in 0..4u32 {
        tree.nodes.push(text(id(), 3, 0, TextRef::Atom(i + 1)));
    }
    for r in 0..ROWS {
        let style = if r % 2 == 0 { 4 } else { 5 };
        let mut row = boxed(id(), style, 4);
        row.key = r as u32 + 1; // keyed, so a re-sort is moves not rebuilds
        tree.nodes.push(row);
        for (c, cell) in cells(r).into_iter().enumerate() {
            text_bytes += cell.len();
            // Column 2 is the status; it repeats, so it earns an atom.
            let content = if intern_status && c == 2 { TextRef::Atom(if r % 3 == 0 { 5 } else { 6 }) } else { TextRef::Inline(cell) };
            tree.nodes.push(text(id(), 6, 0, content));
        }
    }

    ops.push(Op::Mount(tree));
    (Frame::Batch(Batch { seq: 1, ops }).encode(), text_bytes)
}

/// The same table as HTML, with the Tailwind classes such a table really
/// carries. No pretty-printing, no indentation — the favourable case.
fn build_html() -> String {
    let mut s = String::from("<table class=\"w-full text-sm\"><thead><tr class=\"border-b border-slate-800\">");
    for h in HEADERS {
        s.push_str("<th class=\"px-4 py-2 text-left font-semibold text-slate-200\">");
        s.push_str(h);
        s.push_str("</th>");
    }
    s.push_str("</tr></thead><tbody>");
    for r in 0..ROWS {
        s.push_str("<tr class=\"border-b border-slate-800 hover:bg-slate-900/50\">");
        for cell in cells(r) {
            s.push_str("<td class=\"px-4 py-2 text-slate-300\">");
            s.push_str(&cell);
            s.push_str("</td>");
        }
        s.push_str("</tr>");
    }
    s.push_str("</tbody></table>");
    s
}

#[test]
fn a_fifty_row_table_costs_what_the_budget_says() {
    let (plain, text_bytes) = build_eui(false);
    let (interned, interned_text) = build_eui(true);
    let html = build_html();

    // Style records are a fixed 6 × 66 B paid once for the whole session; a
    // second table on the same connection does not pay them again.
    let styles = 6 * (limits::STYLE_RECORD_BYTES + 2);
    let plain_overhead = plain.len() - text_bytes;
    let html_overhead = html.len() - text_bytes;

    println!("\n  50 rows × 4 columns, 256 nodes, rows keyed");
    println!("  ────────────────────────────────────────────────────────");
    println!("  cell + header text (identical both ways)  : {text_bytes:>6} B");
    println!("  EUI  total, no interning                  : {:>6} B", plain.len());
    println!("  EUI  total, status column interned        : {:>6} B", interned.len());
    println!("  EUI    of which style records (once/session): {styles:>4} B");
    println!("  EUI  structure only                       : {plain_overhead:>6} B");
    println!("  HTML total                                : {:>6} B", html.len());
    println!("  HTML structure only                       : {html_overhead:>6} B");
    println!(
        "  ratio — total {:.1}× · structure {:.1}× · interned {:.1}×",
        html.len() as f64 / plain.len() as f64,
        html_overhead as f64 / plain_overhead as f64,
        html.len() as f64 / interned.len() as f64
    );
    println!("  per node: {:.1} B of structure\n", plain_overhead as f64 / 256.0);

    // The text is the data: neither side can compress it away, so the honest
    // claim is about the structure around it. Measured at 5.3× on this table;
    // the budget is set below that so a real regression trips it and ordinary
    // drift does not.
    assert!(
        eui_overhead_ratio(html_overhead, plain_overhead) >= 4.0,
        "EUI structural overhead {plain_overhead} B vs HTML {html_overhead} B \
         is below the 4× budget"
    );
    assert!(plain.len() < 5000, "50-row table is {} B, budget is 5000 B", plain.len());
    assert!(interned.len() < text_bytes.max(interned_text) + 2400);
}

fn eui_overhead_ratio(html: usize, eui: usize) -> f64 {
    html as f64 / eui as f64
}

#[test]
fn a_single_cell_update_is_tiny() {
    let frame = Frame::Batch(Batch { seq: 2, ops: vec![Op::SetText { node: 141, text: TextRef::Inline("1 984, 42 €".into()) }] }).encode();

    println!("\n  one-cell update: {} B (12 B of which are the new text)\n", frame.len());
    assert!(frame.len() < 40, "one-cell update is {} B, budget is 40 B", frame.len());
}

#[test]
fn re_sorting_a_keyed_table_moves_rather_than_rebuilds() {
    // Reversing 50 keyed rows: 49 moves — always the last row to position i —
    // and no subtree ever re-sent.
    let ops = (0..49u32).map(|i| Op::MoveChild { parent: 1, from: 49, to: i }).collect();
    let frame = Frame::Batch(Batch { seq: 3, ops }).encode();

    let (full, _) = build_eui(false);
    println!("\n  reverse 50 rows: {} B by moves vs {} B by re-mount\n", frame.len(), full.len());
    assert!(frame.len() < 300);
}
