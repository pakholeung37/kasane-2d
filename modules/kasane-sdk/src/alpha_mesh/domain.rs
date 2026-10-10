use super::{error, limit, spatial, AlphaMask, AlphaMeshOptions};
use crate::SdkError;
use geo::{unary_union, Area, BooleanOps, Buffer, Coord, MultiPolygon, Polygon, Rect};
use std::collections::BTreeMap;

fn rectangle(left: u32, bottom: u32, right: u32, top: u32) -> Polygon<f64> {
    Rect::new(
        Coord {
            x: left as f64,
            y: bottom as f64,
        },
        Coord {
            x: right as f64,
            y: top as f64,
        },
    )
    .to_polygon()
}

/// Union pixel *areas*, not centers. Merge identical horizontal runs vertically
/// before the polygon union so large flat artwork needs very few rectangles.
fn foreground(mask: AlphaMask<'_>, threshold: u8) -> Result<MultiPolygon<f64>, SdkError> {
    let mut active = BTreeMap::new();
    let mut rectangles = Vec::new();
    for row in 0..=mask.height {
        let mut next = BTreeMap::new();
        if row < mask.height {
            let mut x = 0;
            let offset = row as usize * mask.width as usize;
            while x < mask.width {
                if mask.alpha[offset + x as usize] <= threshold {
                    x += 1;
                    continue;
                }
                let left = x;
                while x < mask.width && mask.alpha[offset + x as usize] > threshold {
                    x += 1;
                }
                let start = active.remove(&(left, x)).unwrap_or(row);
                next.insert((left, x), start);
            }
        }
        for ((left, right), start) in active {
            rectangles.push(rectangle(
                left,
                mask.height - row,
                right,
                mask.height - start,
            ));
        }
        if rectangles.len() + next.len() > 262_144 {
            return Err(limit());
        }
        active = next;
    }
    if rectangles.is_empty() {
        return Err(error(
            "EMPTY_ALPHA_MASK",
            "No pixels are above the alpha threshold",
        ));
    }
    Ok(unary_union(&rectangles))
}

fn offset(source: &MultiPolygon<f64>, distance: f64) -> Result<MultiPolygon<f64>, SdkError> {
    let mut result = source.buffer(distance);
    // Offsets can produce self-touching rings where pixel islands meet exactly
    // at the requested margin. Normalize those rings before simplification;
    // its conservative fallback must itself be a valid domain.
    if !spatial::is_valid(&result)? {
        result = unary_union(&result.0);
        if !spatial::is_valid(&result)? {
            return Err(error(
                "ALPHA_MESH_GEOMETRY",
                "Could not construct a valid alpha offset",
            ));
        }
    }
    Ok(result)
}

pub(super) fn build(
    mask: AlphaMask<'_>,
    options: &AlphaMeshOptions,
) -> Result<(MultiPolygon<f64>, MultiPolygon<f64>), SdkError> {
    let source = foreground(mask, options.alpha_threshold)?;
    let envelope = if options.preserve_holes {
        source.clone()
    } else {
        let filled: Vec<_> = source
            .0
            .iter()
            .map(|p| Polygon::new(p.exterior().clone(), vec![]))
            .collect();
        // A foreground island inside a filled hole must not overlap its parent.
        unary_union(&filled)
    };
    let image = MultiPolygon(vec![rectangle(0, 0, mask.width, mask.height)]);
    let expanded = if options.outside_margin == 0.0 {
        envelope.clone()
    } else {
        offset(&envelope, options.outside_margin)?
    };
    let required = if options.minimum_margin == 0.0 {
        envelope.clone()
    } else if options.minimum_margin == options.outside_margin {
        expanded.clone()
    } else {
        offset(&envelope, options.minimum_margin)?
    };
    // RDP alone can cut corners or collapse thin regions. Accept simplification
    // only if topology is valid and the entire required padded area survives.
    let tolerance =
        (options.outside_spacing * 0.25).min(options.outside_margin - options.minimum_margin);
    // Simplify the complete offset before clipping. Clipping first creates long
    // image-edge chords whose lost padding may be far from original vertices,
    // making the local guard fall back to almost every raster corner.
    let outline = super::simplify::conservative(&expanded, &required, tolerance)?;
    let redistributed = super::resample::smooth(
        &outline,
        &expanded,
        options.outside_spacing,
        options.minimum_boundary_points,
    );
    let outline = if redistributed != outline
        && spatial::is_valid(&redistributed)?
        && required.difference(&redistributed).unsigned_area() <= 1e-8
    {
        redistributed
    } else {
        outline
    };
    let outline = if options.clip_to_image {
        outline.intersection(&image)
    } else {
        outline
    };
    if outline.0.is_empty() || !spatial::is_valid(&outline)? {
        return Err(error(
            "ALPHA_MESH_GEOMETRY",
            "Could not construct a valid closed alpha boundary",
        ));
    }
    Ok((source, outline))
}
