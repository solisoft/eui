# The EUI icon

`eui.svg` is the icon. A rail and three rows — the smallest interface the
client can be asked to draw — which together silhouette an E, in the theme's
own accent indigo. Every part is a rounded rectangle, because that is the one
shape the renderer draws.

Everything else here is rendered from it by `scripts/make-icons.py` (needs
`rsvg-convert`; on Debian/Ubuntu `librsvg2-bin`, on macOS `brew install
librsvg`). Edit the SVG, run the script, commit what changed:

    png/eui-<n>.png   16 … 512, full bleed
    eui.icns          macOS: the artwork inset in 824 of 1024 with a shadow,
                      which is the canvas every other Mac app icon uses
    eui.ico           Windows, for installers and shortcuts
    eui.res           the same icon as a linker input

The rasters are committed rather than rendered during packaging. A DMG
backdrop carries the app's name and is drawn at build time for that reason;
an icon carries nothing that changes, and asking a packaging step on three
platforms for a working SVG rasteriser would trade this directory for a build
that fails differently on each of them.

## How each platform gets it

| Where | What carries it |
|---|---|
| macOS app, Dock, Finder | `eui.icns` copied into `Contents/Resources` and named by `CFBundleIconFile` — `scripts/wrap-macos-app.sh` |
| The DMG's window | the app's own icon, drawn by Finder |
| The DMG's volume, mounted | `.VolumeIcon.icns` on the image, plus the volume's custom-icon flag — same script |
| Windows `eui.exe`, Explorer, taskbar | `eui.res`, linked in by `crates/eui-client/build.rs` |
| The window itself, on X11 and Windows | `png/eui-64.png`, embedded in the binary and handed to winit |
| Linux launchers, docks, alt-tab | `../eui.desktop` and the hicolor tree, installed by `scripts/install-linux-icon.sh` |

Wayland is the one place a window cannot hand its own icon over: the
compositor matches the window's app id (`eui`) against a `.desktop` file and
takes the icon from there. On Wayland that script is not a nicety, it is the
icon.
