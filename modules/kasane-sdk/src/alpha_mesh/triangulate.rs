use super::{
    classify::{self, Cdt},
    error, limit,
    occupied::Occupied,
    spatial::{self, PolygonIndex},
    AlphaMask, AlphaMeshOptions,
};
use crate::{MeshGeometry, SdkError};
use geo::{
    unary_union, Area, BooleanOps, BoundingRect, Buffer, Contains, Coord, InteriorPoint,
    LineString, MultiLineString, MultiPolygon, Point, Polygon, Simplify,
};
use kasane_core::Vec2;
use spade::{handles::FixedVertexHandle, Point2, Triangulation};

struct Sampler {
    cdt: Cdt,
    occupied: Occupied,
    outline: Vec<(FixedVertexHandle, FixedVertexHandle)>,
    remaining: usize,
    max_vertices: usize,
    extent: Option<[f64; 2]>,
}

impl Sampler {
    fn point(&self, p: Coord<f64>) -> Point2<f64> {
        // Quantize before triangulation: f32 output must not collapse distinct
        // f64 vertices or turn a valid triangle inside out during conversion.
        let p = if let Some(extent) = self.extent {
            Coord {
                x: p.x.clamp(0.0, extent[0]),
                y: p.y.clamp(0.0, extent[1]),
            }
        } else {
            p
        };
        Point2::new(p.x as f32 as f64, p.y as f32 as f64)
    }

    fn insert(&mut self, p: Coord<f64>) -> Result<FixedVertexHandle, SdkError> {
        self.remaining = self.remaining.checked_sub(1).ok_or_else(limit)?;
        let p = self.point(p);
        let before = self.cdt.num_vertices();
        let handle = self
            .cdt
            .insert(p)
            .map_err(|e| error("ALPHA_MESH_GEOMETRY", &e.to_string()))?;
        if self.cdt.num_vertices() > self.max_vertices {
            return Err(limit());
        }
        if self.cdt.num_vertices() != before {
            self.occupied.insert(p);
        }
        Ok(handle)
    }

    fn insert_spaced(
        &mut self,
        p: Coord<f64>,
        clearance: f64,
    ) -> Result<Option<FixedVertexHandle>, SdkError> {
        if self.occupied.nearby(self.point(p), clearance) {
            self.remaining = self.remaining.checked_sub(1).ok_or_else(limit)?;
            return Ok(None);
        }
        self.insert(p).map(Some)
    }

    fn ring(
        &mut self,
        ring: &LineString<f64>,
        spacing: f64,
        minimum: usize,
        boundary: bool,
    ) -> Result<(), SdkError> {
        let perimeter: f64 = ring
            .0
            .windows(2)
            .map(|p| (p[1].x - p[0].x).hypot(p[1].y - p[0].y))
            .sum();
        let spacing = spacing.min(perimeter / minimum as f64);
        if spacing <= 0.0 {
            return Err(error("ALPHA_MESH_GEOMETRY", "Boundary has zero length"));
        }
        // Allow 10% slack around the target, so an 81px segment does not become
        // two 40.5px edges for an 80px target. Rounding to the nearest count can
        // stretch edges by 50% and undersample dense presets; bound that slack.
        // Every simplified corner and the explicit ring minimum remain.
        let lengths: Vec<_> = ring
            .0
            .windows(2)
            .map(|p| (p[1].x - p[0].x).hypot(p[1].y - p[0].y))
            .collect();
        let mut subdivisions: Vec<_> = lengths
            .iter()
            .map(|length| (length / (spacing * 1.1)).ceil().max(1.0))
            .collect();
        if subdivisions.iter().sum::<f64>() < minimum as f64 {
            subdivisions = lengths
                .iter()
                .map(|length| (length / spacing).ceil().max(1.0))
                .collect();
        }
        let mut handles = Vec::new();
        for (segment, steps) in ring.0.windows(2).zip(subdivisions) {
            let [a, b] = [segment[0], segment[1]];
            if steps > self.remaining as f64 {
                return Err(limit());
            }
            for i in 0..steps as usize {
                let t = i as f64 / steps;
                let handle = self.insert(Coord {
                    x: a.x + (b.x - a.x) * t,
                    y: a.y + (b.y - a.y) * t,
                })?;
                if handles.last() != Some(&handle) {
                    handles.push(handle);
                }
            }
        }
        if handles.len() < minimum {
            return Err(error(
                "ALPHA_MESH_GEOMETRY",
                "Boundary points collapse at f32 precision",
            ));
        }
        for i in 0..handles.len() {
            let (a, b) = (handles[i], handles[(i + 1) % handles.len()]);
            if a != b && self.cdt.try_add_constraint(a, b).is_empty() {
                return Err(error(
                    "ALPHA_MESH_GEOMETRY",
                    "Alpha boundary constraints intersect",
                ));
            }
        }
        if boundary {
            self.outline
                .extend((0..handles.len()).map(|i| (handles[i], handles[(i + 1) % handles.len()])));
        }
        Ok(())
    }
}

pub(super) fn generate(
    mask: AlphaMask<'_>,
    options: &AlphaMeshOptions,
    foreground: &MultiPolygon<f64>,
    outline: &MultiPolygon<f64>,
) -> Result<MeshGeometry, SdkError> {
    let clearance = (options.inside_spacing * 0.3).max(0.001);
    let mut sampler = Sampler {
        cdt: Cdt::new(),
        occupied: Occupied::new(clearance),
        outline: Vec::new(),
        remaining: 1_048_576,
        max_vertices: options.max_vertices,
        extent: options
            .clip_to_image
            .then_some([mask.width as f64, mask.height as f64]),
    };
    for polygon in &outline.0 {
        for ring in std::iter::once(polygon.exterior()).chain(polygon.interiors()) {
            sampler.ring(
                ring,
                options.outside_spacing,
                options.minimum_boundary_points,
                true,
            )?;
        }
    }
    // Build the thin-feature support graph before adding internal constraints.
    let chains = if options.inside_margin > 0.0 {
        // Classify the actual sampled f32 boundary, including any split edges.
        let inside = classify::faces(&sampler.cdt, &sampler.outline)?;
        super::support::chains(
            &sampler.cdt,
            &inside,
            (options.inside_margin + options.outside_margin).min(options.inside_spacing) * 0.15,
        )
    } else {
        Vec::new()
    };
    // When holes may be spanned, sub-spacing holes need no dedicated support
    // ring. Large holes still suppress the lattice; islands inside them retain
    // their own support. Union also absorbs islands inside newly filled holes.
    let support_foreground = if options.preserve_holes {
        foreground.clone()
    } else {
        let polygons: Vec<_> = foreground
            .0
            .iter()
            .map(|p| {
                Polygon::new(
                    p.exterior().clone(),
                    p.interiors()
                        .iter()
                        .filter(|ring| {
                            !Polygon::new((*ring).clone(), vec![])
                                .buffer(-options.inside_spacing * 0.5)
                                .0
                                .is_empty()
                        })
                        .cloned()
                        .collect(),
                )
            })
            .collect();
        unary_union(&polygons)
    };
    let inner = if options.inside_margin > 0.0 {
        let mut inset = support_foreground.buffer(-options.inside_margin);
        // A barely surviving inset makes two almost coincident support rows.
        // Use the thin-feature chain instead for these sliver components.
        let radius = (options.inside_spacing * 0.15).min(options.inside_margin * 0.5);
        inset.0.retain(|p| !p.buffer(-radius).0.is_empty());
        let mut simplified = inset.clone();
        let mut tolerance = radius;
        let mut required_inner = None;
        for _ in 0..8 {
            let candidate = inset.simplify(tolerance);
            let redistributed =
                super::resample::smooth(&candidate, &inset, options.inside_spacing, 3);
            let candidate = if redistributed != candidate
                && spatial::is_valid(&redistributed)?
                && required_inner
                    .get_or_insert_with(|| inset.buffer(-radius))
                    .difference(&redistributed)
                    .unsigned_area()
                    <= 1e-8
            {
                redistributed
            } else {
                candidate
            };
            if spatial::is_valid(&candidate)?
                && candidate.difference(outline).unsigned_area() <= 1e-8
            {
                simplified = candidate;
                break;
            }
            tolerance *= 0.5;
        }
        simplified
    } else {
        // Filled transparent holes are part of the triangulation domain, but
        // should not receive a dense lattice just because holes are allowed.
        support_foreground
    };
    if options.inside_margin > 0.0 {
        for polygon in &inner.0 {
            for ring in std::iter::once(polygon.exterior()).chain(polygon.interiors()) {
                sampler.ring(ring, options.inside_spacing, 3, false)?;
            }
        }
        // A chordal-axis chain supplies support only outside the surviving core.
        // Keep it away from the ring so a normal boundary band gains no extra row.
        let exclusion = inner.buffer(options.inside_margin * 0.5);
        let support_domain = foreground.difference(&exclusion);
        let clipped = support_domain.clip(&MultiLineString(chains), false);
        let support_spacing = options
            .inside_spacing
            .min(2.0 * (options.inside_margin + options.outside_margin));
        let support_clearance = clearance.min(support_spacing * 0.3);
        let mut supported_chains = Vec::new();
        for chain in clipped {
            let length: f64 = chain
                .0
                .windows(2)
                .map(|s| (s[1].x - s[0].x).hypot(s[1].y - s[0].y))
                .sum();
            if length < (options.inside_margin + options.outside_margin).min(options.inside_spacing)
            {
                continue;
            }
            // Branch tips and junctions are mandatory deformation sites, like
            // ring vertices. Reserve them before filtering optional samples.
            let endpoints = [
                sampler.insert(chain.0[0])?,
                sampler.insert(*chain.0.last().unwrap())?,
            ];
            supported_chains.push((chain, endpoints));
        }
        for (chain, endpoints) in supported_chains {
            let mut previous = None;
            for (segment_index, segment) in chain.0.windows(2).enumerate() {
                let [a, b] = [segment[0], segment[1]];
                let steps = ((b.x - a.x).hypot(b.y - a.y) / support_spacing)
                    .ceil()
                    .max(1.0);
                if steps + 1.0 > sampler.remaining as f64 {
                    return Err(limit());
                }
                for i in 0..=steps as usize {
                    if segment_index > 0 && i == 0 {
                        continue;
                    }
                    let t = i as f64 / steps;
                    let handle = if segment_index == 0 && i == 0 {
                        Some(endpoints[0])
                    } else if segment_index + 2 == chain.0.len() && i == steps as usize {
                        Some(endpoints[1])
                    } else {
                        sampler.insert_spaced(
                            Coord {
                                x: a.x + (b.x - a.x) * t,
                                y: a.y + (b.y - a.y) * t,
                            },
                            support_clearance,
                        )?
                    };
                    let Some(handle) = handle else {
                        // Do not connect across a skipped site near another
                        // branch or an authored ring.
                        previous = None;
                        continue;
                    };
                    if let Some(prev) = previous {
                        if prev != handle {
                            // Simplified branches may cross. Keep their support
                            // points but never force crossing structural edges.
                            sampler.cdt.try_add_constraint(prev, handle);
                        }
                    }
                    previous = Some(handle);
                }
            }
        }
    }
    // Anchor the staggered lattice in source-image coordinates. Component
    // bounds only restrict enumeration; changing a bound must not shift sites.
    let spacing = options.inside_spacing;
    let dy = spacing * (3.0_f64).sqrt() * 0.5;
    let fill = inner.buffer(-clearance);
    let fill_index = PolygonIndex::new(&fill);
    for polygon in &inner.0 {
        let bounds = polygon
            .bounding_rect()
            .ok_or_else(|| error("ALPHA_MESH_GEOMETRY", "Empty boundary"))?;
        let rows = (bounds.height() / dy).ceil();
        let columns = (bounds.width() / spacing).ceil();
        // Check floating-point bounds before converting indices: tiny spacing
        // must hit the work limit, not saturate both ends to the same integer.
        if rows * columns > sampler.remaining as f64
            || bounds.max().y / dy >= i64::MAX as f64
            || bounds.max().x / spacing >= i64::MAX as f64
        {
            return Err(limit());
        }
        let first_row = (bounds.min().y / dy - 0.5).ceil() as i64;
        let last_row = (bounds.max().y / dy - 0.5).ceil() as i64;
        for row in first_row..last_row {
            let y = (row as f64 + 0.5) * dy;
            let offset = if row.rem_euclid(2) == 0 { 0.25 } else { 0.75 };
            let first_col = (bounds.min().x / spacing - offset).ceil() as i64;
            let last_col = (bounds.max().x / spacing - offset).ceil() as i64;
            for col in first_col..last_col {
                let x = (col as f64 + offset) * spacing;
                sampler.remaining = sampler.remaining.checked_sub(1).ok_or_else(limit)?;
                if polygon.contains(&Point::new(x, y)) && fill_index.contains(&Point::new(x, y)) {
                    sampler.insert_spaced(Coord { x, y }, clearance)?;
                }
            }
        }
    }
    if !options.preserve_holes {
        // An island enclosed by a filled hole may have no inset, no lattice
        // site, and no chordal-axis branch through it. Keep at least one site
        // on every foreground component so it remains locally deformable.
        let mut sites: Vec<_> = sampler.cdt.vertices().map(|v| v.position()).collect();
        sites.sort_by(|a, b| a.x.total_cmp(&b.x).then(a.y.total_cmp(&b.y)));
        for polygon in &foreground.0 {
            let bounds = polygon
                .bounding_rect()
                .ok_or_else(|| error("ALPHA_MESH_GEOMETRY", "Empty foreground"))?;
            let start = sites.partition_point(|p| p.x < bounds.min().x);
            let end = sites.partition_point(|p| p.x <= bounds.max().x);
            let mut supported = false;
            for p in &sites[start..end] {
                sampler.remaining = sampler.remaining.checked_sub(1).ok_or_else(limit)?;
                if p.y >= bounds.min().y
                    && p.y <= bounds.max().y
                    && polygon.contains(&Point::new(p.x, p.y))
                {
                    supported = true;
                    break;
                }
            }
            if !supported {
                let p = polygon
                    .interior_point()
                    .ok_or_else(|| error("ALPHA_MESH_GEOMETRY", "No foreground interior site"))?;
                sampler.insert(p.0)?;
            }
        }
    }
    // Only outline constraints toggle inside/outside; support rings and chains
    // remain traversable. Rebuild boundary flags after any edge splits.
    // No global angle refinement: it can destroy the regular interior lattice
    // and add cascades of tiny triangles around intentional narrow features.
    let cdt = &sampler.cdt;
    let inside = classify::faces(cdt, &sampler.outline)?;
    let mut triangles = Vec::new();
    let mut used = vec![false; cdt.num_vertices()];
    for face in cdt.inner_faces() {
        if !inside[face.index()] {
            continue;
        }
        let ids = face.vertices().map(|v| v.index());
        for id in ids {
            used[id] = true;
        }
        triangles.push(ids);
    }
    // Stable coordinate order and canonical triangle order make regeneration
    // independent of internal triangulation handle numbering.
    let mut vertices: Vec<_> = cdt
        .vertices()
        .filter(|v| used[v.index()])
        .map(|v| (v.index(), v.position()))
        .collect();
    vertices.sort_by(|a, b| a.1.x.total_cmp(&b.1.x).then(a.1.y.total_cmp(&b.1.y)));
    let mut remap = vec![0; cdt.num_vertices()];
    let mut positions = Vec::new();
    let mut uvs = Vec::new();
    for (id, (old, p)) in vertices.into_iter().enumerate() {
        remap[old] = id as u32;
        let position = Vec2::new(p.x as f32, p.y as f32);
        positions.push(position);
        uvs.push(Vec2::new(
            position.x / mask.width as f32,
            position.y / mask.height as f32,
        ));
    }
    let mut output = Vec::new();
    for triangle in triangles {
        let mut ids = triangle.map(|i| remap[i]);
        let [a, b, c] = ids.map(|id| positions[id as usize]);
        let cross = (b.x as f64 - a.x as f64) * (c.y as f64 - a.y as f64)
            - (b.y as f64 - a.y as f64) * (c.x as f64 - a.x as f64);
        if cross <= 0.0 {
            return Err(error(
                "ALPHA_MESH_GEOMETRY",
                "Triangle collapses at f32 precision",
            ));
        }
        let start = (0..3).min_by_key(|&i| ids[i]).unwrap();
        ids.rotate_left(start);
        output.push(ids);
    }
    output.sort_unstable();
    if output.is_empty() {
        return Err(error(
            "ALPHA_MESH_GEOMETRY",
            "No interior triangles were generated",
        ));
    }
    Ok(MeshGeometry {
        vertex_ids: (0..positions.len() as u32).collect(),
        positions,
        uvs,
        triangles: output,
    })
}
