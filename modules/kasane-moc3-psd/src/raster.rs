use kasane_core::evaluation::Drawable;
use kasane_core::image::DecodedImage;
use kasane_core::types::{BlendMode, Canvas};

/// Cropped RGBA pixels for one ArtMesh, positioned on the full canvas.
#[derive(Debug, Clone)]
pub struct Layer {
    pub name: String,
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
                rgba[offset + 3] = (sample[3] as f32 * d.opacity.clamp(0.0, 1.0)).round() as u8;
            }
        }
    }
    Some(Layer {
        name: if d.runtime_id.is_empty() {
            d.id.clone()
        } else {
            d.runtime_id.clone()
        },
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
    let y = (v.clamp(0.0, 1.0) * (texture.height - 1) as f32).round() as usize;
    let i = (y * texture.width as usize + x) * 4;
    texture.rgba[i..i + 4].try_into().unwrap()
}
