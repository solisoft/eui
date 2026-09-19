#!/bin/bash
# Copy the documentation and the specification into the site, which is the
# only directory the deploy rsyncs.
#
#   sync-docs.sh          copy, and say what changed
#   sync-docs.sh --check  say whether the copies differ, change nothing
#
# `deploy-site.yml` rsyncs `www/` and nothing else — deliberately, so that
# a deploy cannot reach past its own folder — and the two things the site
# has to serve live outside it. `spec/` is at the root because it is
# normative and a hundred code comments cite it by that path; the prose
# docs are in `doc/docs/eui/` beside the application that previews them.
# Neither should move to satisfy a deploy script.
#
# So the site gets copies, and this is the only thing that writes one. The
# arrangement, the reason and the `--check` are `sync-catalogue.sh`'s next
# door: one home for the source, machine-enforced copies wherever a build
# needs them, and a check in CI so the two cannot drift quietly.

set -euo pipefail

here=$(cd "$(dirname "$0")/.." && pwd)
check=false
[ "${1-}" = "--check" ] && check=true

# from -> to, both relative to the repository root. Markdown only: a
# directory may hold images or a draft, and the site serves what is listed.
pairs=(
  "doc/docs/eui:www/docs/eui"
  "spec:www/docs/spec"
)

stale=()
copied=0

for pair in "${pairs[@]}"; do
  src="$here/${pair%%:*}"
  dst="$here/${pair##*:}"
  [ -d "$src" ] || { echo "no source at $src" >&2; exit 1; }
  $check || mkdir -p "$dst"

  for f in "$src"/*.md; do
    [ -f "$f" ] || continue
    name=$(basename "$f")
    if $check; then
      cmp -s "$f" "$dst/$name" || stale+=("${pair##*:}/$name")
    else
      cp "$f" "$dst/$name"
      copied=$((copied + 1))
    fi
  done

  # A page deleted upstream must not go on being served. Only markdown is
  # considered, for the reason above.
  for f in "$dst"/*.md; do
    [ -f "$f" ] || continue
    name=$(basename "$f")
    [ -f "$src/$name" ] && continue
    if $check; then
      stale+=("${pair##*:}/$name (no longer in ${pair%%:*})")
    else
      rm "$f"
      echo "removed $(basename "$dst")/$name — gone from ${pair%%:*}"
    fi
  done
done

if $check; then
  if [ ${#stale[@]} -eq 0 ]; then
    echo "the site's documentation matches its sources"
    exit 0
  fi
  echo "the site's documentation is stale — run scripts/sync-docs.sh" >&2
  for f in "${stale[@]}"; do echo "  $f" >&2; done
  exit 1
fi

echo "copied $copied file(s) into www/docs/"
