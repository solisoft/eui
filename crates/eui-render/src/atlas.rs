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

    /// The largest edge length: grown to once, then emptied when full.
    pub const MAX: u32 = Self::INITIAL * 2;

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
    /// glyph with no image (a space) or one larger than an empty sheet of
    /// the largest size — the latter is drawn as nothing rather than
    /// crashing.
    ///
    /// A full sheet makes room. Once it has grown to its largest, a glyph
    /// that did not fit used to be remembered as `None` and drawn as
    /// nothing until the next rescale — a long CJK session simply stopped
    /// showing new characters. It now empties the sheet and packs the
    /// glyph into it, as [`ImageAtlas::insert`] does for pictures: the
    /// generation goes up, so every quad built from the old arrangement is
    /// known to be stale (the paint cache keys on it), and the glyphs still
    /// on screen are packed again as the next paint asks for them.
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
        if region.is_none() && could_fit {
            if self.size < Self::MAX {
                self.grow();
                return self.get(text, key, scale);
            }
            // Only from a sheet that holds something: an empty one that
            // still refuses the glyph would refuse it again, for ever.
            if !self.shelves.is_empty() {
                self.clear();
                return self.get(text, key, scale);
            }
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
    /// Rectangles `[x, y, w, h]` written since the last upload, empty when
    /// clean.
    ///
    /// Rectangles and not rows. The sheet is 2048 texels across and four
    /// bytes a texel, so a row is 8 KiB whatever wrote to it: a 320×240
    /// video dirtied 240 of them, about 1.9 MB a frame for 300 KB of
    /// picture, and two videos at opposite ends of the sheet merged into
    /// one band that could be the whole 16 MiB -- copied into the pipe,
    /// out of it, and into the GPU's staging buffer, every frame. A
    /// rectangle costs what was drawn into it. Kept short: past
    /// [`Self::MAX_REGIONS`] the two whose union wastes least are merged.
    dirty: Vec<[u32; 4]>,
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
        Self { size: Self::SIZE, pixels: Vec::new(), shelves: Vec::new(), next_y: 0, map: HashMap::new(), dirty: Vec::new() }
    }

    /// How many dirty rectangles are kept apart before two are merged.
    pub const MAX_REGIONS: usize = 8;

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

    /// Take the RGBA texels of the rectangle `[x, y, w, h]`, tightly
    /// packed: a window process receiving what a worker packed. Refused,
    /// and nothing changes, unless the rectangle lies inside the sheet and
    /// `px` is exactly its bytes -- the worker is the untrusted side of
    /// that pipe.
    pub fn set_region(&mut self, r: [u32; 4], px: &[u8]) -> bool {
        let [x, y, w, h] = r;
        let fits = w > 0 && h > 0 && x.checked_add(w).is_some_and(|e| e <= self.size) && y.checked_add(h).is_some_and(|e| e <= self.size);
        if !fits || px.len() != (w as usize).saturating_mul(h as usize).saturating_mul(4) {
            return false;
        }
        self.ensure();
        let row = (w as usize) * 4;
        for (i, src) in px.chunks_exact(row).enumerate() {
            let at = ((y as usize + i) * self.size as usize + x as usize) * 4;
            if let Some(dst) = self.pixels.get_mut(at..at + row) {
                dst.copy_from_slice(src);
            }
        }
        self.touch(r);
        true
    }

    /// The rectangles `[x, y, w, h]` written since the last upload.
    pub fn dirty_regions(&self) -> &[[u32; 4]] {
        &self.dirty
    }

    /// The texels of a rectangle, tightly packed row by row: what crosses
    /// the pipe for it. Empty for one that is not inside the sheet, or
    /// before anything was packed.
    pub fn region_bytes(&self, r: [u32; 4]) -> Vec<u8> {
        let [x, y, w, h] = r;
        let inside = x.checked_add(w).is_some_and(|e| e <= self.size) && y.checked_add(h).is_some_and(|e| e <= self.size);
        if !inside || self.pixels.is_empty() {
            return Vec::new();
        }
        let row = (w as usize) * 4;
        let mut out = Vec::with_capacity(row * h as usize);
        for i in 0..h as usize {
            let at = ((y as usize + i) * self.size as usize + x as usize) * 4;
            out.extend_from_slice(self.pixels.get(at..at + row).unwrap_or(&[]));
        }
        out
    }

    /// Bytes the dirty rectangles hold, which is what the next upload costs.
    pub fn dirty_bytes(&self) -> usize {
        self.dirty.iter().map(|r| (r[2] as usize) * (r[3] as usize) * 4).sum()
    }

    fn touch(&mut self, r: [u32; 4]) {
        if r[2] == 0 || r[3] == 0 {
            return;
        }
        let union = |a: [u32; 4], b: [u32; 4]| {
            let (x0, y0) = (a[0].min(b[0]), a[1].min(b[1]));
            let (x1, y1) = ((a[0] + a[2]).max(b[0] + b[2]), (a[1] + a[3]).max(b[1] + b[3]));
            [x0, y0, x1 - x0, y1 - y0]
        };
        // Meeting or overlapping: one rectangle. A video's next frame lands
        // exactly on the last one's and costs nothing extra.
        let meets = |a: [u32; 4], b: [u32; 4]| a[0] <= b[0] + b[2] && b[0] <= a[0] + a[2] && a[1] <= b[1] + b[3] && b[1] <= a[1] + a[3];
        let mut r = r;
        while let Some(i) = self.dirty.iter().position(|d| meets(*d, r)) {
            r = union(self.dirty.swap_remove(i), r);
        }
        self.dirty.push(r);
        // Too many: the pair whose union adds the fewest texels becomes one.
        let area = |a: [u32; 4]| u64::from(a[2]) * u64::from(a[3]);
        while self.dirty.len() > Self::MAX_REGIONS {
            let mut best = (u64::MAX, 0, 1);
            for (i, &a) in self.dirty.iter().enumerate() {
                for (j, &b) in self.dirty.iter().enumerate().skip(i + 1) {
                    let waste = area(union(a, b)).saturating_sub(area(a) + area(b));
                    if waste < best.0 {
                        best = (waste, i, j);
                    }
                }
            }
            let b = self.dirty.swap_remove(best.2);
            let a = self.dirty.swap_remove(best.1);
            self.touch(union(a, b));
        }
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
        !self.dirty.is_empty()
    }

    /// Acknowledge an upload.
    pub fn mark_clean(&mut self) {
        self.dirty.clear();
    }

    /// Force a re-upload (the texture was recreated).
    pub fn mark_dirty_all(&mut self) {
        self.dirty.clear();
        self.dirty.push([0, 0, self.size, self.size]);
    }

    /// Where an image lives, if it was packed.
    pub fn get(&self, hash: &[u8; 32]) -> Option<Region> {
        self.map.get(hash).copied().flatten()
    }

    /// Forget everything packed. The texels stay allocated, as they were;
    /// what is still on screen is packed again by whoever notices it is
    /// missing.
    ///
    /// This exists because the sheet is one 2048² texture and it fills: six
    /// photographs of 800×600 are enough. Without a way to start again, the
    /// seventh picture of a session was never drawn — not an error, not a
    /// failed fetch, simply absent, and a different set of absences on
    /// every load.
    ///
    /// Nothing is zeroed and nothing is owed. This used to blank all 16 MiB
    /// and mark the whole sheet for upload -- across the pipe and into the
    /// texture, for pixels no quad would ever sample again. The old texels
    /// belong to nobody now; `pack` writes its whole padded rectangle,
    /// border included, so a stale neighbour cannot bleed into a new
    /// picture, and it dirties exactly that rectangle.
    pub fn clear(&mut self) {
        self.map.clear();
        self.shelves.clear();
        self.next_y = 0;
        self.dirty.clear();
    }

    /// Pack an image. `None` when it does not fit; the hash is remembered
    /// either way so a failure is not retried every frame.
    ///
    /// "Does not fit" means *in an empty sheet*: a picture that is simply
    /// larger than the texture is refused for good, and one that would fit
    /// on its own empties the sheet and takes it. The cost of emptying is
    /// one re-upload and packing again whatever is still on screen; the
    /// cost of not doing it was a picture that never appeared.
    pub fn insert(&mut self, hash: [u8; 32], width: u32, height: u32, rgba: &[u8]) -> Option<Region> {
        if let Some(r) = self.map.get(&hash) {
            return *r;
        }
        let mut region = self.pack(width, height, rgba);
        if region.is_none() && !self.map.is_empty() && width + 2 <= self.size && height + 2 <= self.size {
            self.clear();
            region = self.pack(width, height, rgba);
        }
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
        self.touch([region.x, region.y, region.w, region.h]);
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
        // The one-texel border is written too, as transparent: after a
        // `clear` the sheet still holds whatever was there before, and a
        // bilinear tap at the picture's edge would otherwise pick it up.
        let padded = w as usize * 4;
        for row in 0..h {
            let at = (((y + row) * self.size + x) * 4) as usize;
            if let Some(d) = self.pixels.get_mut(at..at + padded) {
                d.fill(0);
            }
        }
        let row_bytes = width as usize * 4;
        for row in 0..height {
            let src = row as usize * row_bytes;
            let dst = (((y + 1 + row) * self.size + x + 1) * 4) as usize;
            let (Some(s), Some(d)) = (rgba.get(src..src + row_bytes), self.pixels.get_mut(dst..dst + row_bytes)) else {
                return None;
            };
            d.copy_from_slice(s);
        }
        self.touch([x, y, w, h]);
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

    /// A region that must be there, without a panic the lints forbid.
    fn got(r: Option<Region>) -> Region {
        assert!(r.is_some(), "expected a region");
        r.unwrap_or(Region { x: 0, y: 0, w: 0, h: 0, left: 0, top: 0 })
    }

    #[test]
    fn an_image_atlas_costs_nothing_until_it_holds_a_picture() {
        // The point of the laziness: 2048² RGBA is 16 MiB of buffer and a
        // matching 16 MiB of GPU texture, and most applications never show
        // a picture. A fresh atlas must therefore hold no texels and ask
        // for no upload.
        let mut atlas = ImageAtlas::new();
        assert!(atlas.is_empty(), "a fresh image atlas holds no texels");
        assert!(atlas.dirty_regions().is_empty(), "and so has nothing to upload");
        assert!(atlas.pixels().is_empty());

        // Packing one is what allocates, and it reports rows to upload.
        let rgba = vec![255u8; 8 * 8 * 4];
        let region = atlas.insert([7; 32], 8, 8, &rgba);
        assert!(region.is_some(), "an 8x8 picture packs");
        assert!(!atlas.is_empty(), "packing allocates the texels");
        assert_eq!(atlas.dirty_regions(), &[[0, 0, 10, 10]], "and marks its padded rectangle for upload, not its rows");
        assert_eq!(atlas.pixels().len(), (ImageAtlas::SIZE as usize).pow(2) * 4);

        // The same hash again is the cached region, not a second pack.
        assert_eq!(atlas.insert([7; 32], 8, 8, &rgba), region);
    }

    #[test]
    fn a_full_sheet_empties_itself_rather_than_refusing_for_ever() {
        // One 2048² sheet holds six photographs of 800×600 — two to a
        // shelf, three shelves — and the seventh used to be refused and
        // the refusal remembered, so it was never drawn again. In a mail
        // client with seventeen pictures attached, six appeared.
        let edge = 1500;
        let rgba = vec![200u8; (edge as usize).pow(2) * 4];
        let mut atlas = ImageAtlas::new();

        let first = atlas.insert([1; 32], edge, edge, &rgba);
        assert!(first.is_some(), "the first fits");

        // The second cannot share the shelf and cannot start another, so
        // the sheet empties and takes it.
        let second = atlas.insert([2; 32], edge, edge, &rgba);
        assert!(second.is_some(), "the second fits, in an emptied sheet");
        assert_eq!(atlas.get(&[1; 32]), None, "and the first is gone");

        // Something larger than the sheet itself is still refused, and the
        // sheet is left alone.
        let huge = ImageAtlas::SIZE + 10;
        let big = vec![0u8; (huge as usize) * 4];
        assert_eq!(atlas.insert([3; 32], huge, 1, &big), None);
        assert!(atlas.get(&[2; 32]).is_some(), "a refusal costs nothing");
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
    fn a_full_glyph_sheet_empties_itself_and_the_next_glyph_is_still_drawn() {
        // A sheet at its largest with no room left: one full shelf. Before,
        // the next glyph was remembered as `None` and never drawn until a
        // rescale.
        let mut text = TextEngine::new();
        let font = eui_layout::FontSpec { family: eui_proto::FontFamily::Sans, weight: eui_proto::FontWeight::Regular, size: 15.0, line_height: 22.0 };
        let first = |text: &mut TextEngine, font| text.shape("H", font, None, 0).glyphs.first().map(|g| g.key);
        let key = first(&mut text, font);
        assert!(key.is_some(), "an H is a glyph");
        let Some(key) = key else { return };
        let mut atlas = Atlas::with_size(Atlas::MAX);
        atlas.shelves.push((0, Atlas::MAX, Atlas::MAX));
        atlas.next_y = Atlas::MAX;
        let before = atlas.generation();

        let region = atlas.get(&mut text, key, 1.0);
        assert!(region.is_some(), "the glyph is packed into the emptied sheet");
        assert_ne!(atlas.generation(), before, "and every region taken before is known to be stale");
        assert_eq!(atlas.size(), Atlas::MAX, "without growing past the largest size");
        assert_eq!(atlas.get(&mut text, key, 1.0), region, "and it is found there next time");

        // A glyph too big for even an empty sheet is refused once, without
        // emptying the sheet over and over.
        let Some(huge) = first(&mut text, eui_layout::FontSpec { size: 4000.0, line_height: 4000.0, ..font }) else { return };
        let generation = atlas.generation();
        assert_eq!(atlas.get(&mut text, huge, 1.0), None);
        assert!(atlas.generation().wrapping_sub(generation) <= 1, "one clear at most, not a loop");
    }

    #[test]
    fn rows_of_an_unallocated_atlas_are_empty_rather_than_a_panic() {
        // `sync_atlas` asks for rows before anything is packed on the very
        // first frame; an empty slice is the right answer, not an index out
        // of a zero-length buffer.
        let atlas = ImageAtlas::new();
        assert!(atlas.region_bytes([0, 0, 4, 4]).is_empty());
    }

    #[test]
    fn two_pictures_at_opposite_ends_of_the_sheet_cost_their_own_texels() {
        // Two 8×8 videos, one at each end of the same shelf. Tracked as
        // rows, their next frames dirtied the whole 2048-texel width of
        // those rows: 64 KiB for 512 bytes of picture. As rectangles they
        // cost what they are.
        let mut atlas = ImageAtlas::new();
        let px = |v: u8, w: u32, h: u32| vec![v; (w * h * 4) as usize];
        let a = got(atlas.insert([1; 32], 8, 8, &px(1, 8, 8)));
        got(atlas.insert([2; 32], 2000, 8, &px(2, 2000, 8)));
        let z = got(atlas.insert([3; 32], 8, 8, &px(3, 8, 8)));
        assert_eq!((a.x, a.y), (1, 1));
        assert!(z.x > 2000 && z.y == 1, "z is at the far end of the same rows: {z:?}");
        atlas.mark_clean();

        assert!(atlas.update(&[1; 32], &px(9, 8, 8)));
        assert!(atlas.update(&[3; 32], &px(9, 8, 8)));
        assert_eq!(atlas.dirty_regions().len(), 2, "two rectangles, not one band: {:?}", atlas.dirty_regions());
        assert_eq!(atlas.dirty_bytes(), 2 * 8 * 8 * 4, "the bytes of two pictures, and nothing between them");

        // And the same across the height: two rectangles, not every row
        // between them.
        atlas.mark_clean();
        atlas.touch([0, 0, 4, 4]);
        atlas.touch([2044, 2044, 4, 4]);
        assert_eq!(atlas.dirty_regions().len(), 2);
        assert_eq!(atlas.dirty_bytes(), 2 * 4 * 4 * 4);

        // A frame written again over the last one's rectangle is free.
        atlas.touch([0, 0, 4, 4]);
        assert_eq!(atlas.dirty_bytes(), 2 * 4 * 4 * 4);

        // What crosses the pipe is the rectangle's texels, tightly packed,
        // and a second atlas that takes them holds the same picture.
        atlas.mark_clean();
        assert!(atlas.update(&[3; 32], &px(7, 8, 8)));
        let mut window = ImageAtlas::new();
        for &r in atlas.dirty_regions() {
            let bytes = atlas.region_bytes(r);
            assert_eq!(bytes.len(), (r[2] * r[3] * 4) as usize);
            assert!(window.set_region(r, &bytes));
        }
        assert_eq!(window.region_bytes([z.x, z.y, 8, 8]), px(7, 8, 8));
        assert!(!window.set_region([2047, 0, 2, 1], &[0; 8]), "a rectangle off the sheet is refused");
        assert!(!window.set_region([0, 0, 2, 2], &[0; 15]), "and so are the wrong number of bytes");
    }

    #[test]
    fn a_cleared_sheet_owes_no_upload_and_repacks_with_a_clean_border() {
        // Clearing used to blank all 16 MiB and mark the whole sheet dirty:
        // one full transfer for pixels nothing would sample again.
        let mut atlas = ImageAtlas::new();
        got(atlas.insert([1; 32], 30, 30, &vec![255; 30 * 30 * 4]));
        atlas.mark_clean();
        atlas.clear();
        assert!(!atlas.is_dirty(), "a clear owes nothing");
        // The next picture lands where the first was, smaller: its border
        // must be transparent, not the old picture's white.
        let r = got(atlas.insert([2; 32], 8, 8, &vec![10; 8 * 8 * 4]));
        assert_eq!(atlas.dirty_regions(), &[[0, 0, 10, 10]]);
        let border = atlas.region_bytes([r.x + r.w, r.y, 1, r.h]);
        assert!(border.iter().all(|&b| b == 0), "the right border is clear: {border:?}");
        let top = atlas.region_bytes([0, 0, 10, 1]);
        assert!(top.iter().all(|&b| b == 0));
    }
}
