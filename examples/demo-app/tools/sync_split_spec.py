#!/usr/bin/env python3
"""Copy functions out of the catalogue and into the specs.

The catalogue is loaded by the server, not by a bare script, so a spec
cannot import it. Until it is a package the definitions are copied in, and
copies drift -- so the copying is a command and CI can check that running
it changes nothing.

    python3 tools/sync_split_spec.py          # rewrite the specs
    python3 tools/sync_split_spec.py --check  # exit 1 if they are stale

The catalogue is several files that load into one namespace, so which one
a function came from is not something a spec should have to know: they are
read in the order named and searched as one text.
"""

import pathlib
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
CONTROLLERS = ROOT / "app" / "controllers"
SOURCES = [
    CONTROLLERS / "eui_builders.sl",
    CONTROLLERS / "eui_builders_forms.sl",
    CONTROLLERS / "eui_builders_charts.sl",
    CONTROLLERS / "eui_builders_feed.sl",
    CONTROLLERS / "eui_builders_markdown.sl",
    CONTROLLERS / "eui_builders_tw.sl",
]

# `tw()` and everything under it. `node()` calls `tw_node`, so a spec that
# copies `node` copies these too, and declares the `TW_MEMO` constant above
# its marker: the copy carries definitions, never a module's constants.
TW_DEFS = [
    "tw_split",
    "tw",
    "tw_raw",
    "tw_copy",
    "tw_blank",
    "tw_screen",
    "tw_screen_rank",
    "tw_screen_px",
    "tw_responsive?",
    "tw_word",
    "tw_focus_ring?",
    "tw_needs_width",
    "tw_parse",
    "tw_style",
    "tw_stateful?",
    "tw_node",
    "tw_wire",
    "tw_no",
    "tw_unknown",
    "tw_variant",
    "tw_refused",
    "tw_refused_exact",
    "tw_space_steps",
    "tw_text_sizes",
    "tw_max_widths",
    "tw_space",
    "tw_space_near",
    "tw_numeric?",
    "tw_bracket",
    "tw_length",
    "tw_roles",
    "tw_role_alias",
    "tw_neutral?",
    "tw_status",
    "tw_nearest",
    "tw_hex2",
    "tw_hex?",
    "tw_alpha",
    "tw_colour",
    "tw_gray",
    "tw_exact",
    "tw_sides",
    "tw_edge",
    "tw_edges",
    "tw_border_width",
    "tw_class",
    "tw_family",
    "tw_take",
    "tw_divider",
    "tw_gap_settle",
    "tw_divide",
    "tw_divide_kid",
    "tw_divide_handler",
    "tw_divide_style",
    "tw_case_class",
    "tw_case_apply",
    "tw_text",
    "tw_examples",
]
MARKER = "# ---- copied from the catalogue, do not edit ----"
# Every marker this file has ever written. The cut has to find the old one
# too, or a rename leaves the previous marker and its copies sitting above
# the new ones and the spec defines everything twice.
MARKER_PREFIX = "# ---- copied from "

SPECS = {
    ROOT / "tests" / "split_spec.sl": ["split_span", "split_sizes", "split_at"],
    ROOT / "tests" / "selection_spec.sl": [
        "selection",
        "selection_scope",
        "selection_all",
        "selection_none",
        "selection_scoped",
        "selection_ids_of",
        "selection_all?",
        "selection_has?",
        "selection_toggle",
        "selection_count",
        "selection_empty?",
        "selection_mark",
        "selection_index",
        "selection_in?",
        "selection_ids",
    ],
    ROOT / "tests" / "vu_spec.sl": [
        "node",
        "column",
        "row",
        "text",
        *TW_DEFS,
        "vu_zone",
        "vu_segment",
        "vu_strip",
        "vu_scale",
        "vu_meter",
    ],
    ROOT / "tests" / "tag_spec.sl": [
        "tag_add",
        "tag_remove",
        "tag_suggest",
        "tag_highlight",
    ],
    ROOT / "tests" / "combo_spec.sl": [
        "combo_filter",
        "command_row",
        "command_match",
        "tag_highlight",
    ],
    ROOT / "tests" / "otp_spec.sl": [
        "otp_clean",
        "otp_take",
        "otp_pop",
        "otp_jump",
        "otp_apply",
    ],
    ROOT / "tests" / "catalogue_spec.sl": [
        "filter_at",
        "filter_edit",
        "filter_drop",
        "filter_blank",
        "filter_group_blank",
        "filter_group?",
        "track_part",
        "track_thumb",
        "slider",
        "range_slider",
        "diff_tally",
        "editable",
        "editable_key",
        "editable_states",
        "input",
        "field_props",
    ],
    ROOT / "tests" / "stream_spec.sl": [
        "chart_stream_ceiling",
        "chart_band_role",
    ],
    ROOT / "tests" / "markdown_editor_spec.sl": [
        "md_find",
        "md_starts",
        "md_ends",
        "md_int",
        "md_src",
        "md_target",
        "md_fit",
        "md_image_of",
        "md_file_src?",
        "md_file_of",
        "md_media?",
        "md_ordered_marker",
        "md_edit_mark",
        "md_edit_kind_of",
        "md_edit_bare",
        "md_edit_text?",
        "md_edit_verbatim?",
        "md_edit_read",
        "md_edit_parse",
        "md_edit_title",
        "md_edit_line",
        "md_edit_source",
        "md_edit_index",
        "md_edit_at",
        "md_edit_fresh",
        "md_edit_step",
        "md_edit_splice",
        "md_edit_retype",
        "md_edit_set",
        "md_edit_split",
        "md_edit_merge",
        "md_edit_kind",
        "md_edit_wrap",
        "md_edit_done?",
        "md_edit_grid?",
        "md_edit_ruler?",
        "md_edit_dashes?",
        "md_table_cells",
        "md_edit_grid",
        "md_edit_grid_row",
        "md_edit_grid_rule",
        "md_edit_check",
        "md_edit_cell",
        "md_edit_cols",
        "md_edit_row_add",
        "md_edit_col_add",
        "md_edit_table",
        "md_edit_move_to",
        "md_edit_shift",
        "md_edit_remember",
        "md_edit_undo",
        "md_edit_redo",
        "md_edit_accel?",
        "md_edit_tools",
        "md_edit_slash_hits",
        "md_edit_walk",
        "md_edit_unslash",
        "md_edit_pair_wrap",
        "md_edit_step_tool",
        "md_edit_step_change",
        "md_edit_step_key",
        "md_edit_step_pick",
        "md_edit_step_slash",
        "md_edit_step_link",
        "md_edit_put",
        "md_edit_drop",
        "md_edit_split_focus",
        "md_edit_merge_focus",
    ],
    ROOT / "tests" / "tw_spec.sl": [
        "node",
        "column",
        "row",
        "text",
        *TW_DEFS,
    ],
    ROOT / "tests" / "drag_spec.sl": [
        "split_span",
        "split_sizes",
        "split_at",
        "split_drag",
        "split_keys",
        "split_event",
    ],
}


def grab(src: str, name: str) -> str:
    start = src.index(f"def {name}(")
    end = src.index("\nend\n", start) + len("\nend\n")
    return src[start:end]


def catalogue() -> str:
    """Every catalogue file as one text, which is how the server sees them."""
    missing = [s for s in SOURCES if not s.exists()]
    if missing:
        raise SystemExit("no catalogue at: " + ", ".join(str(m) for m in missing))
    return "\n".join(s.read_text() for s in SOURCES)


def rebuild(spec: pathlib.Path, names: list[str], src: str) -> str:
    text = spec.read_text()
    body = text[text.index("\ndef check(") :]
    head = text[: text.index("\ndef check(")]
    # Cut at whichever comes first, a marker or the first definition.
    # Taking the marker alone would preserve a stale copy sitting above it,
    # which is exactly what a file assembled before the marker existed has.
    cuts = [i for i in (head.find(MARKER_PREFIX), head.find("\ndef ")) if i != -1]
    cut = min(cuts) if cuts else len(head)
    head = head[:cut].rstrip("\n")
    copied = "\n".join(grab(src, n) for n in names)
    return f"{head}\n\n{MARKER}\n\n{copied}{body}"


def main() -> int:
    src = catalogue()
    stale = []
    for spec, names in SPECS.items():
        want = rebuild(spec, names, src)
        if spec.read_text() == want:
            continue
        if "--check" in sys.argv:
            stale.append(spec.name)
        else:
            spec.write_text(want)
            print(f"synced {spec.name}")
    if stale:
        print("stale, run tools/sync_split_spec.py: " + ", ".join(stale))
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
