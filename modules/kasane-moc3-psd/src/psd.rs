use crate::{Error, Layer};
use kasane_core::types::BlendMode;

fn u16be(out: &mut Vec<u8>, value: u16) {
    out.extend_from_slice(&value.to_be_bytes());
}
fn i16be(out: &mut Vec<u8>, value: i16) {
    out.extend_from_slice(&value.to_be_bytes());
}
fn u32be(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_be_bytes());
}
fn i32be(out: &mut Vec<u8>, value: i32) {
    out.extend_from_slice(&value.to_be_bytes());
}
fn checked_len(len: usize) -> Result<u32, Error> {
    u32::try_from(len).map_err(|_| Error::PsdLimit("PSD exceeds the 4 GiB section limit".into()))
}

pub fn encode(width: u32, height: u32, layers: &[Layer]) -> Result<Vec<u8>, Error> {
    let mut records = Vec::new();
    let mut pixels = Vec::new();
    i16be(&mut records, layers.len() as i16);
    // PSD layer records run from top to bottom; input layers are bottom to top.
    for layer in layers.iter().rev() {
        let bottom = layer.top + layer.height;
        let right = layer.left + layer.width;
        i32be(&mut records, layer.top as i32);
        i32be(&mut records, layer.left as i32);
        i32be(&mut records, bottom as i32);
        i32be(&mut records, right as i32);
        u16be(&mut records, 4);
        let count = layer.width as usize * layer.height as usize;
        for channel in [0i16, 1, 2, -1] {
            i16be(&mut records, channel);
            u32be(&mut records, checked_len(count + 2)?);
            u16be(&mut pixels, 0); // raw compression
            for texel in layer.rgba.as_chunks::<4>().0 {
                pixels.push(texel[if channel == -1 { 3 } else { channel as usize }]);
            }
        }
        records.extend_from_slice(b"8BIM");
        records.extend_from_slice(match layer.blend_mode {
            BlendMode::Normal => b"norm",
            BlendMode::Multiplicative => b"mul ",
            BlendMode::Additive => b"lddg",
        });
        records.extend_from_slice(&[255, 0, 0, 0]); // opacity, clipping, flags, filler
        let mut extra = Vec::new();
        u32be(&mut extra, 0); // layer mask
        u32be(&mut extra, 0); // blending ranges
                              // Pascal name, padded to a four-byte boundary.
        let ascii: Vec<u8> = layer
            .name
            .bytes()
            .filter(|c| c.is_ascii())
            .take(255)
            .collect();
        extra.push(ascii.len() as u8);
        extra.extend_from_slice(&ascii);
        while extra.len() % 4 != 0 {
            extra.push(0);
        }
        // Unicode layer name preserves the original MOC3 ArtMesh ID.
        let mut unicode = Vec::new();
        let chars: Vec<u16> = layer.name.encode_utf16().collect();
        u32be(&mut unicode, checked_len(chars.len())?);
        for unit in chars {
            u16be(&mut unicode, unit);
        }
        extra.extend_from_slice(b"8BIMluni");
        u32be(&mut extra, checked_len(unicode.len())?);
        extra.extend_from_slice(&unicode);
        while extra.len() % 4 != 0 {
            extra.push(0);
        }
        u32be(&mut records, checked_len(extra.len())?);
        records.extend_from_slice(&extra);
    }
    let mut layer_info = records;
    layer_info.extend_from_slice(&pixels);
    if layer_info.len() % 2 != 0 {
        layer_info.push(0);
    }
    let mut layer_mask = Vec::new();
    u32be(&mut layer_mask, checked_len(layer_info.len())?);
    layer_mask.extend_from_slice(&layer_info);
    u32be(&mut layer_mask, 0); // global layer mask

    // A flattened composite is required by PSD readers that do not parse layers.
    let mut composite = vec![0u8; width as usize * height as usize * 4];
    for layer in layers {
        for y in 0..layer.height {
            for x in 0..layer.width {
                let src = ((y * layer.width + x) * 4) as usize;
                let dst = (((layer.top + y) * width + layer.left + x) * 4) as usize;
                blend(
                    &mut composite[dst..dst + 4],
                    &layer.rgba[src..src + 4],
                    layer.blend_mode,
                );
            }
        }
    }

    let mut out = Vec::new();
    out.extend_from_slice(b"8BPS");
    u16be(&mut out, 1);
    out.extend_from_slice(&[0; 6]);
    u16be(&mut out, 4); // RGBA channels
    u32be(&mut out, height);
    u32be(&mut out, width);
    u16be(&mut out, 8);
    u16be(&mut out, 3); // RGB color mode
    u32be(&mut out, 0); // color mode data
    u32be(&mut out, 0); // image resources
    u32be(&mut out, checked_len(layer_mask.len())?);
    out.extend_from_slice(&layer_mask);
    u16be(&mut out, 0); // raw composite
    for channel in 0..4 {
        for texel in composite.as_chunks::<4>().0 {
            out.push(texel[channel]);
        }
    }
    Ok(out)
}

fn blend(dst: &mut [u8], src: &[u8], mode: BlendMode) {
    let sa = src[3] as f32 / 255.0;
    let da = dst[3] as f32 / 255.0;
    let out_a = sa + da * (1.0 - sa);
    if out_a == 0.0 {
        return;
    }
    for c in 0..3 {
        let s = src[c] as f32 / 255.0;
        let d = dst[c] as f32 / 255.0;
        let b = match mode {
            BlendMode::Normal => s,
            BlendMode::Multiplicative => s * d,
            BlendMode::Additive => (s + d).min(1.0),
        };
        let value = ((1.0 - sa) * da * d + (1.0 - da) * sa * s + sa * da * b) / out_a;
        dst[c] = (value * 255.0).round() as u8;
    }
    dst[3] = (out_a * 255.0).round() as u8;
}
