use std::path::Path;

use image::{imageops::FilterType, RgbaImage};

pub use image::ImageError;

/// Saves an RGBA8 buffer as a PNG. `rgba.len()` must equal `width * height * 4`.
pub fn save_png(width: u32, height: u32, rgba: &[u8], path: &Path) -> Result<(), ImageError> {
    let image = RgbaImage::from_raw(width, height, rgba.to_vec())
        .expect("rgba buffer length must match width * height * 4");
    image.save(path)
}

/// Downsamples a supersampled RGBA8 buffer (rendered at `factor`x the target
/// resolution) to `target_width x target_height` using Lanczos3 filtering.
/// Returns `rgba` unchanged when the sizes already match.
pub fn downsample(
    rendered_width: u32,
    rendered_height: u32,
    rgba: Vec<u8>,
    target_width: u32,
    target_height: u32,
) -> Vec<u8> {
    if rendered_width == target_width && rendered_height == target_height {
        return rgba;
    }
    let image = RgbaImage::from_raw(rendered_width, rendered_height, rgba)
        .expect("rgba buffer length must match rendered_width * rendered_height * 4");
    image::imageops::resize(&image, target_width, target_height, FilterType::Lanczos3).into_raw()
}

/// [`downsample`], then saves the result as a PNG.
pub fn save_png_supersampled(
    rendered_width: u32,
    rendered_height: u32,
    rgba: &[u8],
    target_width: u32,
    target_height: u32,
    path: &Path,
) -> Result<(), ImageError> {
    let target = downsample(rendered_width, rendered_height, rgba.to_vec(), target_width, target_height);
    save_png(target_width, target_height, &target, path)
}
