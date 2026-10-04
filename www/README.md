# eui/www — the public site

One page, server-rendered by Soli, describing what EUI is. It is deliberately
plain machinery: **no framework, no build step, no npm, and no JavaScript
beyond fifteen lines that light up a paragraph when you hover a byte, and
ten that turn the fonts on once the page has painted.**

- `app/views/home/index.html.slv` — the page.
- `app/views/layouts/application.html.slv` — head, stylesheet.
- `app/views/layouts/_fonts.html.slv` and `public/css/fonts.css` — the three
  faces (IBM Plex Mono, IBM Plex Sans, Instrument Serif; SIL OFL), served
  from `public/fonts/` rather than Google Fonts, latin and latin-ext only.
  The stylesheet is linked as `print` and switched on at `load`, so the first
  paint uses the fallback faces the tokens name and no font is on its path;
  a browser that has had them once turns them on before painting.

**Lighthouse, landing page, 2026-10-04** (12.8.2, a local replica — Soli in
production mode behind a brotli HTTPS proxy, calibrated against
eui.solisoft.net, which scored the same 99 there before the change): 100 for
performance, accessibility, best practices and SEO, on mobile and on desktop.
Mobile: first contentful paint 0.9 s, largest 1.4 s, CLS 0.0008, TBT 0 —
from 1.6 s and 1.6 s with Google Fonts, whose stylesheet alone was 0.8 s of
render blocking on the simulated phone. Desktop: 0.35 s and 0.39 s.
- `public/css/site.css` — hand-written, with the design tokens at the top.
- `share/card.html` — the 1200 × 630 picture a link to the site unfurls into
  (Open Graph and X cards, which the layout declares on every page, with the
  canonical URL, an `og:image:alt`, JSON-LD, the touch icon and
  `public/manifest.json`). It is the demo's Meridian dashboard — the real
  render `public/images/demo/gallery-light.png` — in a client window beside
  the thesis, rendered to `public/images/share/eui.png` by
  `../scripts/make-share-card.sh`, which needs a Chromium; edit the HTML (or
  re-take the demo render), run the script, commit both.
  `tests/share_spec.sl` checks every tag and fetches every file they name.

The documentation site is the sibling app in `../doc`.

## The rule this page follows

Every number on it comes from a measurement in this repository, and the page
says which. The twenty-two bytes in the hero are the real encoding of

```rust
Frame::Batch(Batch { seq: 2, ops: vec![Op::SetText { node: 141, text: TextRef::Inline("1 984, 42 €".into()) }] })
```

taken from `crates/eui-proto/tests/size_budget.rs`; the field labels are the
wire format of `spec/02-wire-format.md`, byte for byte. If a number changes,
change it here too — a marketing page that drifts from its own test suite is
worse than no page.

## Running it

```sh
soli serve . --dev          # http://localhost:5011
```

The nav points at GitHub while the documentation host is not deployed; repoint
those four links when it is.

### `/demo` does not run locally, and the reason is the point it makes

The page opens `ws://<this host>/_eui/session/gallery` — deliberately on its
own host, because Soli refuses an upgrade whose `Origin` is not its own
(SEC-046). The check is an authority comparison with no allowlist, so
`127.0.0.1:5011` and `127.0.0.1:5081` are two different origins and a
`data-url` pointing at the demo application is refused exactly as a
cross-site one would be. This site serves no sessions of its own, so in
development `/demo` shows its poster and a note, which is the designed
fallback rather than a fault.

**To see a live session, run the application that serves them** and use its
own page, where the session and the page already share an origin:

```sh
cargo run -p xtask-web                          # build the client first
soli serve ../examples/demo-app --port 5081     # then: /live/gallery
```

`soli serve` caches its assets at boot, so restart it after rebuilding the
client or it goes on serving the module it started with — the `?v=` in the
browser's console lines is the version actually loaded, and
`public/eui/manifest.json` is the one on disk.

**To exercise `/demo` itself** you need what production has: `soli-proxy` in
front of both, carrying `/_eui/` from this site's origin to the application.
Give the two of them their production names under `.test` — RFC 6761 reserves
it for exactly this — and the local rule is the deployed rule, verbatim:

```
eui.solisoft.test/_eui/* -> https://eui-data.solisoft.test/_eui/
```

Both halves matter here as much as they do in production. The **host** scopes
it, or it catches the demo application's own session paths as well. The
**target repeats the prefix**, because a path rule strips what it matched: a
target ending in `/` would send the backend `/session/gallery`, which nothing
serves, and a browser reports the failed upgrade as `close 1006` and no
reason at all. `deploy/README.md` has the full account, including
`scripts/check-demo-session.sh`, which takes a host and says which of the two
is wrong:

```sh
scripts/check-demo-session.sh eui.solisoft.test gallery   # 101 = working
```

The DNS and the wildcard certificate that make `*.solisoft.test` resolve and
serve TLS are a one-time setup, written up in the proxy's own
`docs/omarchy-dev-setup.md` (dnsmasq for `.test`, mkcert for
`*.solisoft.test`).
