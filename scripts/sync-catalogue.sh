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
# Nothing but this script should edit that copy, and every file is byte
# for byte identical on both sides.
#
# It is four files because one was eight thousand lines. They load into
# the same namespace and none depends on which loads first, so adding one
# is a matter of naming it here and in `templates/eui.rs` next door.

set -euo pipefail

here=$(cd "$(dirname "$0")/.." && pwd)
lang=${SOLI_LANG:-$here/../lang}
srcdir="$here/examples/demo-app/app/controllers"
dstdir="$lang/src/scaffold/templates/eui"

files=(
  eui_builders.sl
  eui_builders_forms.sl
  eui_builders_charts.sl
  eui_builders_feed.sl
)

[ -d "$lang" ] || { echo "no language repository at $lang (set SOLI_LANG)" >&2; exit 1; }
for f in "${files[@]}"; do
  [ -f "$srcdir/$f" ] || { echo "no catalogue file at $srcdir/$f" >&2; exit 1; }
done

if [ "${1-}" = "--check" ]; then
  stale=()
  for f in "${files[@]}"; do
    cmp -s "$srcdir/$f" "$dstdir/$f" || stale+=("$f")
  done
  if [ ${#stale[@]} -eq 0 ]; then
    echo "catalogue in sync (${#files[@]} files)"
    exit 0
  fi
  echo "catalogue differs from $dstdir — run scripts/sync-catalogue.sh" >&2
  for f in "${stale[@]}"; do
    diff -u "$dstdir/$f" "$srcdir/$f" 2>/dev/null | head -40 >&2
  done
  exit 1
fi

# A file that was dropped upstream has to go, or `soli new` keeps writing
# it out of a stale `include_str!` nobody edits any more.
mkdir -p "$dstdir"
for f in "$dstdir"/eui_builders*.sl; do
  [ -e "$f" ] || continue
  base=$(basename "$f")
  keep=
  for want in "${files[@]}"; do [ "$base" = "$want" ] && keep=1; done
  [ -n "$keep" ] || { rm "$f"; echo "removed $base, no longer in the catalogue"; }
done

copied=0
for f in "${files[@]}"; do
  if cmp -s "$srcdir/$f" "$dstdir/$f" 2>/dev/null; then continue; fi
  cp "$srcdir/$f" "$dstdir/$f"
  echo "copied $f to $dstdir ($(wc -l < "$dstdir/$f") lines)"
  copied=$((copied + 1))
done
[ "$copied" -gt 0 ] || echo "catalogue already in sync (${#files[@]} files)"
