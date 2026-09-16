# eui-site

EUI's landing page, written as an EUI application. Not a web page about the
protocol — the page *is* the protocol: one component, one handler, one view,
built from the same eight-key node hashes and flat style records an ERP row
is made of. Every colour is a theme role, so the page is right in the
viewer's dark mode without this server ever learning they went there, which
is one of the things the page is claiming.

It is also where two client features earn their keep: `net.open`, so a link
on the page opens in the reader's own browser when *they* click it (spec 03
§3.5), and the two slowest steps of the `motion` scale, for a thing arriving
over a distance rather than a control changing state (spec 05 §2).

## Running it

Needs a `soli` built with `--features eui`.

```
soli serve examples/eui-site
eui ws://127.0.0.1:5012/_eui/session/site
```

The client will ask whether this application may open web addresses in your
browser. Answer no and the page still draws; the links simply do nothing,
which is what a refused capability is supposed to look like.

## The faces

`config/routes.sl` binds two families at boot, from this application's own
`public/fonts/` — so nothing here talks to a font service and the window
opens no connection the session did not (spec 08 §8).

The files themselves are not in the repository. Put these four there:

| File | Family |
|---|---|
| `playfair-display-400.ttf` | Playfair Display, regular |
| `playfair-display-700.ttf` | Playfair Display, bold |
| `space-grotesk-400.ttf` | Space Grotesk, regular |
| `space-grotesk-700.ttf` | Space Grotesk, bold |

Both families are SIL Open Font License 1.1, from Google Fonts. A checkout
without them serves the page anyway: `site_controller.sl` answers `"sans"`
when a face is not on disk, and the client draws in its own Inter rather
than naming a font nothing declared.

The other way round — fetching a family at boot and serving it from your own
origin afterwards — is in the demo app's `google_fonts.sl`, and it ends in
the same `eui_font` call.
