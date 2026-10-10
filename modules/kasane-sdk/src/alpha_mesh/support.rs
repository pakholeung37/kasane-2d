//! Chordal-axis support for branches that disappear under the requested inset.
//! This is a polygon triangulation heuristic, not an exact medial axis.
use geo::{Contains, Coord, LineString, MultiPolygon, Point, Simplify};
use spade::{ConstrainedDelaunayTriangulation, Point2, Triangulation};
use std::collections::BTreeMap;

pub(super) fn chains(
    cdt: &ConstrainedDelaunayTriangulation<Point2<f64>>,
    outline: &MultiPolygon<f64>,
    tolerance: f64,
) -> Vec<LineString<f64>> {
    let mut nodes = Vec::<Coord<f64>>::new();
    let mut edges = BTreeMap::new();
    let mut links = Vec::new();
    for face in cdt.inner_faces() {
        let center = face.center();
        if !outline.contains(&Point::new(center.x, center.y)) {
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
            // Do not straighten a narrow bend through transparent space.
            result.push(if outline.contains(&simplified) {
                simplified
            } else {
                original
            });
        }
    }
    result
}
