#!/usr/bin/env python3
"""Render every icon the three platforms want from assets/icon/eui.svg.

    scripts/make-icons.py

Writes, all under assets/icon/:

    png/eui-<n>.png   16 … 512, full bleed: Linux hicolor, and the 64 the
                      client embeds for the window itself
    eui.icns          the macOS bundle icon, and the DMG's volume icon
    eui.ico           the Windows icon, for installers and shortcuts
    eui.res           the same icon as a linker input, which is how it gets
                      into eui.exe itself (crates/eui-client/build.rs)

The rasters are committed rather than built during packaging. A DMG backdrop
carries the app's name and is drawn at build time for that reason; an icon
carries nothing that changes, and asking every packaging step on three
platforms for a working SVG rasteriser would trade a 60 kB directory for a
build that fails differently on each of them.

Needs `rsvg-convert` (librsvg) and nothing else: `.icns` and `.ico` are
containers, and both are written here rather than by `iconutil` — which is
macOS only — or by ImageMagick.
"""

import pathlib
import shutil
import struct
import subprocess
import sys
import tempfile
import zlib

ROOT = pathlib.Path(__file__).resolve().parent.parent
SRC = ROOT / "assets/icon/eui.svg"
OUT = ROOT / "assets/icon"

# Full bleed, for Linux and Windows and the window's own icon. macOS gets
# its own canvas below.
PNG_SIZES = [16, 24, 32, 48, 64, 128, 256, 512]
# What iOS asks for on a home screen: 60pt at @2x and @3x for a phone, 76pt
# and 83.5pt at @2x for the two iPads, and 1024 for the store. Named for the
# point size because that is how `CFBundleIconFiles` resolves them.
IOS_ICONS = {"AppIcon60x60@2x": 120, "AppIcon60x60@3x": 180, "AppIcon76x76@2x": 152, "AppIcon83.5x83.5@2x": 167, "AppIcon1024": 1024}

# The ICO's entries. The two large ones go in as PNG — that is how a modern
# .ico carries them, and an uncompressed 256 would be a quarter of a
# megabyte on its own; the small ones go in as DIBs, which is what an older
# shell will still take.
ICO_SIZES = [16, 24, 32, 48, 64, 128, 256]
ICO_PNG_FROM = 128

# What `iconutil` makes from an .iconset, in its order: (type, pixels).
# The duplicates are deliberate — 32 px is both `icon_32x32` and
# `icon_16x16@2x`, and macOS picks by the slot, not by the size.
ICNS_ENTRIES = [
    (b"icp4", 16),
    (b"ic11", 32),
    (b"icp5", 32),
    (b"ic12", 64),
    (b"ic07", 128),
    (b"ic13", 256),
    (b"ic08", 256),
    (b"ic14", 512),
    (b"ic09", 512),
    (b"ic10", 1024),
]

# macOS does not draw the tile edge to edge: the artwork sits in 824 of the
# 1024 with a soft shadow under it, which is what makes an app icon line up
# with every other one in the Dock and in the DMG window.
MAC_TEMPLATE = """<svg xmlns="http://www.w3.org/2000/svg" width="1024" height="1024" viewBox="0 0 1024 1024">
  <defs>
    <filter id="drop" x="-10%" y="-10%" width="120%" height="125%">
      <feDropShadow dx="0" dy="10" stdDeviation="11" flood-color="#101728" flood-opacity="0.28"/>
    </filter>
  </defs>
  <g filter="url(#drop)" transform="translate(100 92) scale(0.8047)">
{body}
  </g>
</svg>
"""


def render(svg: pathlib.Path, size: int, dst: pathlib.Path) -> bytes:
    """One PNG at `size`, through librsvg."""
    subprocess.run(
        ["rsvg-convert", "-w", str(size), "-h", str(size), str(svg), "-o", str(dst)],
        check=True,
    )
    return dst.read_bytes()


def mac_svg(tmp: pathlib.Path) -> pathlib.Path:
    """The same mark on the macOS canvas: inset, and with a shadow."""
    text = SRC.read_text()
    body = text[text.index(">", text.index("<svg")) + 1 : text.rindex("</svg>")]
    dst = tmp / "eui-macos.svg"
    dst.write_text(MAC_TEMPLATE.format(body=body))
    return dst


def ios_svg(tmp: pathlib.Path) -> pathlib.Path:
    """The same mark, full bleed.

    iOS masks an app icon into its own squircle and composites whatever is
    left onto black, so the rounded corners this icon draws for every other
    platform would show as a dark fringe *inside* that mask — the corner
    rounded twice, once by us and once by the system, with black between.
    Squaring off our own rounding is the whole difference."""
    text = SRC.read_text().replace('width="1024" height="1024" rx="228" ry="228"', 'width="1024" height="1024"')
    dst = tmp / "eui-ios.svg"
    dst.write_text(text)
    return dst


def unpack_png(data: bytes) -> tuple[int, int, bytes]:
    """A PNG's pixels, as RGBA rows. Only what librsvg writes: 8-bit RGBA,
    not interlaced — enough to fold into the DIBs an .ico wants."""
    pos, w, h, idat = 8, 0, 0, b""
    while pos < len(data):
        (length,) = struct.unpack(">I", data[pos : pos + 4])
        kind = data[pos + 4 : pos + 8]
        body = data[pos + 8 : pos + 8 + length]
        if kind == b"IHDR":
            w, h, depth, colour, _, _, interlace = struct.unpack(">IIBBBBB", body)
            if (depth, colour, interlace) != (8, 6, 0):
                raise SystemExit(f"unexpected PNG: depth {depth}, colour {colour}, interlace {interlace}")
        elif kind == b"IDAT":
            idat += body
        pos += 12 + length
    raw = zlib.decompress(idat)
    stride = w * 4
    out = bytearray(h * stride)
    prev = bytearray(stride)
    src = 0
    for y in range(h):
        filt = raw[src]
        src += 1
        line = bytearray(raw[src : src + stride])
        src += stride
        for x in range(stride):
            a = line[x - 4] if x >= 4 else 0
            b = prev[x]
            c = prev[x - 4] if x >= 4 else 0
            if filt == 1:
                line[x] = (line[x] + a) & 0xFF
            elif filt == 2:
                line[x] = (line[x] + b) & 0xFF
            elif filt == 3:
                line[x] = (line[x] + (a + b) // 2) & 0xFF
            elif filt == 4:
                p = a + b - c
                pa, pb, pc = abs(p - a), abs(p - b), abs(p - c)
                line[x] = (line[x] + (a if pa <= pb and pa <= pc else b if pb <= pc else c)) & 0xFF
        out[y * stride : (y + 1) * stride] = line
        prev = line
    return w, h, bytes(out)


def dib(png: bytes) -> bytes:
    """A PNG as the BGRA bottom-up bitmap an .ico entry holds, with the
    1-bit mask that every entry carries whether or not it is read."""
    w, h, rgba = unpack_png(png)
    rows = []
    for y in range(h - 1, -1, -1):
        row = bytearray()
        for x in range(w):
            r, g, b, a = rgba[(y * w + x) * 4 : (y * w + x) * 4 + 4]
            row += bytes((b, g, r, a))
        rows.append(bytes(row))
    mask_stride = ((w + 31) // 32) * 4
    mask = bytes(mask_stride * h)
    header = struct.pack("<IiiHHIIiiII", 40, w, h * 2, 1, 32, 0, w * h * 4, 0, 0, 0, 0)
    return header + b"".join(rows) + mask


def write_ico(entries: list[tuple[int, bytes]], dst: pathlib.Path) -> None:
    """`entries` is (size, payload); a payload is a PNG or a DIB."""
    head = struct.pack("<HHH", 0, 1, len(entries))
    offset = len(head) + 16 * len(entries)
    directory, blobs = b"", b""
    for size, payload in entries:
        side = 0 if size >= 256 else size
        directory += struct.pack("<BBBBHHII", side, side, 0, 0, 1, 32, len(payload), offset)
        blobs += payload
        offset += len(payload)
    dst.write_bytes(head + directory + blobs)


# A Windows resource script is compiled by `rc.exe`, which exists only on
# Windows and only with the SDK installed. The thing `rc.exe` produces is a
# .res, and link.exe takes one on its command line — so the .res is written
# here instead, and the Windows build links a file rather than looking for a
# compiler. The format is a run of entries, each a 32-byte header (both
# type and name given as ordinals) followed by its data, everything padded
# to a DWORD; the run opens with an empty entry, which is how a reader tells
# a .res from anything else.
RT_ICON = 3
RT_GROUP_ICON = 14
LANG_EN_US = 0x0409


def res_entry(kind: int, name: int, data: bytes, flags: int, lang: int = LANG_EN_US) -> bytes:
    header = struct.pack(
        "<IIHHHHIHHII",
        len(data),
        32,
        0xFFFF,
        kind,
        0xFFFF,
        name,
        0,  # DataVersion
        flags,
        lang,
        0,  # Version
        0,  # Characteristics
    )
    pad = (-len(data)) % 4
    return header + data + bytes(pad)


def write_res(entries: list[tuple[int, bytes]], dst: pathlib.Path) -> None:
    """`entries` is (size, payload) as for the .ico. Every image goes in as
    its own RT_ICON, and one RT_GROUP_ICON names them: that group, being the
    lowest-numbered one in the binary, is the icon Explorer draws."""
    out = [res_entry(0, 0, b"", 0, lang=0)]
    group = struct.pack("<HHH", 0, 1, len(entries))
    for i, (size, payload) in enumerate(entries, start=1):
        side = 0 if size >= 256 else size
        group += struct.pack("<BBBBHHIH", side, side, 0, 0, 1, 32, len(payload), i)
        out.append(res_entry(RT_ICON, i, payload, 0x1010))
    out.append(res_entry(RT_GROUP_ICON, 1, group, 0x1030))
    dst.write_bytes(b"".join(out))


def write_icns(entries: list[tuple[bytes, bytes]], dst: pathlib.Path) -> None:
    """`entries` is (four-byte type, PNG)."""
    body = b"".join(kind + struct.pack(">I", len(png) + 8) + png for kind, png in entries)
    dst.write_bytes(b"icns" + struct.pack(">I", len(body) + 8) + body)


def main() -> None:
    if not shutil.which("rsvg-convert"):
        raise SystemExit("make-icons: rsvg-convert not found (Debian/Ubuntu: librsvg2-bin, macOS: brew install librsvg)")
    tmp = OUT / ".render"
    tmp.mkdir(parents=True, exist_ok=True)
    (OUT / "png").mkdir(parents=True, exist_ok=True)

    for size in PNG_SIZES:
        render(SRC, size, OUT / f"png/eui-{size}.png")
    print(f"make-icons: {len(PNG_SIZES)} PNGs in {OUT / 'png'}")

    (OUT / "ios").mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory() as td:
        flat = ios_svg(pathlib.Path(td))
        for name, size in IOS_ICONS.items():
            render(flat, size, OUT / f"ios/{name}.png")
    print(f"make-icons: {len(IOS_ICONS)} iOS icons in {OUT / 'ios'}")

    ico = []
    for size in ICO_SIZES:
        png = (OUT / f"png/eui-{size}.png").read_bytes()
        ico.append((size, png if size >= ICO_PNG_FROM else dib(png)))
    write_ico(ico, OUT / "eui.ico")
    print(f"make-icons: {OUT / 'eui.ico'} ({(OUT / 'eui.ico').stat().st_size} B)")
    write_res(ico, OUT / "eui.res")
    print(f"make-icons: {OUT / 'eui.res'} ({(OUT / 'eui.res').stat().st_size} B)")

    mac = mac_svg(tmp)
    rendered: dict[int, bytes] = {}
    icns = []
    for kind, size in ICNS_ENTRIES:
        if size not in rendered:
            rendered[size] = render(mac, size, tmp / f"mac-{size}.png")
        icns.append((kind, rendered[size]))
    write_icns(icns, OUT / "eui.icns")
    print(f"make-icons: {OUT / 'eui.icns'} ({(OUT / 'eui.icns').stat().st_size} B)")

    shutil.rmtree(tmp)


if __name__ == "__main__":
    sys.exit(main())
