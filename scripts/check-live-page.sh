#!/bin/bash
# Every component the live page offers must have a session to open.
#
#   scripts/check-live-page.sh
#
# `/live/:component` looks its slug up in a whitelist and then hands it to a
# canvas that opens `wss://.../_eui/session/<slug>`. A slug in the whitelist
# with no `router_eui` beside it is a page that loads, connects to nothing,
# and says only that a socket failed — which is the least useful place to
# learn that a route was renamed.
#
# Both lists are read from disk, so this fails on the rename and not on the
# demo. It is the same bargain as `sync-catalogue.sh --check`: a second of
# CI against an afternoon of looking at a blank rectangle.

set -euo pipefail

here=$(cd "$(dirname "$0")/.." && pwd)
app="$here/examples/demo-app"
routes="$app/config/routes.sl"
page="$app/app/controllers/live_page_controller.sl"

for f in "$routes" "$page"; do
  [ -f "$f" ] || { echo "no file at $f" >&2; exit 1; }
done

routed=$(grep -oE '^router_eui\("[^"]+"' "$routes" | cut -d'"' -f2 | sort -u)
# The whitelist entries are the only `"slug": {` lines in that file.
listed=$(grep -oE '^\s+"[a-z0-9_-]+": \{$' "$page" | tr -d ' "{:' | sort -u)

[ -n "$routed" ] || { echo "no router_eui found in $routes" >&2; exit 1; }
[ -n "$listed" ] || { echo "no components listed in $page" >&2; exit 1; }

missing=$(comm -23 <(echo "$listed") <(echo "$routed"))
if [ -n "$missing" ]; then
  echo "the live page offers components with no session to open:" >&2
  echo "$missing" | sed 's/^/  /' >&2
  echo "add a router_eui for each in config/routes.sl, or drop it from the page." >&2
  exit 1
fi

echo "live page: $(echo "$listed" | wc -l) component(s), every one routed"
