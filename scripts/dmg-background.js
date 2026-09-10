#!/usr/bin/osascript -l JavaScript
//
//   osascript -l JavaScript dmg-background.js <AppName> <out.png> <scale>
//
// Draws the backdrop for the installer window: a gradient, a headline, and
// chevrons pointing from where the app sits to where Applications sits.
//
// It is drawn rather than committed as a PNG so the headline can carry the
// app's own name — the same script dresses EUI and Vitrine — and so the
// repository keeps no binary asset that has to be re-exported by hand when
// the wording changes. AppKit is already on every machine that can build a
// .app, and the drawing is entirely offscreen: it wants no window server,
// no Apple events, and no permission grant, which is what lets it run on a
// CI runner. If any of it fails the caller ships a plain DMG instead.

ObjC.import('AppKit')

// The window, in points. The pixel grid is this times the scale, so one
// pass at 1 and one at 2 give the two representations of a HiDPI backdrop.
var W = 640
var H = 400

// Quartz counts up from the bottom left; every layout number below is read
// off the window from the top left, the way Finder positions its icons.
function up(y) { return H - y }
function rgba(r, g, b, a) { return $.NSColor.colorWithSRGBRedGreenBlueAlpha(r / 255, g / 255, b / 255, a) }

var INK = rgba(29, 36, 51, 1)
var MUTED = rgba(102, 112, 133, 1)
var FAINT = rgba(138, 148, 166, 1)
var ACCENT = [79, 91, 213]

// NSBezierPath's cap and join styles by number: round is 1 in both. The
// named constants were spelled differently before macOS 10.14 and reading
// them from the bridge is one more thing that can come back undefined.
var ROUND = 1

// NSFontWeight* are floating constants; taking them by value keeps this
// working whichever SDK's metadata is loaded.
var REGULAR = 0.0
var SEMIBOLD = 0.3

function text(str, size, weight, color, top) {
    var para = $.NSMutableParagraphStyle.alloc.init
    para.setAlignment(2) // NSTextAlignmentCenter
    var attrs = $.NSDictionary.dictionaryWithObjectsForKeys(
        [$.NSFont.systemFontOfSizeWeight(size, weight), color, para],
        [$.NSFontAttributeName, $.NSForegroundColorAttributeName, $.NSParagraphStyleAttributeName])
    var box = size * 1.7
    // drawInRect fills the box from its top edge down, so the box hangs
    // below `top` and the given size is where the line actually starts.
    $.NSAttributedString.alloc.initWithStringAttributes(str, attrs)
        .drawInRect($.NSMakeRect(0, up(top + box), W, box))
}

// A soft white pool under an icon slot, so a dark app icon has something to
// sit on and the two drop targets read as targets.
function glow(cx, cy, r) {
    var g = $.NSGradient.alloc.initWithStartingColorEndingColor(
        rgba(255, 255, 255, 0.95), rgba(255, 255, 255, 0))
    g.drawInBezierPathRelativeCenterPosition(
        $.NSBezierPath.bezierPathWithOvalInRect($.NSMakeRect(cx - r, up(cy) - r, r * 2, r * 2)),
        $.NSMakePoint(0, 0))
}

function chevron(cx, cy, alpha) {
    var p = $.NSBezierPath.bezierPath
    p.moveToPoint($.NSMakePoint(cx - 8, up(cy - 14)))
    p.lineToPoint($.NSMakePoint(cx + 8, up(cy)))
    p.lineToPoint($.NSMakePoint(cx - 8, up(cy + 14)))
    p.setLineWidth(7)
    p.setLineCapStyle(ROUND)
    p.setLineJoinStyle(ROUND)
    rgba(ACCENT[0], ACCENT[1], ACCENT[2], alpha).set
    p.stroke
}

function run(argv) {
    var name = argv[0] || 'the app'
    var out = argv[1]
    var scale = parseFloat(argv[2] || '1')
    if (!out) throw new Error('usage: dmg-background.js <AppName> <out.png> <scale>')

    var rep = $.NSBitmapImageRep.alloc
        .initWithBitmapDataPlanesPixelsWidePixelsHighBitsPerSampleSamplesPerPixelHasAlphaIsPlanarColorSpaceNameBytesPerRowBitsPerPixel(
            $(), W * scale, H * scale, 8, 4, true, false, $.NSCalibratedRGBColorSpace, 0, 0)
    // Declaring the rep's size in points is what scales the drawing: the
    // context maps this coordinate space onto the larger pixel grid.
    rep.setSize($.NSMakeSize(W, H))

    $.NSGraphicsContext.saveGraphicsState
    var ctx = $.NSGraphicsContext.graphicsContextWithBitmapImageRep(rep)
    $.NSGraphicsContext.setCurrentContext(ctx)

    $.NSGradient.alloc.initWithStartingColorEndingColor(rgba(221, 228, 239, 1), rgba(245, 248, 252, 1))
        .drawInRectAngle($.NSMakeRect(0, 0, W, H), 90)

    glow(170, 205, 118)
    glow(470, 205, 118)

    text(name, 25, SEMIBOLD, INK, 38)
    text('Drag it onto Applications to install', 13, REGULAR, MUTED, 76)

    chevron(288, 205, 0.25)
    chevron(320, 205, 0.5)
    chevron(352, 205, 0.85)

    text('First launch: right-click the app and choose Open', 11, REGULAR, FAINT, 352)

    ctx.flushGraphics
    $.NSGraphicsContext.restoreGraphicsState

    var png = rep.representationUsingTypeProperties(4, $.NSDictionary.dictionary) // NSPNGFileType
    if (!png.writeToFileAtomically(out, true)) throw new Error('could not write ' + out)
}
