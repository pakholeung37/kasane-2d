//! Redistribute smooth convex contours by arc length. Sharp and concave
//! contours keep their existing corners; callers still enforce area guards.
use geo::{Coord, LineString, MultiPolygon, Polygon};

pub(super) fn smooth(
    polygons: &MultiPolygon<f64>,
    guide: &MultiPolygon<f64>,
    spacing: f64,
    minimum: usize,
) -> MultiPolygon<f64> {
    if polygons.0.len() != guide.0.len() {
        return polygons.clone();
    }
    let mut remaining = 1_048_576;
    MultiPolygon(
        polygons
            .0
            .iter()
            .enumerate()
            .map(|(index, p)| {
                if !p.interiors().is_empty() {
                    return p.clone();
                }
                Polygon::new(
                    ring(
                        p.exterior(),
                        guide.0[index].exterior(),
                        spacing,
                        minimum,
                        &mut remaining,
                    ),
                    vec![],
                )
            })
            .collect(),
    )
}

fn ring(
    input: &LineString<f64>,
    guide: &LineString<f64>,
    spacing: f64,
    minimum: usize,
    remaining: &mut usize,
) -> LineString<f64> {
    let n = input.0.len().saturating_sub(1);
    if n < 8 {
        return input.clone();
    }
    let mut sign = 0.0_f64;
    let mut lengths = Vec::with_capacity(n);
    for i in 0..n {
        let a = input.0[(i + n - 1) % n];
        let b = input.0[i];
        let c = input.0[(i + 1) % n];
        let (ux, uy, vx, vy) = (b.x - a.x, b.y - a.y, c.x - b.x, c.y - b.y);
        let (u, v) = (ux.hypot(uy), vx.hypot(vy));
        if u <= 1e-9 || v <= 1e-9 || (ux * vx + uy * vy) / (u * v) < 30_f64.to_radians().cos() {
            return input.clone();
        }
        let cross = ux * vy - uy * vx;
        if cross.abs() > 1e-9 {
            if sign != 0.0 && cross.signum() != sign {
                return input.clone();
            }
            sign = cross.signum();
        }
        lengths.push(v);
    }
    let perimeter: f64 = lengths.iter().sum();
    let spacing = spacing.min(perimeter / minimum as f64);
    let count = (perimeter / spacing).ceil().max(minimum as f64) as usize;
    // Avoid unbounded work and leave already well-spaced rings alone. The
    // comparison uses the same bounded 10% edge slack as the CDT sampler.
    let old_count: f64 = lengths
        .iter()
        .map(|length| (length / (spacing * 1.1)).ceil().max(1.0))
        .sum();
    if count > 65_536 || count as f64 * 1.15 >= old_count {
        return input.clone();
    }
    let work = count.saturating_mul(guide.0.len());
    if work > *remaining {
        return input.clone();
    }
    *remaining -= work;
    let mut points = Vec::with_capacity(count + 1);
    let mut segment = 0;
    let mut start = 0.0;
    for i in 0..count {
        let distance = i as f64 * perimeter / count as f64;
        while segment + 1 < n && start + lengths[segment] < distance {
            start += lengths[segment];
            segment += 1;
        }
        let t = (distance - start) / lengths[segment];
        let a = input.0[segment];
        let b = input.0[segment + 1];
        let point = Coord {
            x: a.x + (b.x - a.x) * t,
            y: a.y + (b.y - a.y) * t,
        };
        // RDP chords lie inside a convex offset. Project back to the original
        // offset so a lower point count does not also shrink the requested band.
        let projected = guide
            .0
            .windows(2)
            .map(|s| {
                let (a, b) = (s[0], s[1]);
                let (dx, dy) = (b.x - a.x, b.y - a.y);
                let length2 = dx * dx + dy * dy;
                let t = if length2 > 0.0 {
                    ((point.x - a.x) * dx + (point.y - a.y) * dy) / length2
                } else {
                    0.0
                }
                .clamp(0.0, 1.0);
                let p = Coord {
                    x: a.x + t * dx,
                    y: a.y + t * dy,
                };
                ((p.x - point.x).powi(2) + (p.y - point.y).powi(2), p)
            })
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .map(|(_, p)| p)
            .unwrap_or(point);
        points.push(projected);
    }
    points.push(points[0]);
    LineString::new(points)
}
