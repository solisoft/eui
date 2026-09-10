#!/bin/bash
# Copy the reference catalogue into the language repository, where
# `soli new <app> --eui` writes it from.
#
#   sync-catalogue.sh          copy, and say what changed
#   sync-catalogue.sh --check  say whether the two differ, change nothing
#
# The catalogue is spec/03-widgets.md §4: a server-side library composed
# only from the primitives. It lives here, in the application that
# exercises it; the copy in `lang` is what a new project starts with.
# Nothing but this script should edit that copy, and the two files are
# byte for byte identical.

set -euo pipefail

here=$(cd "$(dirname "$0")/.." && pwd)
src="$here/examples/demo-app/app/controllers/eui_builders.sl"
lang=${SOLI_LANG:-$here/../lang}
dst="$lang/src/scaffold/templates/eui/eui_builders.sl"

[ -f "$src" ] || { echo "no catalogue at $src" >&2; exit 1; }
[ -d "$lang" ] || { echo "no language repository at $lang (set SOLI_LANG)" >&2; exit 1; }

if [ "${1-}" = "--check" ]; then
  if cmp -s "$src" "$dst"; then
    echo "catalogue in sync"
  else
    echo "catalogue differs from $dst — run scripts/sync-catalogue.sh" >&2
    diff -u "$dst" "$src" | head -40 >&2
    exit 1
  fi
  exit 0
fi

if cmp -s "$src" "$dst" 2>/dev/null; then
  echo "catalogue already in sync"
else
  mkdir -p "$(dirname "$dst")"
  cp "$src" "$dst"
  echo "catalogue copied to $dst ($(wc -l < "$dst") lines)"
fi
