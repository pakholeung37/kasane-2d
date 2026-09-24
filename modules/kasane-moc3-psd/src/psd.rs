use crate::{Error, Layer};
use ag_psd::psd::{BlendMode as PsdBlendMode, Layer as PsdLayer, Psd, WriteOptions};
use ag_psd::{write_psd, PixelData};
use kasane_core::types::BlendMode;

pub fn encode(width: u32, height: u32, layers: Vec<Layer>) -> Result<Vec<u8>, Error> {
    // PSD readers without layer support need a merged preview. The pixels
    // remain separate in `children`, where ag-psd applies ZIP compression.
    let mut composite = vec![0u8; width as usize * height as usize * 4];
    for layer in &layers {
        if layer.hidden {
            continue;
        }
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

    // Affinity stacks the last PSD layer record on top. Keep Kasane's
    // back-to-front draw order so its layer composite matches the preview.
    // Moving the buffers also avoids a second copy before encoding.
    let children = layers
        .into_iter()
        .map(|layer| {
            let blend_mode = match layer.blend_mode {
                BlendMode::Normal => PsdBlendMode::Normal,
                BlendMode::Multiplicative => PsdBlendMode::Multiply,
                BlendMode::Additive => PsdBlendMode::LinearDodge,
            };
            PsdLayer {
                additional_info: ag_psd::psd::LayerAdditionalInfo {
                    name: Some(layer.name),
                    ..Default::default()
                },
                top: Some(f64::from(layer.top)),
                left: Some(f64::from(layer.left)),
                blend_mode: Some(blend_mode),
                hidden: Some(layer.hidden),
                image_data: Some(PixelData {
                    width: layer.width,
                    height: layer.height,
                    data: layer.rgba,
                }),
                ..Default::default()
            }
        })
        .collect();

    let psd = Psd {
        width: f64::from(width),
        height: f64::from(height),
        image_data: Some(PixelData {
            width,
            height,
            data: composite,
        }),
        children: Some(children),
        ..Default::default()
    };
    let options = WriteOptions {
        compress: Some(true),
        no_background: Some(true),
        ..Default::default()
    };
    std::panic::catch_unwind(|| write_psd(&psd, &options))
        .map_err(|_| Error::PsdLimit("ag-psd could not encode the PSD".into()))
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

#[cfg(test)]
mod tests {
    use super::{encode, Layer};
    use kasane_core::types::BlendMode;

    #[test]
    fn writes_back_layer_before_front_layer_for_affinity() {
        let layer = |name: &str, color| Layer {
            name: name.into(),
            hidden: false,
            left: 0,
            top: 0,
            width: 1,
            height: 1,
            rgba: color,
            blend_mode: BlendMode::Normal,
        };
        let bytes = encode(
            1,
            1,
            vec![
                layer("KasaneRegressionBackLayer", vec![255, 0, 0, 255]),
                layer("KasaneRegressionFrontLayer", vec![0, 0, 255, 255]),
            ],
        )
        .unwrap();
        let position = |name: &[u8]| {
            bytes
                .windows(name.len())
                .position(|part| part == name)
                .unwrap()
        };
        assert!(position(b"KasaneRegressionBackLayer") < position(b"KasaneRegressionFrontLayer"));
    }
}
