//! Burns the math overlay into an exported RGBA8 frame on the CPU. Uses the
//! same monospace font chain egui draws the viewport overlay with (Hack,
//! falling back to Ubuntu-Light etc. for glyphs Hack lacks), and the same
//! layout rules from [`MathOverlay`], so an export matches the preview.

use std::sync::OnceLock;

use ab_glyph::{point, Font, FontVec, PxScale, ScaleFont};
use eframe::egui;
use fractal_core::math_overlay::MathOverlay;

/// Overlay box: black at this alpha (of 255).
pub const BOX_ALPHA: u8 = 140;
/// Overlay text: this gray level, fully opaque.
pub const TEXT_GRAY: u8 = 235;

fn fonts() -> &'static [FontVec] {
    static FONTS: OnceLock<Vec<FontVec>> = OnceLock::new();
    FONTS.get_or_init(|| {
        let defs = egui::FontDefinitions::default();
        defs.families
            .get(&egui::FontFamily::Monospace)
            .into_iter()
            .flatten()
            .filter_map(|name| defs.font_data.get(name))
            .filter_map(|data| FontVec::try_from_vec_and_index(data.font.to_vec(), data.index).ok())
            .collect()
    })
}

/// The first font in the chain with a real glyph for `c`, or the primary
/// font (which then draws its `.notdef` box) if none has one.
fn font_for(fonts: &[FontVec], c: char) -> &FontVec {
    fonts.iter().find(|f| f.glyph_id(c).0 != 0).unwrap_or(&fonts[0])
}

/// Characters in `text` that no font in the chain can draw.
pub fn missing_glyphs(text: &str) -> Vec<char> {
    let fonts = fonts();
    text.chars().filter(|&c| !c.is_whitespace() && fonts.iter().all(|f| f.glyph_id(c).0 == 0)).collect()
}

/// Draws `lines` in a translucent box into `rgba` (`width × height` RGBA8),
/// positioned and sized by `overlay`.
pub fn burn(rgba: &mut [u8], width: u32, height: u32, lines: &[String], overlay: &MathOverlay) {
    let fonts = fonts();
    if fonts.is_empty() || lines.is_empty() || width == 0 || height == 0 {
        return;
    }
    let image_h = height as f32;
    let scale = PxScale::from(overlay.text_px(image_h));
    let primary = fonts[0].as_scaled(scale);
    let line_height = primary.height() + primary.line_gap();
    let padding = overlay.padding_px(image_h);

    let advance = |c: char| {
        let font = font_for(fonts, c);
        font.as_scaled(scale).h_advance(font.glyph_id(c))
    };
    let text_w = lines.iter().map(|line| line.chars().map(advance).sum::<f32>()).fold(0.0, f32::max);
    let text_h = line_height * lines.len() as f32 - primary.line_gap();
    let box_w = text_w + 2.0 * padding;
    let box_h = text_h + 2.0 * padding;
    let (box_x, box_y) = overlay.box_origin(width as f32, image_h, box_w, box_h);

    let mut canvas = Canvas { rgba, width, height };
    canvas.fill_rect(box_x, box_y, box_w, box_h, 0, f32::from(BOX_ALPHA) / 255.0);

    for (i, line) in lines.iter().enumerate() {
        let baseline = box_y + padding + primary.ascent() + i as f32 * line_height;
        let mut x = box_x + padding;
        for c in line.chars() {
            let font = font_for(fonts, c);
            let id = font.glyph_id(c);
            let glyph = id.with_scale_and_position(scale, point(x, baseline));
            x += font.as_scaled(scale).h_advance(id);
            if let Some(outlined) = font.outline_glyph(glyph) {
                let bounds = outlined.px_bounds();
                outlined.draw(|gx, gy, coverage| {
                    let px = bounds.min.x as i64 + i64::from(gx);
                    let py = bounds.min.y as i64 + i64::from(gy);
                    canvas.blend(px, py, TEXT_GRAY, coverage);
                });
            }
        }
    }
}

struct Canvas<'a> {
    rgba: &'a mut [u8],
    width: u32,
    height: u32,
}

impl Canvas<'_> {
    fn fill_rect(&mut self, x: f32, y: f32, w: f32, h: f32, gray: u8, alpha: f32) {
        let x0 = x.round() as i64;
        let y0 = y.round() as i64;
        let x1 = (x + w).round() as i64;
        let y1 = (y + h).round() as i64;
        for py in y0..y1 {
            for px in x0..x1 {
                self.blend(px, py, gray, alpha);
            }
        }
    }

    /// Source-over blend of an opaque `gray` at `alpha` coverage onto one pixel.
    fn blend(&mut self, x: i64, y: i64, gray: u8, alpha: f32) {
        if x < 0 || y < 0 || x >= i64::from(self.width) || y >= i64::from(self.height) {
            return;
        }
        let alpha = alpha.clamp(0.0, 1.0);
        let i = ((y as usize) * self.width as usize + x as usize) * 4;
        for channel in &mut self.rgba[i..i + 3] {
            *channel = (f32::from(gray) * alpha + f32::from(*channel) * (1.0 - alpha)).round() as u8;
        }
        let a = &mut self.rgba[i + 3];
        *a = (255.0 * alpha + f32::from(*a) * (1.0 - alpha)).round() as u8;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fractal_core::kinds::FractalKind;
    use fractal_core::fractal3d::{
        Formula3D, Inversion, Params3D, KIFS_DEFAULT, MANDELBOX_DEFAULT, MANDELBULB_DEFAULT, PSEUDO_KLEINIAN_DEFAULT,
    };
    use fractal_core::math_overlay::{describe, describe_3d, MathDetail, OverlayCorner};
    use fractal_core::FractalParams;

    #[test]
    fn every_overlay_character_has_a_glyph() {
        let kinds = [
            FractalKind::Mandelbrot,
            FractalKind::Julia { seed: [-0.8, 0.156] },
            FractalKind::BurningShip,
            FractalKind::Tricorn,
            FractalKind::Multibrot { power: 5.0 },
        ];
        for kind in kinds {
            for zoom in [1.0, 3.0e7, 1.0e-3, 1.0e120] {
                let mut params = FractalParams { kind, zoom, ..FractalParams::default() };
                params.palette.distance_shading = true;
                for detail in MathDetail::ALL {
                    for line in describe(&params, detail) {
                        assert!(missing_glyphs(&line).is_empty(), "no glyph for {:?} in {line:?}", missing_glyphs(&line));
                    }
                }
            }
        }
        let formulas =
            [MANDELBULB_DEFAULT, Formula3D::Mandelbulb { power: 7.5 }, MANDELBOX_DEFAULT, KIFS_DEFAULT, PSEUDO_KLEINIAN_DEFAULT];
        for formula in formulas {
            let mut params = Params3D::new(formula);
            params.camera.position = [-1.234567, 0.000001, 3.0];
            params.inversion = Some(Inversion { center: [0.0, -0.5, 0.25], radius: 1.0 });
            for detail in MathDetail::ALL {
                for line in describe_3d(&params, detail) {
                    assert!(missing_glyphs(&line).is_empty(), "no glyph for {:?} in {line:?}", missing_glyphs(&line));
                }
            }
        }
    }

    #[test]
    fn burn_draws_a_box_in_the_chosen_corner_only() {
        let (w, h) = (400u32, 300u32);
        let mut rgba = vec![200u8; (w * h * 4) as usize];
        // Large enough text that glyph stems get full pixel coverage.
        let overlay = MathOverlay { corner: OverlayCorner::TopRight, text_scale: 0.08, ..MathOverlay::default() };
        burn(&mut rgba, w, h, &["z → z² + c".to_string()], &overlay);

        let pixel = |x: u32, y: u32| rgba[((y * w + x) * 4) as usize];
        let inside = overlay.margin_px(h as f32) as u32 + 2;
        assert!(pixel(w - inside, inside) < 200, "box should darken the top-right corner");
        assert_eq!(pixel(w - 2, 2), 200, "the margin outside the box must stay untouched");
        assert_eq!(pixel(10, h - 10), 200, "bottom-left must stay untouched");
        // Some text pixels should be brighter than the darkened box.
        let brightest = (0..h / 2).flat_map(|y| (w / 2..w).map(move |x| (x, y))).map(|(x, y)| pixel(x, y)).max();
        assert!(brightest.unwrap() > 200, "text should be drawn lighter than the background");
    }
}
