#!/usr/bin/env python3
"""Copy the split-pane functions out of the catalogue and into the specs.

`app/controllers/eui_builders.sl` is loaded by the server, not by a bare
script, so a spec cannot import it. Until the catalogue is a package the
definitions are copied in, and copies drift -- so the copying is a command
and CI can check that running it changes nothing.

    python3 tools/sync_split_spec.py          # rewrite the specs
    python3 tools/sync_split_spec.py --check  # exit 1 if they are stale
"""

import pathlib
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
SOURCE = ROOT / "app" / "controllers" / "eui_builders.sl"
MARKER = "# ---- copied from app/controllers/eui_builders.sl, do not edit ----"

SPECS = {
    ROOT / "tests" / "split_spec.sl": ["split_span", "split_sizes", "split_at"],
    ROOT / "tests" / "drag_spec.sl": ["split_span", "split_sizes", "split_at", "split_event"],
}


def grab(src: str, name: str) -> str:
    start = src.index(f"def {name}(")
    end = src.index("\nend\n", start) + len("\nend\n")
    return src[start:end]


def rebuild(spec: pathlib.Path, names: list[str], src: str) -> str:
    text = spec.read_text()
    body = text[text.index("\ndef check(") :]
    head = text[: text.index("\ndef check(")]
    # Cut at whichever comes first, the marker or the first definition.
    # Taking the marker alone would preserve a stale copy sitting above it,
    # which is exactly what a file assembled before the marker existed has.
    cuts = [i for i in (head.find(MARKER), head.find("\ndef ")) if i != -1]
    cut = min(cuts) if cuts else len(head)
    head = head[:cut].rstrip("\n")
    copied = "\n".join(grab(src, n) for n in names)
    return f"{head}\n\n{MARKER}\n\n{copied}{body}"


def main() -> int:
    src = SOURCE.read_text()
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
