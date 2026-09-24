#!/bin/bash
# Copy the reference catalogue into the language repository, where
# `soli new <app> --eui` writes it from, and into any example that
# vendors it rather than defining its own handful of primitives.
#
#   sync-catalogue.sh          copy, and say what changed
#   sync-catalogue.sh --check  say whether the copies differ, change nothing
#
# The catalogue is spec/03-widgets.md §4: a server-side library composed
# only from the primitives. It lives here, in the application that
# exercises it; every other copy is written by this script. Nothing but
# this script should edit one, and every file is byte for byte identical
# on every side.
#
# It is six files because one was eight thousand lines, and the sixth —
# `tw()`, Tailwind-style classes as EUI styles — is a thing a new
# application reaches for on its own. They load into the same namespace
# and none depends on which loads first, so adding one is a matter of
# naming it here and in `templates/eui.rs` next door.
#
# A vendoring example is a third copy, and a third copy drifts exactly as
# the second would; so they are listed here too, and skipped when they are
# not checked out.

set -euo pipefail

here=$(cd "$(dirname "$0")/.." && pwd)
lang=${SOLI_LANG:-$here/../lang}
srcdir="$here/examples/demo-app/app/controllers"

files=(
  eui_builders.sl
  eui_builders_forms.sl
  eui_builders_charts.sl
  eui_builders_feed.sl
  eui_builders_markdown.sl
  eui_builders_tw.sl
)

[ -d "$lang" ] || { echo "no language repository at $lang (set SOLI_LANG)" >&2; exit 1; }
for f in "${files[@]}"; do
  [ -f "$srcdir/$f" ] || { echo "no catalogue file at $srcdir/$f" >&2; exit 1; }
done

dests=("$lang/src/scaffold/templates/eui")
for app in mail-app; do
  [ -d "$here/examples/$app/app/controllers" ] && dests+=("$here/examples/$app/app/controllers")
done

if [ "${1-}" = "--check" ]; then
  stale=()
  for dst in "${dests[@]}"; do
    for f in "${files[@]}"; do
      cmp -s "$srcdir/$f" "$dst/$f" || stale+=("$f -> $dst")
    done
  done
  if [ ${#stale[@]} -eq 0 ]; then
    echo "catalogue in sync (${#files[@]} files, ${#dests[@]} copies)"
    exit 0
  fi
  echo "catalogue differs — run scripts/sync-catalogue.sh" >&2
  for f in "${stale[@]}"; do echo "  $f" >&2; done
  exit 1
fi

copied=0
for dst in "${dests[@]}"; do
  mkdir -p "$dst"
  # A file that was dropped upstream has to go, or `soli new` keeps
  # writing it out of a stale `include_str!` nobody edits any more.
  for f in "$dst"/eui_builders*.sl; do
    [ -e "$f" ] || continue
    base=$(basename "$f")
    keep=
    for want in "${files[@]}"; do [ "$base" = "$want" ] && keep=1; done
    [ -n "$keep" ] || { rm "$f"; echo "removed $base from $dst, no longer in the catalogue"; }
  done

  for f in "${files[@]}"; do
    if cmp -s "$srcdir/$f" "$dst/$f" 2>/dev/null; then continue; fi
    cp "$srcdir/$f" "$dst/$f"
    echo "copied $f to $dst ($(wc -l < "$dst/$f") lines)"
    copied=$((copied + 1))
  done
done
[ "$copied" -gt 0 ] || echo "catalogue already in sync (${#files[@]} files, ${#dests[@]} copies)"
