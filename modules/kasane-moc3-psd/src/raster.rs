use kasane_core::evaluation::Drawable;
use kasane_core::image::DecodedImage;
use kasane_core::types::{BlendMode, Canvas};

/// Cropped RGBA pixels for one ArtMesh, positioned on the full canvas.
#[derive(Debug, Clone)]
pub struct Layer {
    pub name: String,
    pub hidden: bool,
    pub left: u32,
    pub top: u32,
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
    pub blend_mode: BlendMode,
}

pub fn rasterize(
    d: &Drawable,
    texture: &DecodedImage,
    canvas: Canvas,
    width: u32,
    height: u32,
) -> Option<Layer> {
    if d.positions.len() != d.uvs.len() || d.indices.len() < 3 {
        return None;
    }
    let points: Vec<(f32, f32)> = d
        .positions
        .iter()
        .map(|p| {
            (
                p.x * canvas.pixels_per_unit + canvas.origin.x,
                canvas.origin.y - p.y * canvas.pixels_per_unit,
            )
        })
        .collect();
    let (mut min_x, mut min_y, mut max_x, mut max_y) = (
        f32::INFINITY,
        f32::INFINITY,
        f32::NEG_INFINITY,
        f32::NEG_INFINITY,
    );
    for &(x, y) in &points {
        if !x.is_finite() || !y.is_finite() {
            return None;
        }
        min_x = min_x.min(x);
        min_y = min_y.min(y);
        max_x = max_x.max(x);
        max_y = max_y.max(y);
    }
    let left = min_x.floor().max(0.0).min(width as f32) as u32;
    let top = min_y.floor().max(0.0).min(height as f32) as u32;
    let right = max_x.ceil().max(0.0).min(width as f32) as u32;
    let bottom = max_y.ceil().max(0.0).min(height as f32) as u32;
    if left >= right || top >= bottom {
        return None;
    }
    let layer_width = right - left;
    let layer_height = bottom - top;
    let mut rgba = vec![0; layer_width as usize * layer_height as usize * 4];
    // An ArtMesh with zero default opacity still needs usable pixels when its
    // hidden PSD layer is toggled on. PSD visibility carries the zero state.
    let opacity = if !d.visible && d.opacity == 0.0 {
        1.0
    } else {
        d.opacity.clamp(0.0, 1.0)
    };
    for triangle in d.indices.as_chunks::<3>().0 {
        let [a, b, c] = [
            triangle[0] as usize,
            triangle[1] as usize,
            triangle[2] as usize,
        ];
        if a >= points.len() || b >= points.len() || c >= points.len() {
            continue;
        }
        let (ax, ay) = points[a];
        let (bx, by) = points[b];
        let (cx, cy) = points[c];
        let area = edge(ax, ay, bx, by, cx, cy);
        if area.abs() < 1e-8 {
            continue;
        }
        let x0 = ax.min(bx).min(cx).floor().max(left as f32) as u32;
        let y0 = ay.min(by).min(cy).floor().max(top as f32) as u32;
        let x1 = ax.max(bx).max(cx).ceil().min(right as f32) as u32;
        let y1 = ay.max(by).max(cy).ceil().min(bottom as f32) as u32;
        for y in y0..y1 {
            for x in x0..x1 {
                let px = x as f32 + 0.5;
                let py = y as f32 + 0.5;
                let wa = edge(bx, by, cx, cy, px, py) / area;
                let wb = edge(cx, cy, ax, ay, px, py) / area;
                let wc = 1.0 - wa - wb;
                if wa < -1e-5 || wb < -1e-5 || wc < -1e-5 {
                    continue;
                }
                let u = wa * d.uvs[a].x + wb * d.uvs[b].x + wc * d.uvs[c].x;
                let v = wa * d.uvs[a].y + wb * d.uvs[b].y + wc * d.uvs[c].y;
                let sample = sample(texture, u, v);
                let offset = ((y - top) as usize * layer_width as usize + (x - left) as usize) * 4;
                for channel in 0..3 {
                    let value = sample[channel] as f32 / 255.0;
                    let colored = (value * d.multiply_color[channel]
                        + d.screen_color[channel] * (1.0 - value))
                        .clamp(0.0, 1.0);
                    rgba[offset + channel] = (colored * 255.0).round() as u8;
                }
                rgba[offset + 3] = (sample[3] as f32 * opacity).round() as u8;
            }
        }
    }
    Some(Layer {
        name: if d.runtime_id.is_empty() {
            d.id.clone()
        } else {
            d.runtime_id.clone()
        },
        hidden: !d.visible,
        left,
        top,
        width: layer_width,
        height: layer_height,
        rgba,
        blend_mode: d.blend_mode,
    })
}

fn edge(ax: f32, ay: f32, bx: f32, by: f32, px: f32, py: f32) -> f32 {
    (px - ax) * (by - ay) - (py - ay) * (bx - ax)
}

fn sample(texture: &DecodedImage, u: f32, v: f32) -> [u8; 4] {
    if !u.is_finite() || !v.is_finite() || texture.width == 0 || texture.height == 0 {
        return [0; 4];
    }
    let x = (u.clamp(0.0, 1.0) * (texture.width - 1) as f32).round() as usize;
    // Kasane stores UVs with a bottom-left origin. PNG rows and the GPU
    // texture sampler use a top-left origin (see vertices_for in geometry.rs).
    let y = ((1.0 - v).clamp(0.0, 1.0) * (texture.height - 1) as f32).round() as usize;
    let i = (y * texture.width as usize + x) * 4;
    texture.rgba[i..i + 4].try_into().unwrap()
}

#[cfg(test)]
mod tests {
    use super::{rasterize, sample};
    use crate::psd;
    use ag_psd::psd::ReadOptions;
    use ag_psd::read_psd;
    use kasane_core::evaluation::Drawable;
    use kasane_core::image::DecodedImage;
    use kasane_core::types::{Canvas, Vec2};
    use std::sync::Arc;

    #[test]
    fn samples_moc3_uvs_with_the_same_vertical_origin_as_the_renderer() {
        let texture = DecodedImage {
            width: 1,
            height: 2,
            rgba: vec![255, 0, 0, 255, 0, 0, 255, 255],
        };
        assert_eq!(sample(&texture, 0.0, 1.0), [255, 0, 0, 255]);
        assert_eq!(sample(&texture, 0.0, 0.0), [0, 0, 255, 255]);
    }

    #[test]
    fn zero_opacity_artmesh_exports_as_hidden_layer_with_pixels() {
        let texture = DecodedImage {
            width: 1,
            height: 1,
            rgba: vec![255, 0, 0, 255],
        };
        let drawable = Drawable {
            id: "hidden-mesh".into(),
            positions: vec![
                Vec2::new(0.0, 0.0),
                Vec2::new(2.0, 0.0),
                Vec2::new(0.0, -2.0),
            ],
            uvs: Arc::from([Vec2::new(0.0, 0.0); 3]),
            indices: Arc::from([0, 1, 2]),
            opacity: 0.0,
            visible: false,
            ..Default::default()
        };
        let layer = rasterize(
            &drawable,
            &texture,
            Canvas::new(2.0, 2.0, Vec2::new(0.0, 0.0), 1.0),
            2,
            2,
        )
        .unwrap();
        assert!(layer.hidden);
        assert!(layer.rgba.chunks_exact(4).any(|pixel| pixel[3] > 0));

        let bytes = psd::encode(2, 2, vec![layer]).unwrap();
        let parsed = read_psd(
            &bytes,
            &ReadOptions {
                use_image_data: Some(true),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(parsed.children.as_ref().unwrap()[0].hidden, Some(true));
        assert!(parsed.children.as_ref().unwrap()[0]
            .image_data
            .as_ref()
            .unwrap()
            .data
            .chunks_exact(4)
            .any(|pixel| pixel[3] > 0));
        assert!(parsed
            .image_data
            .as_ref()
            .unwrap()
            .data
            .chunks_exact(4)
            .all(|pixel| pixel[3] == 0));
    }
}
