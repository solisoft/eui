//! What a text node painted last frame, kept for the frames it does not
//! change in.
//!
//! A frame walks the whole tree and builds every quad afresh: for a text
//! node that is a shape-cache lookup and, per glyph, an atlas lookup and a
//! quad. Most frames change a few nodes and none of the words. So a text
//! node that is not dirty and paints under the same key -- style, box,
//! colour, opacity, scale, atlas -- gets its quads back as they were, and
//! one moved by whole device pixels (a scroll) gets them moved.
//!
//! What is kept is the quads as the node built them, before the entrance
//! above it or its own transition touched them: those are applied as the
//! quads are pushed, every frame, so a transition that starts later finds
//! them as it would have.

use std::collections::HashMap;

use eui_tree::NodeIx;

use crate::paint::Quad;

/// Everything a text node's quads were built from. Equal keys, equal quads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TextKey {
    /// The node's id, since an index is reused.
    pub id: u32,
    /// Its style id: records are defined once.
    pub style: u32,
    /// Its border box in device pixels, rounded as the painter rounds it.
    pub dev: [i32; 4],
    /// The foreground it inherited, as bits.
    pub fg: [u32; 4],
    /// Its opacity, as bits.
    pub opacity: u32,
    /// Device pixels per logical pixel, as bits.
    pub scale: u32,
    /// The atlas generation the uvs were taken from.
    pub atlas: u32,
}

impl TextKey {
    /// `Some(dx, dy)` when the node under `self` is the same node, painted
    /// alike, in a box of the same size moved by whole device pixels from
    /// where `other` had it.
    fn translation_from(&self, other: &TextKey) -> Option<(f32, f32)> {
        let alike = self.id == other.id
            && self.style == other.style
            && self.fg == other.fg
            && self.opacity == other.opacity
            && self.scale == other.scale
            && self.atlas == other.atlas
            && self.dev[2] == other.dev[2]
            && self.dev[3] == other.dev[3];
        #[expect(clippy::cast_precision_loss, reason = "device pixel offsets, far below 2^24")]
        alike.then(|| ((self.dev[0] - other.dev[0]) as f32, (self.dev[1] - other.dev[1]) as f32))
    }
}

struct TextEntry {
    key: TextKey,
    quads: Vec<Quad>,
    /// The paint generation that last used it.
    seen: u32,
}

/// What the cache did over its life.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct PaintStats {
    /// Text nodes whose quads were handed back as they were.
    pub hits: u64,
    /// Handed back moved by whole device pixels.
    pub translated: u64,
    /// Built afresh.
    pub rebuilt: u64,
    /// Dropped: not painted for a frame, or the cache was full.
    pub evicted: u64,
}

/// The retained quads of the text nodes painted last frame.
#[derive(Default)]
pub struct PaintCache {
    text: HashMap<NodeIx, TextEntry>,
    generation: u32,
    quads: usize,
    stats: PaintStats,
}

impl PaintCache {
    /// How many quads the cache may hold before it is emptied rather than
    /// grown: a few dialogs' worth of glyphs, seven megabytes.
    pub const MAX_QUADS: usize = 65_536;

    /// An empty cache.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Forget everything: a theme, a scale, a font changed, and every
    /// node paints differently.
    pub fn clear(&mut self) {
        self.stats.evicted = self.stats.evicted.saturating_add(self.text.len() as u64);
        self.text.clear();
        self.quads = 0;
    }

    /// What the cache did so far.
    #[must_use]
    pub fn stats(&self) -> PaintStats {
        self.stats
    }

    /// How many text nodes are held.
    #[must_use]
    pub fn len(&self) -> usize {
        self.text.len()
    }

    /// True when nothing is held.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    /// A paint begins.
    pub(crate) fn begin(&mut self) {
        self.generation = self.generation.wrapping_add(1);
    }

    /// A paint ended: what it did not use is dropped, and a cache past
    /// its size is emptied.
    pub(crate) fn end(&mut self) {
        let generation = self.generation;
        let before = self.text.len();
        self.text.retain(|_, e| e.seen == generation);
        self.stats.evicted = self.stats.evicted.saturating_add((before - self.text.len()) as u64);
        self.quads = self.text.values().map(|e| e.quads.len()).sum();
        if self.quads > Self::MAX_QUADS {
            self.clear();
        }
    }

    /// The node's quads from last frame, if they are still right under
    /// `key` -- moved into place when the box moved by whole pixels. The
    /// caller pushes them and hands them back with [`Self::restore`].
    pub(crate) fn take(&mut self, ix: NodeIx, key: &TextKey) -> Option<Vec<Quad>> {
        let e = self.text.get_mut(&ix)?;
        if e.key == *key {
            self.stats.hits = self.stats.hits.saturating_add(1);
        } else {
            let (dx, dy) = key.translation_from(&e.key)?;
            for q in &mut e.quads {
                q.rect[0] += dx;
                q.rect[1] += dy;
            }
            e.key = *key;
            self.stats.translated = self.stats.translated.saturating_add(1);
        }
        e.seen = self.generation;
        Some(std::mem::take(&mut e.quads))
    }

    /// The quads [`Self::take`] lent, back where they were.
    pub(crate) fn restore(&mut self, ix: NodeIx, quads: Vec<Quad>) {
        if let Some(e) = self.text.get_mut(&ix) {
            e.quads = quads;
        }
    }

    /// Quads built afresh this frame, for the next.
    pub(crate) fn keep(&mut self, ix: NodeIx, key: TextKey, quads: Vec<Quad>) {
        self.stats.rebuilt = self.stats.rebuilt.saturating_add(1);
        self.text.insert(ix, TextEntry { key, quads, seen: self.generation });
    }
}
