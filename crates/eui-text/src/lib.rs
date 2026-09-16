//! # EUI text
//!
//! Shaping and glyph rasterisation for the client, over `cosmic-text`
//! (rustybuzz for shaping, swash for rasterising). This is the one
//! third-party dependency on the CPU side of the client: shaping is the part
//! of text that must not be reinvented, and everything around it here is
//! ours.
//!
//! - Four faces are embedded — Inter (regular, bold), JetBrains Mono, and
//!   Noto Sans Symbols behind them — so a session that asks for nothing
//!   still draws. An application MAY name others: a `DefFont` op binds a
//!   font role to faces named by asset hash, and the client loads them here
//!   through [`TextEngine::add_font`] and [`TextEngine::bind_role`].
//! - [`TextEngine`] implements [`eui_layout::TextMeasurer`], with a bounded
//!   cache keyed by `(text, font, width, clamp)`: layout measures the same
//!   run under several constraints per frame, and shaping it once is the
//!   difference between a frame budget met and missed.
//! - [`Shaped`] carries glyph positions for the renderer, and
//!   [`TextEngine::rasterize`] turns a glyph into an alpha or colour bitmap
//!   at a device scale.
//!
//! Line clamping truncates; it does not yet append an ellipsis.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

use cosmic_text::{fontdb, Attrs, Buffer, CacheKey, Family, FontSystem, Metrics, Shaping, SwashCache, SwashContent, Weight};
use eui_layout::{FontSpec, TextMeasurer, TextMetrics};
use eui_proto::FontWeight;

const INTER_REGULAR: &[u8] = include_bytes!("../fonts/Inter-Regular.ttf");
const INTER_BOLD: &[u8] = include_bytes!("../fonts/Inter-Bold.ttf");
const JETBRAINS_MONO: &[u8] = include_bytes!("../fonts/JetBrainsMono-Regular.ttf");
/// Hearts, arrows, stars: what an interface reaches for that a text face
/// does not carry. Loaded last, so it only ever fills a gap.
const NOTO_SYMBOLS: &[u8] = include_bytes!("../fonts/NotoSansSymbols-Regular.ttf");

const SANS_FAMILY: &str = "Inter";
const MONO_FAMILY: &str = "JetBrains Mono";

/// The role a face is drawn in when the one a style names has no binding —
/// an application's face that has not arrived yet, or one whose bytes were
/// refused. Text drawn in the wrong face is a typographic complaint; text
/// not drawn at all is a broken page, so the fallback is never nothing.
const FALLBACK_FAMILY: &str = SANS_FAMILY;

/// How many shaped runs to keep before evicting the oldest.
/// How many shaped runs to keep. Sized to hold a page's working set rather
/// than a screenful: layout measures the same run at several widths on the
/// way to a line break, so an entry evicted before its next use is not a
/// miss, it is a re-shape. A 630-line markdown document (4 400 nodes, one
/// per word) asked for 23 324 shapes against 4 096 entries and 7 785
/// against these — 334 ms of layout against 137 (10 §"The shaped-run
/// cache"). Eviction is first-in, first-out, which is enough for a cache
/// that holds a page.
const CACHE_ENTRIES: usize = 16_384;

/// One positioned glyph, ready for the renderer.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Glyph {
    /// Left edge of the glyph's advance box, from the run's origin.
    pub x: f32,
    /// Baseline of the line the glyph sits on, from the run's top.
    pub y: f32,
    /// Advance width.
    pub w: f32,
    /// Size the glyph was shaped at.
    pub size: f32,
    /// Opaque key for [`TextEngine::rasterize`]. Two glyphs with equal keys
    /// share a bitmap at a given scale.
    pub key: GlyphKey,
    /// The byte range of the source text this glyph draws — a cluster, so
    /// `end` is always a char boundary. What a caret is placed against.
    pub start: usize,
    /// See `start`.
    pub end: usize,
}

impl Shaped {
    /// Where a caret placed before byte `at` sits: `(x, baseline)` from the
    /// run's origin.
    ///
    /// The line comes first, and it comes from [`Shaped::lines`] rather than
    /// from the glyphs: a line that carries no glyph — the one a newline
    /// just opened, or a blank line in the middle of a paragraph — has a
    /// place a caret can stand, and looking only at glyphs left the caret at
    /// the end of the line above until something was typed.
    pub fn caret(&self, at: usize) -> (f32, f32) {
        let Some(line) = self.lines.iter().rev().find(|l| l.start <= at).or_else(|| self.lines.first()) else {
            return (0.0, self.metrics.baseline);
        };
        let mut end = 0.0;
        for g in self.glyphs.iter().filter(|g| g.y == line.y) {
            if g.start >= at {
                return (g.x, line.y);
            }
            end = g.x + g.w;
        }
        (end, line.y)
    }

    /// The byte offset nearest a point from the run's origin: the line whose
    /// baseline is closest, then the glyph edge closest along it.
    pub fn byte_at(&self, x: f32, y: f32) -> usize {
        let Some(line) = self.lines.iter().min_by(|a, b| (a.y - y).abs().partial_cmp(&(b.y - y).abs()).unwrap_or(std::cmp::Ordering::Equal)) else {
            return 0;
        };
        let mut end = line.start;
        for g in self.glyphs.iter().filter(|g| g.y == line.y) {
            if x < g.x + g.w / 2.0 {
                return g.start;
            }
            end = g.end;
        }
        end
    }
}

/// Identifies a glyph in a face at a size; scale is applied at rasterisation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GlyphKey {
    font: fontdb::ID,
    glyph: u16,
    size_bits: u32,
}

/// A rasterised glyph.
/// A rasterised glyph as the engine holds it, borrowed: what an atlas packs
/// without a copy of the bitmap in between.
#[derive(Debug, Clone, Copy)]
pub struct GlyphRef<'a> {
    /// Left bearing, px.
    pub left: i32,
    /// Top bearing, px.
    pub top: i32,
    /// Width, px.
    pub width: u32,
    /// Height, px.
    pub height: u32,
    /// A colour glyph (an emoji): RGBA rather than coverage.
    pub color: bool,
    /// Row-major coverage, or RGBA when `color`.
    pub data: &'a [u8],
}

/// A rasterised glyph, owned: coverage, or RGBA when `color`, with its
/// bearings.
#[derive(Debug, Clone, PartialEq)]
pub struct GlyphImage {
    /// Horizontal offset from the glyph origin to the bitmap's left edge.
    pub left: i32,
    /// Vertical offset from the baseline up to the bitmap's top edge.
    pub top: i32,
    /// Bitmap width in device pixels.
    pub width: u32,
    /// Bitmap height in device pixels.
    pub height: u32,
    /// `true` for RGBA colour data, `false` for 8-bit coverage.
    pub color: bool,
    /// Row-major pixels; one byte per pixel for coverage, four for colour.
    pub data: Vec<u8>,
}

/// A shaped run: what layout measured and what the renderer draws.
#[derive(Debug, Clone, PartialEq)]
pub struct Shaped {
    /// The measurement layout used.
    pub metrics: TextMetrics,
    /// Glyphs in visual order.
    pub glyphs: Vec<Glyph>,
    /// One entry per laid-out line, in order, including the lines that
    /// carry no glyph. A caret and a click need a line before they need a
    /// glyph, and an empty line has the first without the second.
    pub lines: Vec<Line>,
}

/// One laid-out line: where its baseline sits and which bytes it holds.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Line {
    /// Baseline, from the run's top — the same `y` this line's glyphs carry.
    pub y: f32,
    /// First byte of the source text laid out on this line.
    pub start: usize,
    /// One past the last byte drawn on this line. The newline that ended the
    /// line is not counted: a caret there belongs to the line below.
    pub end: usize,
}

/// Cache statistics, for the budget harness.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Stats {
    /// Runs served from cache.
    pub hits: u64,
    /// Runs shaped.
    pub misses: u64,
    /// Bounded requests answered from the run's natural shape, because it
    /// fit: neither a hit under that key nor a shape.
    pub reused: u64,
    /// Runs evicted.
    pub evictions: u64,
    /// Entries added to the cache: a miss, or a reuse under a new key.
    pub inserted: u64,
}

/// Cache key. The text is represented by a 64-bit hash so a lookup allocates
/// nothing; the entry keeps the full text and a hit compares it, so a
/// collision costs a miss rather than a wrong glyph run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct ShapeKey {
    text_hash: u64,
    family: u8,
    weight: u8,
    size: u32,
    line_height: u32,
    max_width: Option<u32>,
    clamp: u8,
}

/// The text engine: font database, shaping, rasterisation, and the cache.
pub struct TextEngine {
    fonts: FontSystem,
    swash: SwashCache,
    cache: HashMap<ShapeKey, (String, Arc<Shaped>)>,
    order: VecDeque<ShapeKey>,
    /// The baseline an empty run takes, per font: the one a real glyph
    /// gets in that font, probed once rather than once per empty field
    /// per width.
    probes: HashMap<(u8, u8, u32, u32), Option<f32>>,
    /// Font role to family name. Roles `0` and `1` start on the embedded
    /// faces and an application may rebind them; `2..` are the
    /// application's own and start bound to nothing.
    roles: HashMap<u8, Arc<str>>,
    stats: Stats,
}

impl std::fmt::Debug for TextEngine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TextEngine").field("cached", &self.cache.len()).field("stats", &self.stats).finish()
    }
}

impl Default for TextEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl TextEngine {
    /// An engine with the embedded faces and nothing else.
    ///
    /// The database is built by hand rather than through
    /// `FontSystem::new_with_fonts`, which also enumerates the system's fonts.
    /// That would make the same text shape differently on different machines
    /// and expose the installed font list — two things the protocol forbids.
    /// The locale is fixed for the same reason: it only steers script
    /// fallback, and fallback must not depend on where the client runs.
    pub fn new() -> Self {
        let mut db = fontdb::Database::new();
        for bytes in [INTER_REGULAR, INTER_BOLD, JETBRAINS_MONO, NOTO_SYMBOLS] {
            db.load_font_source(fontdb::Source::Binary(Arc::new(bytes)));
        }
        let fonts = FontSystem::new_with_locale_and_db("en-US".to_owned(), db);
        let roles = HashMap::from([(0u8, Arc::from(SANS_FAMILY)), (1u8, Arc::from(MONO_FAMILY))]);
        Self { fonts, swash: SwashCache::new(), cache: HashMap::new(), order: VecDeque::new(), probes: HashMap::new(), roles, stats: Stats::default() }
    }

    /// Load an additional face from bytes — an application's font asset, once
    /// verified against its hash — and answer the family it calls itself.
    ///
    /// The name comes out of the face's own `name` table, because that is
    /// what shaping will match on: `bind_role` takes it and a role, and the
    /// two together are the whole of "this application draws headings in
    /// Playfair Display". `None` when the bytes are not a face this build can
    /// read, which is where a hostile or truncated asset stops — the caller
    /// fails the hash rather than binding a role to nothing.
    ///
    /// Later faces take priority for their family, so a second face with a
    /// family already loaded shadows the first for this session.
    ///
    /// The bytes arrive behind an `Arc` because that is how the asset store
    /// already holds them: a face is most of a megabyte, and the shaper and
    /// the store share the one allocation rather than each keeping a copy.
    pub fn add_font(&mut self, bytes: Arc<dyn AsRef<[u8]> + Send + Sync>) -> Option<String> {
        let db = self.fonts.db_mut();
        // `load_font_source` answers the ids it added -- a collection file
        // holds several faces -- and says nothing when the bytes parse as no
        // face at all. The first is the one whose name is taken: a `.ttc` is
        // one family in practice, and a caller that wants more than a name
        // wants a second asset, not a second face hidden in the first.
        let added = db.load_font_source(fontdb::Source::Binary(bytes));
        let family = added.first().and_then(|id| db.face(*id)).and_then(|face| face.families.first().map(|(name, _)| name.clone()));
        self.forget();
        family
    }

    /// Draw `role` in `family` from here on.
    ///
    /// Roles `0` and `1` are `sans` and `mono`; rebinding one replaces the
    /// embedded face for this session, which is what a theme's `font_sans`
    /// asset means (05 §3). Nothing verifies that a face by that name is
    /// loaded: one that is not falls back like a role that was never bound,
    /// and the text is drawn.
    pub fn bind_role(&mut self, role: u8, family: &str) {
        if self.roles.get(&role).is_some_and(|held| &**held == family) {
            return;
        }
        self.roles.insert(role, Arc::from(family));
        self.forget();
    }

    /// Unbind every application role and put `sans` and `mono` back on the
    /// embedded faces. The faces themselves stay in the database — they are
    /// content-addressed, so the session that loaded them may name them
    /// again, and re-parsing bytes it already has buys nothing.
    pub fn clear_roles(&mut self) {
        self.roles.clear();
        self.roles.insert(0, Arc::from(SANS_FAMILY));
        self.roles.insert(1, Arc::from(MONO_FAMILY));
        self.forget();
    }

    /// The family a role draws in, or `None` for one nothing has bound.
    pub fn role_family(&self, role: u8) -> Option<&str> {
        self.roles.get(&role).map(|f| &**f)
    }

    /// Drop everything shaped under the old face set. A shape is keyed by
    /// the *role*, not the face, so a role that changed meaning would
    /// otherwise be served the old glyphs out of the cache.
    fn forget(&mut self) {
        self.cache.clear();
        self.order.clear();
        self.probes.clear();
    }

    /// Cache statistics.
    pub fn stats(&self) -> Stats {
        self.stats
    }

    /// Number of faces loaded.
    pub fn face_count(&self) -> usize {
        self.fonts.db().len()
    }

    /// Shape a run, from cache when possible.
    ///
    /// Layout asks for the same run at every width it tries on the way to a
    /// line break — unbounded first, then the row's width, then the line's —
    /// and a run that fits on one line is the same run at every width that
    /// holds it. So a bounded request is answered from the run's natural
    /// shape whenever that shape fits, and the natural shape is made once. A
    /// 4 400-node markdown page asked for 7 785 shapes before this and 4 6xx
    /// after; the difference was most of its layout time.
    pub fn shape(&mut self, text: &str, font: FontSpec, max_width: Option<f32>, line_clamp: u8) -> Arc<Shaped> {
        // A width to the quarter pixel, rounded up: a run asked for at
        // 100.0 and again at 100.1 as a flex reflow settles is one entry,
        // not a miss of the whole cache. Shaped at that width too, so the
        // key and the shape agree.
        let max_width = max_width.map(quantise);
        let key = Self::key(text, font, max_width, line_clamp);
        if let Some(hit) = self.lookup(&key, text) {
            return hit;
        }
        if let Some(w) = max_width.filter(|w| w.is_finite() && *w >= 0.0) {
            if !text.contains('\n') {
                let natural = self.shape(text, font, None, line_clamp);
                // The same slack `shape_uncached` gives a bounded buffer, so
                // "fits" here and "did not wrap" there agree at the edge.
                if natural.metrics.lines <= 1 && natural.metrics.width <= w + 0.05 {
                    self.stats.reused = self.stats.reused.saturating_add(1);
                    self.remember(key, text, Arc::clone(&natural));
                    return natural;
                }
            }
        }
        self.stats.misses = self.stats.misses.saturating_add(1);
        let shaped = Arc::new(self.shape_uncached(text, font, max_width, line_clamp));
        self.remember(key, text, Arc::clone(&shaped));
        shaped
    }

    fn key(text: &str, font: FontSpec, max_width: Option<f32>, line_clamp: u8) -> ShapeKey {
        ShapeKey {
            text_hash: text_hash(text),
            family: font.family.to_u8(),
            weight: font.weight.to_u8(),
            size: font.size.to_bits(),
            line_height: font.line_height.to_bits(),
            max_width: max_width.map(f32::to_bits),
            clamp: line_clamp,
        }
    }

    fn lookup(&mut self, key: &ShapeKey, text: &str) -> Option<Arc<Shaped>> {
        let (cached_text, hit) = self.cache.get(key)?;
        if cached_text != text {
            return None;
        }
        self.stats.hits = self.stats.hits.saturating_add(1);
        Some(Arc::clone(hit))
    }

    /// Keep a shape under a key, evicting the oldest entry when full. A
    /// key already held -- a collision's text, replaced -- is updated in
    /// place: it keeps its place in the order, and the order keeps one
    /// entry per key, so the oldest entry evicted is the oldest and not a
    /// live one whose key was pushed twice.
    fn remember(&mut self, key: ShapeKey, text: &str, shaped: Arc<Shaped>) {
        if let Some(slot) = self.cache.get_mut(&key) {
            if slot.0 != text {
                slot.0 = text.to_owned();
            }
            slot.1 = shaped;
            return;
        }
        if self.cache.len() >= CACHE_ENTRIES {
            if let Some(old) = self.order.pop_front() {
                self.cache.remove(&old);
                self.stats.evictions = self.stats.evictions.saturating_add(1);
            }
        }
        self.order.push_back(key);
        self.stats.inserted = self.stats.inserted.saturating_add(1);
        self.cache.insert(key, (text.to_owned(), shaped));
    }

    fn shape_uncached(&mut self, text: &str, font: FontSpec, max_width: Option<f32>, line_clamp: u8) -> Shaped {
        let size = if font.size.is_finite() && font.size > 0.0 { font.size } else { 1.0 };
        let line_height = if font.line_height.is_finite() && font.line_height > 0.0 { font.line_height } else { size };
        let mut buffer = Buffer::new(&mut self.fonts, Metrics::new(size, line_height));
        // Cloned out of the table before the font system is borrowed: an
        // `Attrs` holds the name by reference for as long as the buffer is
        // shaped, and the table lives behind the same `&mut self`.
        let family: Arc<str> = self.roles.get(&font.family.to_u8()).cloned().unwrap_or_else(|| Arc::from(FALLBACK_FAMILY));
        let weight = match font.weight {
            FontWeight::Regular => Weight::NORMAL,
            FontWeight::Medium => Weight::MEDIUM,
            FontWeight::Semibold => Weight::SEMIBOLD,
            FontWeight::Bold => Weight::BOLD,
        };
        let attrs = Attrs::new().family(Family::Name(&family)).weight(weight);
        // Layout re-measures a run at exactly the width it first reported; a
        // wrap at float equality would then split it. A hair of slack keeps
        // "measure, then lay out at that size" a fixed point.
        buffer.set_size(&mut self.fonts, max_width.filter(|w| w.is_finite() && *w >= 0.0).map(|w| w + 0.05), None);
        buffer.set_text(&mut self.fonts, text, attrs, Shaping::Advanced);
        buffer.shape_until_scroll(&mut self.fonts, false);

        // cosmic-text reports a glyph's byte range relative to its *buffer
        // line*, so on multi-line text every line starts again at zero. A
        // caret, a selection and a `spans` prop are all expressed against
        // the whole string, so the line's own offset is added back here and
        // `Glyph::start` means what its documentation says.
        let mut line_base = Vec::with_capacity(16);
        let mut at = 0usize;
        for line in text.split('\n') {
            line_base.push(at);
            at = at.saturating_add(line.len()).saturating_add(1);
        }

        let mut glyphs = Vec::new();
        let mut rows: Vec<Line> = Vec::new();
        let mut width = 0.0f32;
        let mut lines = 0u32;
        let mut baseline = None;
        for run in buffer.layout_runs() {
            if line_clamp > 0 && lines >= u32::from(line_clamp) {
                break;
            }
            lines = lines.saturating_add(1);
            width = width.max(run.line_w);
            if baseline.is_none() {
                baseline = Some(run.line_y - run.line_top);
            }
            let base = line_base.get(run.line_i).copied().unwrap_or(0);
            // A run with no glyph is an empty line, and its bytes are the
            // empty range where the line begins. One with glyphs takes its
            // range from them rather than from the buffer line, because a
            // wrapped line is several runs and each holds a slice of it.
            let mut row = Line { y: run.line_y, start: base.saturating_add(run.glyphs.first().map_or(0, |g| g.start)), end: base };
            for g in run.glyphs.iter() {
                row.start = row.start.min(base.saturating_add(g.start));
                row.end = row.end.max(base.saturating_add(g.end));
                glyphs.push(Glyph {
                    x: g.x,
                    y: run.line_y,
                    w: g.w,
                    size: g.font_size,
                    key: GlyphKey { font: g.font_id, glyph: g.glyph_id, size_bits: g.font_size.to_bits() },
                    start: base.saturating_add(g.start),
                    end: base.saturating_add(g.end),
                });
            }
            rows.push(row);
        }
        // cosmic-text cuts a text into buffer lines the way `str::lines`
        // does, so the empty line a trailing newline opens is never laid
        // out: Return pressed at the end of a field added no height and
        // moved no caret, and nothing happened on screen until the next
        // character arrived. That line is a line. Its baseline is filled in
        // below, with every other line that carries no glyph.
        if text.ends_with('\n') && (line_clamp == 0 || lines < u32::from(line_clamp)) {
            lines = lines.saturating_add(1);
            rows.push(Line { y: f32::NAN, start: text.len(), end: text.len() });
        }
        // An empty text lays out an empty line whose baseline is the
        // fallback font's, not the face's the first glyph will use; a caret
        // in an empty field must sit where that glyph's would, so the
        // baseline is the one a real glyph gets in the same font.
        if glyphs.is_empty() {
            let probe_key = (font.family.to_u8(), font.weight.to_u8(), size.to_bits(), line_height.to_bits());
            baseline = match self.probes.get(&probe_key) {
                Some(b) => *b,
                None => {
                    let mut probe = Buffer::new(&mut self.fonts, Metrics::new(size, line_height));
                    probe.set_text(&mut self.fonts, "x", attrs, Shaping::Advanced);
                    probe.shape_until_scroll(&mut self.fonts, false);
                    let b = probe.layout_runs().next().map(|run| run.line_y - run.line_top);
                    self.probes.insert(probe_key, b);
                    b
                }
            };
        }
        // The same correction for a line that is empty inside a text that is
        // not — the one a trailing newline opens. It carries no glyph to be
        // measured from, so cosmic-text lays it out in the default face, and
        // a caret standing on it would sit a hair off the line above it.
        // Every line here advances by the same `line_height`, so an empty one
        // takes its baseline from the first line that has a glyph.
        let anchor = glyphs.first().and_then(|g| rows.iter().position(|r| r.y == g.y).map(|i| (i, g.y)));
        for (i, row) in rows.iter_mut().enumerate() {
            if glyphs.iter().any(|g| g.y == row.y) {
                continue;
            }
            row.y = match anchor {
                Some((at, y)) => y + (i as f32 - at as f32) * line_height,
                None => baseline.unwrap_or(size * 0.8) + i as f32 * line_height,
            };
        }
        let lines = lines.max(1);
        Shaped { metrics: TextMetrics { width, height: lines as f32 * line_height, baseline: baseline.unwrap_or(size * 0.8), lines }, glyphs, lines: rows }
    }

    /// Rasterise a glyph at a device scale (`2.0` for a 2× display).
    /// `None` when the face has no image for it.
    pub fn rasterize(&mut self, key: GlyphKey, scale: f32) -> Option<GlyphImage> {
        self.with_glyph(key, scale, |g| GlyphImage { left: g.left, top: g.top, width: g.width, height: g.height, color: g.color, data: g.data.to_vec() })
    }

    /// [`Self::rasterize`] without the copy: the bitmap is lent to `f`, as
    /// an atlas packing it wants it. `None` when the face has no image.
    pub fn with_glyph<T>(&mut self, key: GlyphKey, scale: f32, f: impl FnOnce(GlyphRef<'_>) -> T) -> Option<T> {
        let size = f32::from_bits(key.size_bits) * scale;
        let cache_key = CacheKey::new(key.font, key.glyph, size, (0.0, 0.0), cosmic_text::CacheKeyFlags::empty()).0;
        let image = self.swash.get_image(&mut self.fonts, cache_key).as_ref()?;
        let color = match image.content {
            SwashContent::Mask => false,
            SwashContent::Color => true,
            SwashContent::SubpixelMask => false,
        };
        Some(f(GlyphRef { left: image.placement.left, top: image.placement.top, width: image.placement.width, height: image.placement.height, color, data: &image.data }))
    }
}

/// A width to the quarter pixel, rounded up; anything that is not a width
/// is left alone.
fn quantise(w: f32) -> f32 {
    if w.is_finite() && w >= 0.0 {
        (w * 4.0).ceil() / 4.0
    } else {
        w
    }
}

/// FNV-1a over the bytes: stable across runs and platforms, and cheap. The
/// cache is per process, so a cryptographic hash would buy nothing here.
fn text_hash(text: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in text.bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}

impl TextMeasurer for TextEngine {
    fn measure(&mut self, text: &str, font: FontSpec, max_width: Option<f32>, line_clamp: u8) -> TextMetrics {
        self.shape(text, font, max_width, line_clamp).metrics
    }
}
