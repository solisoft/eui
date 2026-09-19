# One component: the page. `soli serve examples/eui-site` and open it with
#
#   eui ws://127.0.0.1:5012/_eui/session/site
#
# Needs a `soli` built with `--features eui`.

# The two faces this application draws in, bound to font roles here rather
# than in a view.
#
# Declaring a font is what takes the manifest's floor to EUI 4 (spec 02
# §5.1): `DefFont` is opcode 0x15 and a font role is a style byte of 2,
# and both are decode errors below it. A floor belongs in a manifest, and a
# manifest is signed before any view has run — so this is boot work, and a
# face declared halfway through a session would only fall back to `sans`.
#
# The files are in this application's own `public/fonts`, so nothing here
# talks to a font service and the window opens no connection the session
# did not (08 §8). The other way round -- fetching a family from Google at
# boot and serving it from your own origin afterwards -- is in the demo
# app's `google_fonts.sl`, and it ends in the same call.
# `rescue` because the faces are not in the repository -- `.gitignore`
# excludes `public/fonts/` -- and a checkout without them must still boot.
# Without this the server dies at line one with `EUI: asset ... not found`,
# and the whole application is unreachable over a typeface. It then draws
# in the client's own Inter, which is the fallback the views already expect
# from their own `rescue`.
eui_font("Space Grotesk", [
  "public/fonts/space-grotesk-400.ttf",
  "public/fonts/space-grotesk-700.ttf"
]) rescue nil

# The one thing this page asks of the machine: hand an https address to
# your browser when *you* click one. There is no op that opens an address
# and no event that reports one, so this buys the application nothing it
# can use on its own (03 §3.5).
eui_capabilities("net.open")

# The application's own name would go here — `eui_name("EUI")` — and does
# not yet. The builtin is on `soli_lang` main and in no release, the deploy
# pins an exact Soli, and `rescue` does not help: an unknown identifier is
# a *static* error and the routes never run. Re-add it with the pin, not
# before. Until then the manifest carries the directory's name, which is
# what a launcher entry is labelled with.

get("/health", "home#health")

router_eui("site", "site#site", "site#site_view", {"static": "public, max-age=60"})
