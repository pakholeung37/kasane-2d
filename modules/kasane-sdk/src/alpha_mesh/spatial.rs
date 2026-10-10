//! Broad-phase component queries. Exact geo predicates still decide topology;
//! disjoint island bounds never need point or polygon-pair tests.
use super::limit;
use crate::SdkError;
use geo::{
    coordinate_position::CoordPos, dimensions::Dimensions, BoundingRect, Contains, MultiPolygon,
    Point, Relate, Validation,
};
use rstar::{
    primitives::{GeomWithData, Rectangle},
    RTree, RTreeObject, AABB,
};

type Entry = GeomWithData<Rectangle<[f64; 2]>, usize>;

pub(super) struct PolygonIndex<'a> {
    polygons: &'a MultiPolygon<f64>,
    tree: RTree<Entry>,
}

impl<'a> PolygonIndex<'a> {
    pub fn new(polygons: &'a MultiPolygon<f64>) -> Self {
        let entries = polygons
            .0
            .iter()
            .enumerate()
            .filter_map(|(id, polygon)| {
                let bounds = polygon.bounding_rect()?;
                Some(GeomWithData::new(
                    Rectangle::from_corners(
                        [bounds.min().x, bounds.min().y],
                        [bounds.max().x, bounds.max().y],
                    ),
                    id,
                ))
            })
            .collect();
        Self {
            polygons,
            tree: RTree::bulk_load(entries),
        }
    }

    pub fn contains(&self, point: &Point<f64>) -> bool {
        self.tree
            .locate_in_envelope_intersecting(&AABB::from_point([point.x(), point.y()]))
            .any(|entry| self.polygons.0[entry.data].contains(point))
    }

    fn valid_pairs(&self, mut remaining: usize) -> Result<bool, SdkError> {
        for entry in &self.tree {
            for other in self.tree.locate_in_envelope_intersecting(&entry.envelope()) {
                if other.data <= entry.data {
                    continue;
                }
                remaining = remaining.checked_sub(1).ok_or_else(limit)?;
                let relation = self.polygons.0[entry.data].relate(&self.polygons.0[other.data]);
                // Match geo's MultiPolygon validity rules: point contacts are
                // allowed, overlapping interiors and shared line segments aren't.
                if relation.get(CoordPos::Inside, CoordPos::Inside) == Dimensions::TwoDimensional
                    || relation.get(CoordPos::OnBoundary, CoordPos::OnBoundary)
                        == Dimensions::OneDimensional
                {
                    return Ok(false);
                }
            }
        }
        Ok(true)
    }
}

pub(super) fn is_valid(polygons: &MultiPolygon<f64>) -> Result<bool, SdkError> {
    // Validate rings before constructing bounds or running inter-polygon relate.
    if polygons.0.iter().any(|polygon| !polygon.is_valid()) {
        return Ok(false);
    }
    PolygonIndex::new(polygons).valid_pairs(1_048_576)
}

#[cfg(test)]
mod tests {
    use super::*;
    use geo::{LineString, Polygon, Rect};

    fn square(x: f64, y: f64, size: f64) -> Polygon<f64> {
        Rect::new((x, y), (x + size, y + size)).to_polygon()
    }

    #[test]
    fn indexed_predicates_keep_holes_contacts_and_invalidity() {
        let holed = Polygon::new(
            square(0., 0., 10.).exterior().clone(),
            vec![square(2., 2., 6.).exterior().clone()],
        );
        for polygons in [
            vec![],
            vec![square(0., 0., 1.), square(1., 1., 1.)],
            vec![square(0., 0., 1.), square(1., 0., 1.)],
            vec![square(0., 0., 2.), square(1., 1., 2.)],
            vec![square(0., 0., 10.), square(3., 3., 1.)],
            vec![holed.clone(), square(3., 3., 1.)],
            vec![holed, square(1., 3., 2.)],
            vec![Polygon::new(
                LineString::from(vec![(0., 0.), (2., 2.), (0., 2.), (2., 0.), (0., 0.)]),
                vec![],
            )],
        ] {
            let polygons = MultiPolygon(polygons);
            assert_eq!(is_valid(&polygons).unwrap(), polygons.is_valid());
            if !polygons.is_valid() {
                continue;
            }
            let index = PolygonIndex::new(&polygons);
            for x in -2..=22 {
                for y in -2..=22 {
                    let point = Point::new(x as f64 * 0.5, y as f64 * 0.5);
                    assert_eq!(index.contains(&point), polygons.contains(&point));
                }
            }
        }
    }

    #[test]
    fn disjoint_islands_need_no_pair_predicates() {
        let polygons = MultiPolygon(
            (0..16_384)
                .map(|i| square((i % 128 * 4) as f64, (i / 128 * 4) as f64, 1.))
                .collect(),
        );
        assert!(PolygonIndex::new(&polygons).valid_pairs(0).unwrap());
    }

    #[test]
    fn overlapping_bounds_respect_validation_budget() {
        let polygons = MultiPolygon(vec![square(0., 0., 2.), square(1., 1., 2.)]);
        assert_eq!(
            PolygonIndex::new(&polygons)
                .valid_pairs(0)
                .unwrap_err()
                .code
                .as_ref(),
            "ALPHA_MESH_LIMIT"
        );
    }
}
