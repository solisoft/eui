//! A glyph atlas: one R8 texture, shelf-packed, grown once.

use std::collections::HashMap;

use eui_text::{GlyphKey, GlyphRef, TextEngine};

/// Where a glyph lives in the atlas, in texels, plus its placement.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Region {
    /// Left texel.
    pub x: u32,
    /// Top texel.
    pub y: u32,
    /// Width in texels.
    pub w: u32,
    /// Height in texels.
    pub h: u32,
    /// Bitmap offset from the glyph origin, device px.
    pub left: i32,
    /// Bitmap offset up from the baseline, device px.
    pub top: i32,
}

/// Coverage bitmaps packed into one square texture.
#[derive(Debug)]
pub struct Atlas {
    size: u32,
    pixels: Vec<u8>,
    shelves: Vec<(u32, u32, u32)>, // (y, height, next x)
    next_y: u32,
    map: HashMap<(GlyphKey, u32), Option<Region>>,
    /// Bands of rows `y0..y1` written since the last upload, empty when
    /// clean. A shelf packer touches a shelf at a time, and two glyphs on
    /// shelves far apart used to cost every row between them; a few bands
    /// cost the rows they hold. Kept short: past [`Self::MAX_BANDS`] the
    /// two nearest are merged.
    dirty: Vec<(u32, u32)>,
    /// Bumped whenever every packed glyph moves or goes -- a grow, a new
    /// size from the other side of a pipe -- so a quad built from a
    /// region can tell it is stale.
    generation: u32,
}

impl Atlas {
    /// The initial edge length; the atlas grows to twice this once.
    pub const INITIAL: u32 = 1024;

    /// An empty atlas.
    pub fn new() -> Self {
        Self::with_size(Self::INITIAL)
    }

    /// How many dirty bands are kept apart before the nearest two merge.
    pub const MAX_BANDS: usize = 4;

    /// A fresh atlas is clean: a texture is made blank, and so is the
    /// bitmap of the process it is sent to, so nothing is owed until a
    /// glyph is packed. Uploading a megabyte of nothing was the first
    /// frame's largest write.
    fn with_size(size: u32) -> Self {
        Self { size, pixels: vec![0; (size * size) as usize], shelves: Vec::new(), next_y: 0, map: HashMap::new(), dirty: Vec::new(), generation: 0 }
    }

    /// Which arrangement of glyphs this is: a region taken under one
    /// generation is wrong under the next.
    pub fn generation(&self) -> u32 {
        self.generation
    }

    /// Take rows `y0..y1` of a `size × size` coverage bitmap: a window
    /// process receiving what a worker rasterised (`eui-client`'s process
    /// boundary). A new `size` starts a blank bitmap first. Refused, and
    /// nothing changes, unless `rows` is exactly those rows.
    pub fn set_rows(&mut self, size: u32, y0: u32, y1: u32, rows: &[u8]) -> bool {
        if size == 0 || y1 > size || y0 >= y1 || rows.len() != ((y1 - y0) as usize).saturating_mul(size as usize) {
            return false;
        }
        if size != self.size {
            self.size = size;
            self.pixels = vec![0; (size as usize).saturating_mul(size as usize)];
            self.map.clear();
            self.shelves.clear();
            self.next_y = 0;
            self.dirty.clear();
            self.generation = self.generation.wrapping_add(1);
        }
        let start = (y0 as usize).saturating_mul(size as usize);
        if let Some(dst) = self.pixels.get_mut(start..start.saturating_add(rows.len())) {
            dst.copy_from_slice(rows);
        }
        self.touch(y0, y1);
        true
    }

    /// Rows written since the last upload, `y0..y1`, as one band from the
    /// first to the last; `None` when clean. [`Self::dirty_bands`] is the
    /// same without the rows between.
    pub fn dirty_rows(&self) -> Option<(u32, u32)> {
        let lo = self.dirty.iter().map(|b| b.0).min()?;
        let hi = self.dirty.iter().map(|b| b.1).max()?;
        Some((lo, hi))
    }

    /// The bands of rows written since the last upload, in order.
    pub fn dirty_bands(&self) -> &[(u32, u32)] {
        &self.dirty
    }

    /// The bytes of rows `y0..y1`.
    pub fn rows(&self, y0: u32, y1: u32) -> &[u8] {
        let (a, b) = ((y0 as usize).saturating_mul(self.size as usize), (y1 as usize).saturating_mul(self.size as usize));
        self.pixels.get(a..b).unwrap_or(&[])
    }

    fn touch(&mut self, y0: u32, y1: u32) {
        if y0 >= y1 {
            return;
        }
        // Into a band it meets or overlaps, else a band of its own; then
        // neighbours that came to meet are joined.
        match self.dirty.iter_mut().find(|b| y0 <= b.1 && y1 >= b.0) {
            Some(b) => {
                b.0 = b.0.min(y0);
                b.1 = b.1.max(y1);
            }
            None => self.dirty.push((y0, y1)),
        }
        self.dirty.sort_unstable();
        let mut merged: Vec<(u32, u32)> = Vec::with_capacity(self.dirty.len());
        for &(lo, hi) in &self.dirty {
            match merged.last_mut() {
                Some(last) if lo <= last.1 => last.1 = last.1.max(hi),
                _ => merged.push((lo, hi)),
            }
        }
        // Too many: the two with the least between them become one.
        while merged.len() > Self::MAX_BANDS {
            let gap = |i: usize| merged.get(i + 1).zip(merged.get(i)).map_or(u32::MAX, |(n, c)| n.0.saturating_sub(c.1));
            let Some(i) = (0..merged.len() - 1).min_by_key(|i| gap(*i)) else { break };
            let next = merged.remove(i + 1);
            if let Some(cur) = merged.get_mut(i) {
                cur.1 = cur.1.max(next.1);
            }
        }
        self.dirty = merged;
    }

    /// Edge length in texels.
    pub fn size(&self) -> u32 {
        self.size
    }

    /// The R8 texel data, row-major.
    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }

    /// True when the texture must be re-uploaded; cleared by [`Self::mark_clean`].
    pub fn is_dirty(&self) -> bool {
        !self.dirty.is_empty()
    }

    /// Acknowledge an upload.
    pub fn mark_clean(&mut self) {
        self.dirty.clear();
    }

    /// Every row is owed again: the texture these glyphs were uploaded to
    /// is gone. The two atlas textures share a bind group, so growing the
    /// image texture remakes this one blank under a client that still
    /// believes its glyphs are on the GPU.
    pub fn mark_dirty_all(&mut self) {
        self.dirty.clear();
        if !self.pixels.is_empty() {
            self.dirty.push((0, self.size));
        }
    }

    /// Glyphs currently packed.
    pub fn len(&self) -> usize {
        self.map.values().filter(|r| r.is_some()).count()
    }

    /// True when nothing is packed.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Find or rasterise-and-pack a glyph at a device scale. `None` for a
    /// glyph with no image (a space) or one that does not fit even after
    /// growing — the latter is drawn as nothing rather than crashing.
    pub fn get(&mut self, text: &mut TextEngine, key: GlyphKey, scale: f32) -> Option<Region> {
        let k = (key, scale.to_bits());
        if let Some(r) = self.map.get(&k) {
            return *r;
        }
        // Rasterised once, and packed straight from the engine's bitmap.
        // A colour glyph -- an emoji -- is not coverage and is not packed
        // here; it must not make the atlas grow for nothing either.
        let (region, could_fit) = text
            .with_glyph(key, scale, |g| {
                let drawable = g.width > 0 && g.height > 0 && !g.color;
                (if drawable { self.pack(g) } else { None }, drawable)
            })
            .unwrap_or((None, false));
        if region.is_none() && could_fit && self.size < Self::INITIAL * 2 {
            self.grow();
            return self.get(text, key, scale);
        }
        self.map.insert(k, region);
        region
    }

    fn pack(&mut self, img: GlyphRef<'_>) -> Option<Region> {
        // One texel of padding on every side stops bilinear bleed.
        let w = img.width.checked_add(2)?;
        let h = img.height.checked_add(2)?;
        if w > self.size || h > self.size {
            return None;
        }
        let shelf = self.shelves.iter().position(|(_, sh, nx)| *sh >= h && nx.saturating_add(w) <= self.size);
        let (x, y) = match shelf {
            Some(i) => {
                let s = self.shelves.get_mut(i)?;
                let x = s.2;
                s.2 = x.saturating_add(w);
                (x, s.0)
            }
            None => {
                if self.next_y.saturating_add(h) > self.size {
                    return None;
                }
                let y = self.next_y;
                self.shelves.push((y, h, w));
                self.next_y = y.saturating_add(h);
                (0, y)
            }
        };
        for row in 0..img.height {
            let src = (row * img.width) as usize;
            let dst = ((y + 1 + row) * self.size + x + 1) as usize;
            let (Some(s), Some(d)) = (img.data.get(src..src + img.width as usize), self.pixels.get_mut(dst..dst + img.width as usize)) else {
                return None;
            };
            d.copy_from_slice(s);
        }
        self.touch(y, y.saturating_add(h).min(self.size));
        Some(Region { x: x + 1, y: y + 1, w: img.width, h: img.height, left: img.left, top: img.top })
    }

    /// Forget every glyph, keeping the size.
    ///
    /// The map is keyed by scale as well as by glyph, and nothing here has
    /// ever been evicted — which was right while the scale only changed
    /// when a window was dragged between two monitors. A zoom makes it a
    /// keystroke, and thirteen levels of a text-heavy page would fill the
    /// sheet; a full one packs nothing more and draws those glyphs as
    /// nothing, silently. Cleared on a rescale instead, beside the layout
    /// and the paint cache that are already thrown away there.
    ///
    /// The generation goes up, so a uv taken before this is not mistaken
    /// for one taken after. Nothing is marked dirty: as after [`Self::grow`],
    /// the rows still on the GPU belong to nobody, and the glyphs packed
    /// next will dirty the rows they land on.
    pub fn clear(&mut self) {
        let generation = self.generation.wrapping_add(1);
        let size = self.size;
        *self = Self::with_size(size);
        self.generation = generation;
    }

    /// Double the edge length, forgetting every packed glyph; they are
    /// re-packed on demand.
    fn grow(&mut self) {
        let size = self.size.saturating_mul(2);
        let generation = self.generation.wrapping_add(1);
        *self = Self::with_size(size);
        self.generation = generation;
    }
}

impl Default for Atlas {
    fn default() -> Self {
        Self::new()
    }
}

/// Decoded images packed into one RGBA8 texture, shelf-packed like the glyph
/// atlas. An image is uploaded once and referenced by content hash.
#[derive(Debug)]
pub struct ImageAtlas {
    size: u32,
    pixels: Vec<u8>,
    shelves: Vec<(u32, u32, u32)>,
    next_y: u32,
    map: HashMap<[u8; 32], Option<Region>>,
    /// Rows written since the last upload, `None` when clean.
    dirty: Option<(u32, u32)>,
}

impl ImageAtlas {
    /// Edge length in texels.
    pub const SIZE: u32 = 2048;

    /// An empty atlas.
    ///
    /// The 16 MiB of texels is not allocated here. Most applications never
    /// show a picture, and one that does not should not carry the buffer —
    /// nor the matching texture, which is 16 MiB of GPU memory that no
    /// budget in `spec/10-budgets.md` currently counts. The first write
    /// allocates; until then this atlas is a few dozen bytes.
    pub fn new() -> Self {
        Self { size: Self::SIZE, pixels: Vec::new(), shelves: Vec::new(), next_y: 0, map: HashMap::new(), dirty: None }
    }

    /// True once the texels exist, i.e. once anything has been packed. The
    /// renderer asks so it can leave the texture unmade.
    pub fn is_empty(&self) -> bool {
        self.pixels.is_empty()
    }

    /// Allocate the texels on the first write.
    fn ensure(&mut self) {
        if self.pixels.is_empty() {
            self.pixels = vec![0; (self.size as usize).saturating_mul(self.size as usize).saturating_mul(4)];
        }
    }

    /// Take rows `y0..y1` of RGBA texels; refused, and nothing changes,
    /// unless `rows` is exactly those rows.
    pub fn set_rows(&mut self, y0: u32, y1: u32, rows: &[u8]) -> bool {
        if y1 > self.size || y0 >= y1 || rows.len() != ((y1 - y0) as usize).saturating_mul(self.size as usize).saturating_mul(4) {
            return false;
        }
        self.ensure();
        let start = (y0 as usize).saturating_mul(self.size as usize).saturating_mul(4);
        if let Some(dst) = self.pixels.get_mut(start..start.saturating_add(rows.len())) {
            dst.copy_from_slice(rows);
        }
        self.touch(y0, y1);
        true
    }

    /// Rows written since the last upload, `y0..y1`; `None` when clean.
    pub fn dirty_rows(&self) -> Option<(u32, u32)> {
        self.dirty
    }

    /// The bytes of rows `y0..y1`.
    pub fn rows(&self, y0: u32, y1: u32) -> &[u8] {
        let (a, b) = ((y0 as usize).saturating_mul(self.size as usize).saturating_mul(4), (y1 as usize).saturating_mul(self.size as usize).saturating_mul(4));
        self.pixels.get(a..b).unwrap_or(&[])
    }

    fn touch(&mut self, y0: u32, y1: u32) {
        self.dirty = Some(match self.dirty {
            Some((a, b)) => (a.min(y0), b.max(y1)),
            None => (y0, y1),
        });
    }

    /// Edge length in texels.
    pub fn size(&self) -> u32 {
        self.size
    }

    /// RGBA8 texels, row-major.
    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }

    /// True when the texture must be re-uploaded.
    pub fn is_dirty(&self) -> bool {
        self.dirty.is_some()
    }

    /// Acknowledge an upload.
    pub fn mark_clean(&mut self) {
        self.dirty = None;
    }

    /// Force a re-upload (the texture was recreated).
    pub fn mark_dirty_all(&mut self) {
        self.dirty = Some((0, self.size));
    }

    /// Where an image lives, if it was packed.
    pub fn get(&self, hash: &[u8; 32]) -> Option<Region> {
        self.map.get(hash).copied().flatten()
    }

    /// Pack an image. `None` when it does not fit; the hash is remembered
    /// either way so a failure is not retried every frame.
    pub fn insert(&mut self, hash: [u8; 32], width: u32, height: u32, rgba: &[u8]) -> Option<Region> {
        if let Some(r) = self.map.get(&hash) {
            return *r;
        }
        let region = self.pack(width, height, rgba);
        self.map.insert(hash, region);
        region
    }

    /// Rewrite the pixels of a hash already packed, same size — a video's
    /// next frame. `false` when the hash is unknown or the bytes are the
    /// wrong length; nothing changes then.
    pub fn update(&mut self, hash: &[u8; 32], rgba: &[u8]) -> bool {
        let Some(Some(region)) = self.map.get(hash).copied() else { return false };
        let row_bytes = (region.w as usize).saturating_mul(4);
        if rgba.len() != row_bytes.saturating_mul(region.h as usize) {
            return false;
        }
        for row in 0..region.h {
            let src = (row as usize).saturating_mul(row_bytes);
            let dst = (((region.y + row) * self.size + region.x) * 4) as usize;
            let (Some(s), Some(d)) = (rgba.get(src..src + row_bytes), self.pixels.get_mut(dst..dst + row_bytes)) else {
                return false;
            };
            d.copy_from_slice(s);
        }
        self.touch(region.y, region.y.saturating_add(region.h).min(self.size));
        true
    }

    fn pack(&mut self, width: u32, height: u32, rgba: &[u8]) -> Option<Region> {
        if rgba.len() != (width as usize).checked_mul(height as usize)?.checked_mul(4)? {
            return None;
        }
        // The one place a picture first reaches this atlas, so the one
        // place the texels have to exist by. `update` only rewrites a
        // region this already packed.
        self.ensure();
        let w = width.checked_add(2)?;
        let h = height.checked_add(2)?;
        if w > self.size || h > self.size {
            return None;
        }
        let shelf = self.shelves.iter().position(|(_, sh, nx)| *sh >= h && nx.saturating_add(w) <= self.size);
        let (x, y) = match shelf {
            Some(i) => {
                let s = self.shelves.get_mut(i)?;
                let x = s.2;
                s.2 = x.saturating_add(w);
                (x, s.0)
            }
            None => {
                if self.next_y.saturating_add(h) > self.size {
                    return None;
                }
                let y = self.next_y;
                self.shelves.push((y, h, w));
                self.next_y = y.saturating_add(h);
                (0, y)
            }
        };
        let row_bytes = width as usize * 4;
        for row in 0..height {
            let src = row as usize * row_bytes;
            let dst = (((y + 1 + row) * self.size + x + 1) * 4) as usize;
            let (Some(s), Some(d)) = (rgba.get(src..src + row_bytes), self.pixels.get_mut(dst..dst + row_bytes)) else {
                return None;
            };
            d.copy_from_slice(s);
        }
        self.touch(y, y.saturating_add(h).min(self.size));
        Some(Region { x: x + 1, y: y + 1, w: width, h: height, left: 0, top: 0 })
    }
}

impl Default for ImageAtlas {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_image_atlas_costs_nothing_until_it_holds_a_picture() {
        // The point of the laziness: 2048² RGBA is 16 MiB of buffer and a
        // matching 16 MiB of GPU texture, and most applications never show
        // a picture. A fresh atlas must therefore hold no texels and ask
        // for no upload.
        let mut atlas = ImageAtlas::new();
        assert!(atlas.is_empty(), "a fresh image atlas holds no texels");
        assert_eq!(atlas.dirty_rows(), None, "and so has nothing to upload");
        assert!(atlas.pixels().is_empty());

        // Packing one is what allocates, and it reports rows to upload.
        let rgba = vec![255u8; 8 * 8 * 4];
        let region = atlas.insert([7; 32], 8, 8, &rgba);
        assert!(region.is_some(), "an 8x8 picture packs");
        assert!(!atlas.is_empty(), "packing allocates the texels");
        assert!(atlas.dirty_rows().is_some(), "and marks rows for upload");
        assert_eq!(atlas.pixels().len(), (ImageAtlas::SIZE as usize).pow(2) * 4);

        // The same hash again is the cached region, not a second pack.
        assert_eq!(atlas.insert([7; 32], 8, 8, &rgba), region);
    }

    #[test]
    fn a_fresh_atlas_is_clean_and_far_apart_writes_are_separate_bands() {
        let mut atlas = Atlas::new();
        assert_eq!(atlas.dirty_rows(), None, "nothing to upload until a glyph is packed");
        assert!(!atlas.is_dirty());
        atlas.touch(10, 20);
        atlas.touch(900, 910);
        assert_eq!(atlas.dirty_bands(), &[(10, 20), (900, 910)], "two bands, not the 900 rows between");
        assert_eq!(atlas.dirty_rows(), Some((10, 910)), "as one band, for whoever still wants one");
        atlas.touch(20, 25);
        atlas.touch(905, 908);
        assert_eq!(atlas.dirty_bands(), &[(10, 25), (900, 910)], "a write that meets a band joins it");
        for y in [100, 300, 500, 700] {
            atlas.touch(y, y + 4);
        }
        assert!(atlas.dirty_bands().len() <= Atlas::MAX_BANDS, "{:?}", atlas.dirty_bands());
        atlas.mark_clean();
        assert!(atlas.dirty_bands().is_empty());
    }

    #[test]
    fn rows_of_an_unallocated_atlas_are_empty_rather_than_a_panic() {
        // `sync_atlas` asks for rows before anything is packed on the very
        // first frame; an empty slice is the right answer, not an index out
        // of a zero-length buffer.
        let atlas = ImageAtlas::new();
        assert!(atlas.rows(0, 4).is_empty());
    }
}
