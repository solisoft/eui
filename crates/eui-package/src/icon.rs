//! The application's icon, at every size a launcher asks for.
//!
//! One square PNG goes in — 1024 px covers everything, iOS's store icon
//! included — and each size comes out resampled from it: averaged over the
//! area each output pixel covers when it shrinks, interpolated when it
//! grows, in premultiplied alpha so a transparent edge does not darken.

use std::path::Path;

/// An image as 8-bit RGBA, unpremultiplied, row by row.
#[derive(Debug, Clone)]
pub struct Icon {
    /// Width and height: the icon is square.
    pub size: u32,
    rgba: Vec<u8>,
}

impl Icon {
    /// Read a square PNG.
    pub fn load(path: &Path) -> Result<Self, String> {
        let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
        Self::decode(&bytes).map_err(|e| format!("{}: {e}", path.display()))
    }

    /// Decode a square PNG.
    pub fn decode(bytes: &[u8]) -> Result<Self, String> {
        let mut decoder = png::Decoder::new(bytes);
        decoder.set_transformations(png::Transformations::normalize_to_color8());
        let mut reader = decoder.read_info().map_err(|e| format!("not a PNG ({e})"))?;
        let mut buf = vec![0u8; reader.output_buffer_size()];
        let info = reader.next_frame(&mut buf).map_err(|e| format!("not a PNG ({e})"))?;
        let (w, h) = (info.width, info.height);
        if w != h {
            return Err(format!("the icon is {w}×{h}; a launcher icon is square"));
        }
        if w < 48 {
            return Err(format!("the icon is {w} px; give at least 192, and 1024 to cover the iOS store icon"));
        }
        let px = (w as usize) * (h as usize);
        let src = buf.get(..info.buffer_size()).unwrap_or(&[]);
        let rgba: Vec<u8> = match info.color_type {
            png::ColorType::Rgba => src.to_vec(),
            png::ColorType::Rgb => src.chunks_exact(3).flat_map(|c| [c.first().copied().unwrap_or(0), c.get(1).copied().unwrap_or(0), c.get(2).copied().unwrap_or(0), 255]).collect(),
            png::ColorType::GrayscaleAlpha => src
                .chunks_exact(2)
                .flat_map(|c| {
                    let g = c.first().copied().unwrap_or(0);
                    [g, g, g, c.get(1).copied().unwrap_or(255)]
                })
                .collect(),
            png::ColorType::Grayscale => src.iter().flat_map(|&g| [g, g, g, 255]).collect(),
            png::ColorType::Indexed => return Err("an indexed PNG that did not expand".into()),
        };
        if rgba.len() != px * 4 {
            return Err("the PNG's pixels do not add up".into());
        }
        Ok(Self { size: w, rgba })
    }

    /// The icon at `size` px, as a PNG. `background`, when given, is what a
    /// transparent pixel becomes, and the PNG then has no alpha channel at
    /// all — iOS draws an icon's transparency black, and the App Store
    /// refuses a 1024 px icon that has one.
    pub fn png(&self, size: u32, background: Option<[u8; 3]>) -> Result<Vec<u8>, String> {
        let mut px = resample(&self.rgba, self.size as usize, size as usize);
        if let Some(bg) = background {
            for p in px.chunks_exact_mut(4) {
                if let [r, g, b, a] = p {
                    let alpha = u16::from(*a);
                    let over = |c: u8, under: u8| u8::try_from((u16::from(c) * alpha + u16::from(under) * (255 - alpha) + 127) / 255).unwrap_or(255);
                    *r = over(*r, bg[0]);
                    *g = over(*g, bg[1]);
                    *b = over(*b, bg[2]);
                    *a = 255;
                }
            }
        }
        let (color, px) = if background.is_some() { (png::ColorType::Rgb, px.chunks_exact(4).flat_map(|p| p.iter().take(3).copied()).collect()) } else { (png::ColorType::Rgba, px) };
        let mut out = Vec::new();
        {
            let mut enc = png::Encoder::new(&mut out, size, size);
            enc.set_color(color);
            enc.set_depth(png::BitDepth::Eight);
            enc.set_compression(png::Compression::Best);
            let mut w = enc.write_header().map_err(|e| e.to_string())?;
            w.write_image_data(&px).map_err(|e| e.to_string())?;
        }
        Ok(out)
    }
}

/// For each output pixel along one axis, the input pixels it draws from and
/// their weights, which sum to one.
fn weights(src: usize, dst: usize) -> Vec<Vec<(usize, f32)>> {
    let scale = src as f32 / dst as f32;
    (0..dst)
        .map(|i| {
            if scale >= 1.0 {
                // Shrinking: the area of each input pixel the output one covers.
                let start = i as f32 * scale;
                let end = start + scale;
                let mut w = Vec::new();
                let mut j = start.floor() as usize;
                while (j as f32) < end && j < src {
                    let cover = end.min(j as f32 + 1.0) - start.max(j as f32);
                    if cover > 0.0 {
                        w.push((j, cover / scale));
                    }
                    j += 1;
                }
                w
            } else {
                // Growing: the two nearest, linearly.
                let centre = ((i as f32 + 0.5) * scale - 0.5).max(0.0);
                let j = (centre.floor() as usize).min(src - 1);
                let t = centre - j as f32;
                let k = (j + 1).min(src - 1);
                vec![(j, 1.0 - t), (k, t)]
            }
        })
        .collect()
}

/// `src`×`src` RGBA to `dst`×`dst`, separably, in premultiplied alpha.
fn resample(rgba: &[u8], src: usize, dst: usize) -> Vec<u8> {
    let pre: Vec<f32> = rgba
        .chunks_exact(4)
        .flat_map(|p| {
            let a = f32::from(p.get(3).copied().unwrap_or(255)) / 255.0;
            [f32::from(p.first().copied().unwrap_or(0)) * a, f32::from(p.get(1).copied().unwrap_or(0)) * a, f32::from(p.get(2).copied().unwrap_or(0)) * a, a * 255.0]
        })
        .collect();
    let w = weights(src, dst);
    // Rows first: src rows of dst columns.
    let mut across = vec![0f32; src * dst * 4];
    for y in 0..src {
        for (x, taps) in w.iter().enumerate() {
            for c in 0..4 {
                let v: f32 = taps.iter().map(|&(j, k)| pre.get((y * src + j) * 4 + c).copied().unwrap_or(0.0) * k).sum();
                if let Some(o) = across.get_mut((y * dst + x) * 4 + c) {
                    *o = v;
                }
            }
        }
    }
    let mut out = vec![0u8; dst * dst * 4];
    for (y, taps) in w.iter().enumerate() {
        for x in 0..dst {
            let mut v = [0f32; 4];
            for (c, slot) in v.iter_mut().enumerate() {
                *slot = taps.iter().map(|&(j, k)| across.get((j * dst + x) * 4 + c).copied().unwrap_or(0.0) * k).sum();
            }
            let [r, g, b, a] = v;
            let un = |c: f32| if a > 0.0 { (c * 255.0 / a).round().clamp(0.0, 255.0) as u8 } else { 0 };
            let at = (y * dst + x) * 4;
            if let Some(p) = out.get_mut(at..at + 4) {
                p.copy_from_slice(&[un(r), un(g), un(b), a.round().clamp(0.0, 255.0) as u8]);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    fn square(size: u32, rgba: [u8; 4]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut enc = png::Encoder::new(&mut out, size, size);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        enc.write_header().unwrap().write_image_data(&rgba.repeat((size * size) as usize)).unwrap();
        out
    }

    #[test]
    fn a_flat_colour_stays_that_colour_at_every_size() {
        let icon = Icon::decode(&square(100, [200, 40, 10, 255])).unwrap();
        for size in [48, 72, 96, 144, 192, 1024] {
            let back = Icon::decode(&icon.png(size, None).unwrap()).unwrap();
            assert_eq!(back.size, size);
            assert!(back.rgba.chunks_exact(4).all(|p| p == [200, 40, 10, 255]), "at {size}");
        }
    }

    #[test]
    fn transparency_takes_the_background_when_one_is_given() {
        let icon = Icon::decode(&square(64, [0, 0, 0, 0])).unwrap();
        let back = Icon::decode(&icon.png(60, Some([255, 255, 255])).unwrap()).unwrap();
        assert!(back.rgba.chunks_exact(4).all(|p| p == [255, 255, 255, 255]));
    }

    #[test]
    fn not_square_is_refused() {
        let mut out = Vec::new();
        let mut enc = png::Encoder::new(&mut out, 200, 100);
        enc.set_color(png::ColorType::Rgb);
        enc.write_header().unwrap().write_image_data(&[0u8; 200 * 100 * 3]).unwrap();
        assert!(Icon::decode(&out).unwrap_err().contains("square"));
    }
}
