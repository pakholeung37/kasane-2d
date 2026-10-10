//! Preserve difficult corners locally while simplifying smooth spans.
use super::spatial;
use crate::SdkError;
use geo::{
    Area, BooleanOps, Buffer, Coord, Intersects, LineString, MultiPolygon, Point, Polygon, Simplify,
};

fn guarded_ring(
    ring: &LineString<f64>,
    guard: &MultiPolygon<f64>,
    tolerance: f64,
) -> LineString<f64> {
    let n = ring.0.len() - 1;
    let mut anchors: Vec<_> = (0..n)
        .filter(|&i| guard.intersects(&Point::from(ring.0[i])))
        .collect();
    if anchors.is_empty() {
        return ring.simplify(tolerance);
    }
    if anchors.len() == 1 {
        anchors.push((anchors[0] + n / 2) % n);
        anchors.sort_unstable();
    }
    let mut result = Vec::<Coord<f64>>::new();
    for i in 0..anchors.len() {
        let start = anchors[i];
        let end = anchors[(i + 1) % anchors.len()];
        let length = (end + n - start) % n;
        let part = LineString::new((0..=length).map(|j| ring.0[(start + j) % n]).collect());
        let simplified = part.simplify(tolerance);
        result.extend_from_slice(&simplified.0[..simplified.0.len() - 1]);
    }
    result.push(result[0]);
    LineString::new(result)
}

pub(super) fn conservative(
    expanded: &MultiPolygon<f64>,
    required: &MultiPolygon<f64>,
    mut tolerance: f64,
) -> Result<MultiPolygon<f64>, SdkError> {
    for _ in 0..8 {
        if tolerance <= 1e-6 {
            break;
        }
        let mut guard = MultiPolygon::<f64>::new(vec![]);
        for _ in 0..4 {
            let candidate = MultiPolygon::new(
                expanded
                    .0
                    .iter()
                    .map(|p| {
                        Polygon::new(
                            guarded_ring(p.exterior(), &guard, tolerance),
                            p.interiors()
                                .iter()
                                .map(|r| guarded_ring(r, &guard, tolerance))
                                .collect(),
                        )
                    })
                    .collect(),
            );
            if !spatial::is_valid(&candidate)? {
                break;
            }
            let missing = required.difference(&candidate);
            if missing.unsigned_area() <= 1e-8 {
                return Ok(candidate);
            }
            // Only protect raster corners near lost padding. Smooth spans keep
            // the original tolerance, even if one sharp corner needs more detail.
            guard = guard.union(&missing.buffer(tolerance * 2.0));
        }
        tolerance *= 0.5;
    }
    Ok(expanded.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use geo::Validation;

    #[test]
    fn a_required_tip_does_not_densify_a_separate_smooth_boundary() {
        let circle = |radius: f64| {
            Polygon::new(
                LineString::new(
                    (0..=128)
                        .map(|i| {
                            let angle = (i % 128) as f64 * std::f64::consts::TAU / 128.0;
                            Coord {
                                x: radius * angle.cos(),
                                y: radius * angle.sin(),
                            }
                        })
                        .collect(),
                ),
                vec![],
            )
        };
        let tip = |height: f64, inset: f64| {
            Polygon::new(
                LineString::from(vec![
                    (200.0 + inset, inset),
                    (240.0 - inset, inset),
                    (240.0 - inset, 40.0 - inset),
                    (224.0, 40.0 - inset),
                    (220.0, height),
                    (216.0, 40.0 - inset),
                    (200.0 + inset, 40.0 - inset),
                    (200.0 + inset, inset),
                ]),
                vec![],
            )
        };
        let expanded = MultiPolygon::new(vec![circle(100.0), tip(41.0, 0.0)]);
        let required = MultiPolygon::new(vec![circle(97.0), tip(40.5, 1.0)]);
        let output = conservative(&expanded, &required, 2.0).unwrap();
        assert!(output.is_valid());
        assert_eq!(output.0.len(), 2);
        assert!(required.difference(&output).unsigned_area() <= 1e-8);
        assert!(
            output.0[0].exterior().0.len() <= 20,
            "a distant sharp feature forced dense sampling on the smooth circle"
        );
        assert_eq!(output, conservative(&expanded, &required, 2.0).unwrap());
    }
}
