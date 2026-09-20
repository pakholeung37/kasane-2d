//! Shared PNG validation and normalization for import and project resources.
use crate::types::Status;
use png::{BitDepth, ColorType};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodedImage {
    pub rgba: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

fn convert_to_rgba8(
    color_type: ColorType,
    bit_depth: BitDepth,
    data: &[u8],
    width: u32,
    height: u32,
) -> Result<Vec<u8>, Status> {
    let total_pixels = (width as usize) * (height as usize);
    let expected_rgba_len = total_pixels * 4;

    if bit_depth != BitDepth::Eight {
        return Err(Status::error(
            "INVALID_PNG",
            "Unsupported PNG bit depth (requires 8-bit)",
        ));
    }

    match color_type {
        ColorType::Rgba => {
            if data.len() < expected_rgba_len {
                return Err(Status::error("INVALID_PNG", "Incomplete RGBA PNG data"));
            }
            Ok(data[..expected_rgba_len].to_vec())
        }
        ColorType::Rgb => {
            let mut out = Vec::with_capacity(expected_rgba_len);
            for chunk in data.as_chunks::<3>().0.iter().take(total_pixels) {
                out.push(chunk[0]);
                out.push(chunk[1]);
                out.push(chunk[2]);
                out.push(255);
            }
            if out.len() != expected_rgba_len {
                return Err(Status::error("INVALID_PNG", "Incomplete RGB PNG data"));
            }
            Ok(out)
        }
        ColorType::Grayscale => {
            let mut out = Vec::with_capacity(expected_rgba_len);
            for &g in data.iter().take(total_pixels) {
                out.push(g);
                out.push(g);
                out.push(g);
                out.push(255);
            }
            if out.len() != expected_rgba_len {
                return Err(Status::error(
                    "INVALID_PNG",
                    "Incomplete Grayscale PNG data",
                ));
            }
            Ok(out)
        }
        ColorType::GrayscaleAlpha => {
            let mut out = Vec::with_capacity(expected_rgba_len);
            for chunk in data.as_chunks::<2>().0.iter().take(total_pixels) {
                out.push(chunk[0]);
                out.push(chunk[0]);
                out.push(chunk[0]);
                out.push(chunk[1]);
            }
            if out.len() != expected_rgba_len {
                return Err(Status::error(
                    "INVALID_PNG",
                    "Incomplete GrayscaleAlpha PNG data",
                ));
            }
            Ok(out)
        }
        _ => Err(Status::error(
            "INVALID_PNG",
            "Unsupported PNG color type (requires RGB, RGBA, or Grayscale)",
        )),
    }
}

pub fn decode_png(bytes: &[u8]) -> Result<DecodedImage, Status> {
    let cursor = std::io::Cursor::new(bytes);
    let mut decoder = png::Decoder::new(cursor);
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder
        .read_info()
        .map_err(|_| Status::error("INVALID_PNG", "PNG header cannot be decoded"))?;

    let info = reader.info();
    let width = info.width;
    let height = info.height;

    if (width as u64) * (height as u64) > 268435456 {
        return Err(Status::error("CAPACITY", "PNG exceeds 1 GiB decoded"));
    }

    let mut buf = vec![0; reader.output_buffer_size()];
    let output_info = reader
        .next_frame(&mut buf)
        .map_err(|_| Status::error("INVALID_PNG", "PNG data cannot be decoded"))?;
    buf.truncate(output_info.buffer_size());

    let rgba = convert_to_rgba8(
        output_info.color_type,
        output_info.bit_depth,
        &buf,
        width,
        height,
    )?;

    reader
        .finish()
        .map_err(|_| Status::error("INVALID_PNG", "PNG stream is incomplete or corrupt"))?;
    Ok(DecodedImage {
        rgba,
        width,
        height,
    })
}
