//! Chordal-axis support for branches that disappear under the requested inset.
//! This is a polygon triangulation heuristic, not an exact medial axis.
use super::classify::Cdt;
use geo::{Coord, LineString, Simplify};
use spade::{Intersection, LineIntersectionIterator, Point2, Triangulation};
use std::collections::BTreeMap;

pub(super) fn chains(cdt: &Cdt, inside: &[bool], tolerance: f64) -> Vec<LineString<f64>> {
    let mut nodes = Vec::<Coord<f64>>::new();
    let mut edges = BTreeMap::new();
    let mut links = Vec::new();
    for face in cdt.inner_faces() {
        if !inside[face.index()] {
            continue;
        }
        let mut neighbors = Vec::new();
        for edge in face.adjacent_edges() {
            if !edge.is_constraint_edge() {
                let id = *edges.entry(edge.as_undirected().fix()).or_insert_with(|| {
                    let p = edge.center();
                    nodes.push(Coord { x: p.x, y: p.y });
                    nodes.len() - 1
                });
                neighbors.push(id);
            }
        }
        if neighbors.len() == 2 {
            links.push((neighbors[0], neighbors[1]));
        } else {
            // Incenter stays inside even an obtuse terminal/junction triangle.
            let p = face.vertices().map(|v| v.position());
            let weights = [
                (p[1].x - p[2].x).hypot(p[1].y - p[2].y),
                (p[0].x - p[2].x).hypot(p[0].y - p[2].y),
                (p[0].x - p[1].x).hypot(p[0].y - p[1].y),
            ];
            let sum: f64 = weights.iter().sum();
            let id = nodes.len();
            nodes.push(Coord {
                x: (0..3).map(|i| p[i].x * weights[i]).sum::<f64>() / sum,
                y: (0..3).map(|i| p[i].y * weights[i]).sum::<f64>() / sum,
            });
            for neighbor in neighbors {
                links.push((id, neighbor));
            }
        }
    }
    let mut adjacency = vec![Vec::new(); nodes.len()];
    for (id, &(a, b)) in links.iter().enumerate() {
        adjacency[a].push((b, id));
        adjacency[b].push((a, id));
    }
    let mut visited = vec![false; links.len()];
    let mut result = Vec::new();
    // Start at junctions/leaves first, then consume closed cycles around holes.
    let starts = (0..nodes.len())
        .filter(|&i| adjacency[i].len() != 2)
        .chain((0..nodes.len()).filter(|&i| adjacency[i].len() == 2));
    for start in starts {
        for &(next, edge) in &adjacency[start] {
            if visited[edge] {
                continue;
            }
            visited[edge] = true;
            let mut path = vec![nodes[start], nodes[next]];
            let mut current = next;
            while adjacency[current].len() == 2 {
                let Some(&(next, edge)) = adjacency[current].iter().find(|&&(_, e)| !visited[e])
                else {
                    break;
                };
                visited[edge] = true;
                path.push(nodes[next]);
                current = next;
            }
            let original = LineString::new(path);
            let simplified = original.simplify(tolerance);
            // Original links lie in classified interior triangles by construction.
            // Only a changed path needs a geometric containment check.
            result.push(
                if simplified == original || path_inside(cdt, inside, &simplified) {
                    simplified
                } else {
                    original
                },
            );
        }
    }
    result
}

/// Path endpoints are original support nodes, strictly inside the domain.
/// Check the triangles traversed by each shortcut instead of relating the
/// entire MultiPolygon to every chain. Touching/following a boundary is valid.
fn path_inside(cdt: &Cdt, inside: &[bool], path: &LineString<f64>) -> bool {
    path.0.windows(2).all(|segment| {
        let [a, b] = [segment[0], segment[1]];
        LineIntersectionIterator::new(cdt, Point2::new(a.x, a.y), Point2::new(b.x, b.y)).all(
            |intersection| match intersection {
                Intersection::EdgeIntersection(edge) => {
                    inside[edge.face().index()] && inside[edge.rev().face().index()]
                }
                Intersection::EdgeOverlap(edge) => {
                    inside[edge.face().index()] || inside[edge.rev().face().index()]
                }
                Intersection::VertexIntersection(_) => true,
            },
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use geo::{Contains, Point, Polygon, Rect};

    #[test]
    fn shortcuts_respect_concavities_holes_and_vertex_contacts() {
        let notch = Polygon::new(
            LineString::from(vec![
                (0., 0.),
                (10., 0.),
                (10., 10.),
                (7., 10.),
                (7., 3.),
                (3., 3.),
                (3., 10.),
                (0., 10.),
                (0., 0.),
            ]),
            vec![],
        );
        let square = Rect::new((0., 0.), (10., 10.)).to_polygon();
        let hole = Rect::new((4., 4.), (6., 6.)).to_polygon();
        let holed = Polygon::new(square.exterior().clone(), vec![hole.exterior().clone()]);
        for domain in [notch, holed] {
            let mut cdt = Cdt::new();
            let mut boundary = Vec::new();
            for ring in std::iter::once(domain.exterior()).chain(domain.interiors()) {
                let vertices: Vec<_> = ring.0[..ring.0.len() - 1]
                    .iter()
                    .map(|p| cdt.insert(Point2::new(p.x, p.y)).unwrap())
                    .collect();
                for i in 0..vertices.len() {
                    let (a, b) = (vertices[i], vertices[(i + 1) % vertices.len()]);
                    cdt.add_constraint(a, b);
                    boundary.push((a, b));
                }
            }
            let inside = super::super::classify::faces(&cdt, &boundary).unwrap();
            // Integer endpoints exercise lines through vertices and along hole
            // or notch edges, as well as shortcuts through transparent gaps.
            let points: Vec<_> = (1..10)
                .flat_map(|x| (1..10).map(move |y| (x as f64, y as f64)))
                .filter(|&(x, y)| domain.contains(&Point::new(x, y)))
                .collect();
            for (i, &a) in points.iter().enumerate() {
                for &b in &points[i + 1..] {
                    let path = LineString::from(vec![a, b]);
                    assert_eq!(
                        path_inside(&cdt, &inside, &path),
                        domain.contains(&path),
                        "{path:?}"
                    );
                }
            }
        }
    }
}
