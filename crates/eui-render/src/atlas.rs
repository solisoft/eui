//! A glyph atlas: one R8 texture, shelf-packed, grown once.

use std::collections::HashMap;

use eui_text::{GlyphImage, GlyphKey, TextEngine};

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
    /// Rows `y0..y1` written since the last upload, `None` when clean. A
    /// shelf packer only ever touches a band, and a band is what crosses
    /// to the GPU — or to another process.
    dirty: Option<(u32, u32)>,
}

impl Atlas {
    /// The initial edge length; the atlas grows to twice this once.
    pub const INITIAL: u32 = 1024;

    /// An empty atlas.
    pub fn new() -> Self {
        Self::with_size(Self::INITIAL)
    }

    fn with_size(size: u32) -> Self {
        Self { size, pixels: vec![0; (size * size) as usize], shelves: Vec::new(), next_y: 0, map: HashMap::new(), dirty: Some((0, size)) }
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
            self.dirty = Some((0, size));
        }
        let start = (y0 as usize).saturating_mul(size as usize);
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
        let (a, b) = ((y0 as usize).saturating_mul(self.size as usize), (y1 as usize).saturating_mul(self.size as usize));
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

    /// The R8 texel data, row-major.
    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }

    /// True when the texture must be re-uploaded; cleared by [`Self::mark_clean`].
    pub fn is_dirty(&self) -> bool {
        self.dirty.is_some()
    }

    /// Acknowledge an upload.
    pub fn mark_clean(&mut self) {
        self.dirty = None;
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
        let image = text.rasterize(key, scale).filter(|i| i.width > 0 && i.height > 0 && !i.color);
        let region = image.and_then(|img| self.pack(&img));
        if region.is_none() && image_is_some_and_large(&text.rasterize(key, scale)) && self.size < Self::INITIAL * 2 {
            self.grow();
            return self.get(text, key, scale);
        }
        self.map.insert(k, region);
        region
    }

    fn pack(&mut self, img: &GlyphImage) -> Option<Region> {
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

    /// Double the edge length, forgetting every packed glyph; they are
    /// re-packed on demand.
    fn grow(&mut self) {
        let size = self.size.saturating_mul(2);
        *self = Self::with_size(size);
    }
}

impl Default for Atlas {
    fn default() -> Self {
        Self::new()
    }
}

fn image_is_some_and_large(img: &Option<GlyphImage>) -> bool {
    img.as_ref().is_some_and(|i| i.width > 0 && i.height > 0)
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
    pub fn new() -> Self {
        Self { size: Self::SIZE, pixels: vec![0; (Self::SIZE * Self::SIZE * 4) as usize], shelves: Vec::new(), next_y: 0, map: HashMap::new(), dirty: Some((0, Self::SIZE)) }
    }

    /// Take rows `y0..y1` of RGBA texels; refused, and nothing changes,
    /// unless `rows` is exactly those rows.
    pub fn set_rows(&mut self, y0: u32, y1: u32, rows: &[u8]) -> bool {
        if y1 > self.size || y0 >= y1 || rows.len() != ((y1 - y0) as usize).saturating_mul(self.size as usize).saturating_mul(4) {
            return false;
        }
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
