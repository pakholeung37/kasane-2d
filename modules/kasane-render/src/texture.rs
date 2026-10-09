//! Source-image preparation. Authored pixels remain straight RGBA; GPU filtering
//! must interpolate premultiplied colors to exclude invisible texels' RGB.

/// Convert straight RGBA8 to premultiplied RGBA8 without changing the source.
pub fn premultiply_rgba8(rgba: &[u8]) -> Vec<u8> {
    assert_eq!(rgba.len() % 4, 0);
    let mut result = rgba.to_vec();
    for pixel in result.chunks_exact_mut(4) {
        let alpha = u16::from(pixel[3]);
        for color in &mut pixel[..3] {
            *color = ((u16::from(*color) * alpha + 127) / 255) as u8;
        }
    }
    result
}

/// Generate area-filtered mip levels, returned as straight RGBA for image upload.
/// RGB is weighted by alpha before averaging. Fractional footprints include the
/// last row/column of odd-sized levels and work on single-pixel axes.
pub fn straight_rgba_mipmaps(width: u32, height: u32, rgba: &[u8]) -> Vec<(u32, u32, Vec<u8>)> {
    assert!(width > 0 && height > 0);
    assert_eq!(rgba.len(), width as usize * height as usize * 4);
    let mut levels: Vec<(u32, u32, Vec<u8>)> = Vec::new();
    let (mut source_width, mut source_height) = (width, height);
    while source_width > 1 || source_height > 1 {
        let source = levels
            .last()
            .map_or(rgba, |(_, _, pixels)| pixels.as_slice());
        let next_width = (source_width / 2).max(1);
        let next_height = (source_height / 2).max(1);
        let mut next = vec![0u8; next_width as usize * next_height as usize * 4];
        for y in 0..next_height {
            for x in 0..next_width {
                let x0 = x as f64 * source_width as f64 / next_width as f64;
                let x1 = (x + 1) as f64 * source_width as f64 / next_width as f64;
                let y0 = y as f64 * source_height as f64 / next_height as f64;
                let y1 = (y + 1) as f64 * source_height as f64 / next_height as f64;
                let mut sums = [0.0; 4];
                for sy in y0.floor() as u32..y1.ceil() as u32 {
                    for sx in x0.floor() as u32..x1.ceil() as u32 {
                        let weight = (x1.min((sx + 1) as f64) - x0.max(sx as f64))
                            * (y1.min((sy + 1) as f64) - y0.max(sy as f64));
                        let offset = (sy as usize * source_width as usize + sx as usize) * 4;
                        let alpha = f64::from(source[offset + 3]);
                        for channel in 0..3 {
                            sums[channel] += f64::from(source[offset + channel]) * alpha * weight;
                        }
                        sums[3] += alpha * weight;
                    }
                }
                let area = (x1 - x0) * (y1 - y0);
                let offset = (y as usize * next_width as usize + x as usize) * 4;
                if sums[3] > 0.0 {
                    for channel in 0..3 {
                        next[offset + channel] = (sums[channel] / sums[3]).round() as u8;
                    }
                }
                next[offset + 3] = (sums[3] / area).round() as u8;
            }
        }
        levels.push((next_width, next_height, next));
        (source_width, source_height) = (next_width, next_height);
    }
    levels
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invisible_rgb_does_not_contribute_to_uploaded_color() {
        let rgba = [255, 0, 255, 0, 255, 100, 20, 128];
        assert_eq!(premultiply_rgba8(&rgba), [0, 0, 0, 0, 128, 50, 10, 128]);
        assert_eq!(rgba[0], 255);
    }

    #[test]
    fn mipmaps_exclude_invisible_color_and_weight_partial_alpha() {
        let levels = straight_rgba_mipmaps(3, 1, &[255, 0, 255, 0, 0, 255, 0, 255, 0, 0, 255, 255]);
        assert_eq!(levels[0], (1, 1, vec![0, 128, 128, 170]));
        let levels = straight_rgba_mipmaps(2, 1, &[255, 0, 0, 255, 0, 0, 255, 85]);
        assert_eq!(levels[0], (1, 1, vec![191, 0, 64, 170]));
        assert_eq!(
            straight_rgba_mipmaps(2, 1, &[255, 0, 255, 0, 0, 255, 0, 0])[0].2,
            [0; 4]
        );
    }

    #[test]
    fn mipmaps_include_odd_edges_and_single_pixel_axes() {
        for (width, height) in [(3, 1), (1, 3), (3, 3), (5, 1)] {
            let mut rgba = vec![0; width * height * 4];
            rgba[(width * height - 1) * 4..].fill(255);
            let levels = straight_rgba_mipmaps(width as u32, height as u32, &rgba);
            let last = levels.last().unwrap();
            assert_eq!((last.0, last.1), (1, 1));
            assert_eq!(
                last.2,
                [
                    255,
                    255,
                    255,
                    (255.0 / (width * height) as f64).round() as u8
                ]
            );
        }
    }
}
