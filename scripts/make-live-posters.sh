#!/bin/bash
# The still each live demo shows before anybody asks for two megabytes.
#
#   scripts/make-live-posters.sh [port]
#
# Every poster is a real render of the component it stands in for: a Soli
# server answering an EUI session, the client laying it out and painting it on
# the GPU, read back off-screen by `examples/snapshot`. No mockups — which is
# also what makes them worth showing when a session cannot be opened at all.
# A browser with no WebGPU and no WebGL2, or a demo server that is down, ends
# up looking at the application rather than at a black rectangle.
#
# They are committed, because a runner has no GPU: `Renderer::new_headless`
# wants an adapter and CI has none. Regenerate after changing a component.

set -euo pipefail

here=$(cd "$(dirname "$0")/.." && pwd)
port=${1:-5096}
out="$here/examples/demo-app/public/images/live"
# The stage is 4:3 and at most ~914 CSS px wide; 900x675 at 2x is what a
# retina reader sees, and it is what the CSS `object-fit: contain` expects.
w=900; h=675; scale=2

# Same list as the page's whitelist. `check-live-page.sh` is what keeps the
# page and the routes honest; this only has to keep up with the page.
components=$(grep -oE '^\s+"[a-z0-9_-]+": \{$' \
  "$here/examples/demo-app/app/controllers/live_page_controller.sl" | tr -d ' "{:')

curl -sf -o /dev/null "http://127.0.0.1:$port/health" \
  || { echo "no demo app on :$port — run \`soli serve examples/demo-app --port $port\`" >&2; exit 1; }

# `rbuild eui` builds the whole workspace on the remote in under a minute and
# pulls the binaries into `target/remote/release` — which is also where
# `~/.local/bin/eui` looks first. Prefer whichever of the two is newer, and
# fall back to cargo where there is no remote builder at all.
snap=""
for candidate in "$here/target/remote/release/snapshot" "$here/target/release/snapshot"; do
  if [ -x "$candidate" ] && { [ -z "$snap" ] || [ "$candidate" -nt "$snap" ]; }; then snap=$candidate; fi
done
if [ -z "$snap" ]; then
  command -v cargo >/dev/null || { echo "no snapshot binary and no cargo" >&2; exit 1; }
  cargo build --release -p snapshot
  snap="$here/target/release/snapshot"
fi
echo "using ${snap/#$HOME/~}"

mkdir -p "$out"

# Let time pass before the last paint. Without it the tool paints but never
# ticks, so anything the clock drives is invisible to it — the `clock`
# component is *made* of that (06 §1.1) and photographs as an empty box, and
# a gallery whose pictures are still arriving photographs without them.
settle=${SNAPSHOT_SETTLE:-900}

for c in $components; do
  echo "poster: $c"
  EUI_ALLOW_INSECURE_LOOPBACK=1 SNAPSHOT_PNG=1 SNAPSHOT_SETTLE="$settle" \
    "$snap" "$out" \
    --soli "ws://127.0.0.1:$port/_eui/session/$c" "$c" "$w" "$h" "$scale"
done

echo
ls -l "$out" | tail -n +2 | awk '{ printf "  %-28s %6.1f KB\n", $9, $5 / 1024 }'
