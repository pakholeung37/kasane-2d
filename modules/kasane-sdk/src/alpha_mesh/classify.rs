//! Classify the CDT dual graph, crossing only the actual outline to change sides.
use super::error;
use crate::SdkError;
use spade::{
    handles::FixedVertexHandle, ConstrainedDelaunayTriangulation, Intersection,
    LineIntersectionIterator, Point2, Triangulation,
};

pub(super) type Cdt = ConstrainedDelaunayTriangulation<Point2<f64>>;

pub(super) fn faces(
    cdt: &Cdt,
    outline: &[(FixedVertexHandle, FixedVertexHandle)],
) -> Result<Vec<bool>, SdkError> {
    let invalid = || {
        error(
            "ALPHA_MESH_GEOMETRY",
            "Inconsistent alpha boundary topology",
        )
    };
    let mut boundary = vec![false; cdt.num_undirected_edges()];
    // Vertex handles survive insertion, whereas a boundary edge may split when
    // a later support site lands on it. Walk its current subedges rather than
    // relying on edge data (Spade does not propagate custom data across splits).
    for &(a, b) in outline {
        let mut found = false;
        for intersection in LineIntersectionIterator::new_from_handles(cdt, a, b) {
            match intersection {
                Intersection::EdgeOverlap(edge) => {
                    let flag = &mut boundary[edge.as_undirected().index()];
                    if !edge.is_constraint_edge() || *flag {
                        return Err(invalid());
                    }
                    *flag = true;
                    found = true;
                }
                Intersection::EdgeIntersection(_) => return Err(invalid()),
                Intersection::VertexIntersection(_) => {}
            }
        }
        if !found {
            return Err(invalid());
        }
    }
    let mut inside = vec![None; cdt.num_all_faces()];
    inside[cdt.outer_face().index()] = Some(false);
    let mut pending: Vec<_> = cdt.convex_hull().map(|e| e.fix()).collect();
    while let Some(edge) = pending.pop() {
        let edge = cdt.directed_edge(edge);
        let value = inside[edge.face().index()].unwrap() ^ boundary[edge.as_undirected().index()];
        let next = edge.rev().face();
        if let Some(previous) = inside[next.index()] {
            if value != previous {
                return Err(invalid());
            }
        } else {
            inside[next.index()] = Some(value);
            if let Some(face) = next.as_inner() {
                pending.extend(face.adjacent_edges().map(|e| e.fix()));
            }
        }
    }
    inside
        .into_iter()
        .map(|value| value.ok_or_else(invalid))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use geo::{Contains, MultiPolygon, Point, Polygon, Rect};

    #[test]
    fn split_boundaries_and_structural_constraints_keep_nested_domains() {
        let rectangle = |a, b| Rect::new(a, b).to_polygon();
        let outer = rectangle((0., 0.), (10., 10.));
        let hole = rectangle((3., 3.), (7., 7.));
        let island = rectangle((4., 4.), (6., 6.));
        let touching = rectangle((10., 10.), (12., 12.));
        let domain = MultiPolygon(vec![
            Polygon::new(outer.exterior().clone(), vec![hole.exterior().clone()]),
            island.clone(),
            touching.clone(),
        ]);
        let mut cdt = Cdt::new();
        let mut boundary = Vec::new();
        for polygon in [&outer, &hole, &island, &touching] {
            let vertices: Vec<_> = polygon.exterior().0[..4]
                .iter()
                .map(|p| cdt.insert(Point2::new(p.x, p.y)).unwrap())
                .collect();
            for i in 0..4 {
                let (a, b) = (vertices[i], vertices[(i + 1) % 4]);
                cdt.add_constraint(a, b);
                boundary.push((a, b));
            }
        }
        // Both an outer edge and a hole edge split after boundary construction.
        for p in [(5., 0.), (3., 5.), (5., 5.)] {
            cdt.insert(Point2::new(p.0, p.1)).unwrap();
        }
        let structural: Vec<_> = [(0., 0.), (3., 0.), (3., 2.), (0., 2.)]
            .into_iter()
            .map(|p| cdt.insert(Point2::new(p.0, p.1)).unwrap())
            .collect();
        for i in 0..4 {
            cdt.add_constraint(structural[i], structural[(i + 1) % 4]);
        }
        let labels = faces(&cdt, &boundary).unwrap();
        let mut area = 0.0;
        for face in cdt.inner_faces() {
            let p = face.center();
            assert_eq!(labels[face.index()], domain.contains(&Point::new(p.x, p.y)));
            if labels[face.index()] {
                area += face.area();
            }
        }
        assert!((area - 92.0).abs() < 1e-10);
    }
}
