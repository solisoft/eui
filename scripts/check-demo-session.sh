#!/bin/bash
# Does /demo's session route actually complete a WebSocket upgrade?
#
#   scripts/check-demo-session.sh [host] [component]
#
# The route this asks about is one line in `proxy.conf` on the server, not
# in this repository (deploy/README.md explains why), so nothing here can
# assert it — and it fails in the quietest way there is. A browser told an
# upgrade failed reports `close code=1006`, no `open`, no reason; the page
# looks like a page that loads and a client that starts, forever.
#
# Two things are needed to see the truth instead:
#
#   --http1.1, because Cloudflare drops the upgrade headers off an h2
#   request and answers with an ordinary page — a plain `curl -i` says 404
#   or 200 about something that was never a handshake;
#
#   --max-time, because success does not close. A 101 leaves the socket
#   open and curl waits on it, so a timeout AFTER the status line is the
#   passing case and has to be told apart from a timeout instead of one.

set -uo pipefail

host=${1:-eui.solisoft.net}
component=${2:-gallery}

ask() {  # ask <url> -> prints the status line's code, or "timeout"
  local out
  out=$(curl -sS -i --http1.1 --max-time 8 \
    -H "Connection: Upgrade" -H "Upgrade: websocket" \
    -H "Sec-WebSocket-Version: 13" \
    -H "Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==" \
    -H "Origin: https://$host" \
    "$1" 2>/dev/null | head -1)
  echo "${out:-timeout}" | grep -oE '[0-9]{3}' | head -1
}

url="https://$host/_eui/session/$component"
code=$(ask "$url")
echo "$url -> ${code:-no answer}"

case "$code" in
  101) echo "the session route carries an upgrade."; exit 0 ;;
  "")  echo "No status line came back at all — the host or TLS is the problem," >&2
       echo "not the route." >&2; exit 1 ;;
esac

echo >&2
echo "That is not an upgrade. The two ways this line is wrong, and how to tell:" >&2
echo >&2
echo "  502 — the target does not repeat the prefix. A path rule STRIPS it," >&2
echo "        so a target ending in '/' turns /_eui/session/$component into" >&2
echo "        /session/$component, which the demo application does not serve." >&2
echo "        Confirm by asking the backend both ways:" >&2
echo "          $0 eui-data.solisoft.net $component     # should be 101" >&2
echo >&2
echo "  404 — the rule is unscoped, and is catching the demo application's" >&2
echo "        own session paths as well. Scope it to the host." >&2
echo >&2
echo "  421 — no rule matched at all." >&2
echo >&2
echo "The line, in proxy.conf on the server:" >&2
echo "  eui.solisoft.net/_eui/* -> https://eui-data.solisoft.net/_eui/" >&2
exit 1
