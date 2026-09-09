//! The one thing layout cannot do alone: shape text.

use eui_proto::{FontFamily, FontWeight};

/// What a text run is shaped with.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FontSpec {
    /// Face role.
    pub family: FontFamily,
    /// Weight role.
    pub weight: FontWeight,
    /// Size in px, after the viewer's font scale.
    pub size: f32,
    /// Line height in px, after the viewer's font scale.
    pub line_height: f32,
}

/// The result of shaping.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct TextMetrics {
    /// Widest line.
    pub width: f32,
    /// `lines × line height`.
    pub height: f32,
    /// First line's ascent from the top.
    pub baseline: f32,
    /// Lines produced, after clamping.
    pub lines: u32,
}

/// Shapes text. `eui-text` implements this over a real font engine; tests
/// use a fixed-pitch stand-in.
pub trait TextMeasurer {
    /// Shape `text`, wrapping at `max_width` when given, keeping at most
    /// `line_clamp` lines when non-zero.
    fn measure(&mut self, text: &str, font: FontSpec, max_width: Option<f32>, line_clamp: u8) -> TextMetrics;

    /// Intrinsic size of an image or icon asset, if known; `None` sizes it to
    /// zero. Layout never blocks on an asset.
    fn asset_size(&mut self, _hash: &[u8; 32]) -> Option<(f32, f32)> {
        None
    }
}

/// A fixed-pitch measurer: every character is `0.6 × size` wide, wrapping is
/// greedy at spaces, and no font is needed. Deterministic, so golden layout
/// tests can pin exact numbers.
#[derive(Debug, Default)]
pub struct Monospace {
    /// How many times `measure` was called; tests use it to prove that a
    /// virtualised list skipped off-screen rows.
    pub calls: u32,
}

impl TextMeasurer for Monospace {
    fn measure(&mut self, text: &str, font: FontSpec, max_width: Option<f32>, line_clamp: u8) -> TextMetrics {
        self.calls = self.calls.saturating_add(1);
        let cw = font.size * 0.6;
        let mut lines: Vec<f32> = Vec::new();
        for paragraph in text.split('\n') {
            let mut line = 0usize;
            let mut started = false;
            for word in paragraph.split(' ') {
                let wl = word.chars().count();
                let candidate = if started { line.saturating_add(1).saturating_add(wl) } else { wl };
                let fits = max_width.map_or(true, |mw| candidate as f32 * cw <= mw);
                if started && !fits {
                    lines.push(line as f32 * cw);
                    line = wl;
                } else {
                    line = candidate;
                }
                started = true;
            }
            lines.push(line as f32 * cw);
        }
        if line_clamp > 0 && lines.len() > usize::from(line_clamp) {
            lines.truncate(usize::from(line_clamp));
        }
        let n = lines.len().max(1) as u32;
        TextMetrics { width: lines.iter().copied().fold(0.0, f32::max), height: n as f32 * font.line_height, baseline: font.size * 0.8, lines: n }
    }
}
