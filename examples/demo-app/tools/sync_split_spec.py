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
