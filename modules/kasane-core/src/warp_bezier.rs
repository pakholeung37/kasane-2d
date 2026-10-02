//! Authoring-only bicubic controllers. Runtime evaluation still consumes the
//! baked conversion lattice; controller edits apply deltas to preserve imports.
use crate::{Status, Vec2};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WarpBezierNode {
    pub position: Vec2,
    /// Absolute positions: left, right, up, down in lattice coordinates.
    pub handles: [Vec2; 4],
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WarpBezier {
    pub rows: u32,
    pub columns: u32,
    pub nodes: Vec<WarpBezierNode>,
}
fn add(a: Vec2, b: Vec2) -> Vec2 {
    Vec2::new(a.x + b.x, a.y + b.y)
}
fn sub(a: Vec2, b: Vec2) -> Vec2 {
    Vec2::new(a.x - b.x, a.y - b.y)
}
fn mul(a: Vec2, b: f32) -> Vec2 {
    Vec2::new(a.x * b, a.y * b)
}
impl WarpBezier {
    pub fn validate(&self) -> Status {
        if self.rows == 0
            || self.columns == 0
            || self.rows > 32
            || self.columns > 32
            || self.nodes.len() != ((self.rows + 1) * (self.columns + 1)) as usize
        {
            return Status::error("INVALID_WARP_BEZIER", "Invalid controller dimensions");
        }
        if self
            .nodes
            .iter()
            .flat_map(|n| std::iter::once(&n.position).chain(n.handles.iter()))
            .any(|p| !p.x.is_finite() || !p.y.is_finite())
        {
            return Status::error("INVALID_WARP_BEZIER", "Non-finite controller position");
        }
        Status::ok()
    }
    pub fn has_handle(&self, node: usize, handle: usize) -> bool {
        let x = node % (self.columns as usize + 1);
        let y = node / (self.columns as usize + 1);
        match handle {
            0 => x > 0,
            1 => x < self.columns as usize,
            2 => y > 0,
            3 => y < self.rows as usize,
            _ => false,
        }
    }
    pub fn from_lattice(rows: u32, columns: u32, quad: bool, points: &[Vec2]) -> Self {
        let flat: Vec<_> = points.iter().flat_map(|p| [p.x, p.y]).collect();
        let sample = |u: f32, v: f32| {
            let x = u * columns as f32;
            let y = v * rows as f32;
            if (x - x.round()).abs() < 1e-6 && (y - y.round()).abs() < 1e-6 {
                return points[y.round() as usize * (columns as usize + 1) + x.round() as usize];
            }
            let mut out = [0.; 2];
            crate::deformers::warp_points(
                rows as i32,
                columns as i32,
                quad,
                &flat,
                &[u, v],
                &mut out,
                1,
            );
            Vec2::new(out[0], out[1])
        };
        let nr = if rows >= 4 { 2 } else { 1 };
        let nc = if columns >= 4 { 2 } else { 1 };
        let mut nodes = Vec::new();
        for y in 0..=nr {
            for x in 0..=nc {
                let u = x as f32 / nc as f32;
                let v = y as f32 / nr as f32;
                let position = sample(u, v);
                let du = 1. / nc as f32;
                let dv = 1. / nr as f32;
                let a = sample((u - du).max(0.), v);
                let b = sample((u + du).min(1.), v);
                let c = sample(u, (v - dv).max(0.));
                let d = sample(u, (v + dv).min(1.));
                let dx = mul(sub(b, a), 1. / if x > 0 && x < nc { 6. } else { 3. });
                let dy = mul(sub(d, c), 1. / if y > 0 && y < nr { 6. } else { 3. });
                nodes.push(WarpBezierNode {
                    position,
                    handles: [
                        sub(position, dx),
                        add(position, dx),
                        sub(position, dy),
                        add(position, dy),
                    ],
                });
            }
        }
        Self {
            rows: nr,
            columns: nc,
            nodes,
        }
    }
    pub fn sample(&self, u: f32, v: f32) -> Vec2 {
        let u = u.clamp(0., 1.) * self.columns as f32;
        let v = v.clamp(0., 1.) * self.rows as f32;
        let x = (u.floor() as u32).min(self.columns - 1);
        let y = (v.floor() as u32).min(self.rows - 1);
        let n = |x: u32, y: u32| &self.nodes[(y * (self.columns + 1) + x) as usize];
        let a = n(x, y);
        let b = n(x + 1, y);
        let c = n(x, y + 1);
        let d = n(x + 1, y + 1);
        let interior = |n: &WarpBezierNode, h: usize, k: usize| {
            sub(add(n.handles[h], n.handles[k]), n.position)
        };
        let net = [
            [a.position, a.handles[1], b.handles[0], b.position],
            [
                a.handles[3],
                interior(a, 1, 3),
                interior(b, 0, 3),
                b.handles[3],
            ],
            [
                c.handles[2],
                interior(c, 1, 2),
                interior(d, 0, 2),
                d.handles[2],
            ],
            [c.position, c.handles[1], d.handles[0], d.position],
        ];
        let basis = |t: f32| {
            [
                (1. - t).powi(3),
                3. * t * (1. - t).powi(2),
                3. * t * t * (1. - t),
                t.powi(3),
            ]
        };
        let bu = basis(u - x as f32);
        let bv = basis(v - y as f32);
        let mut p = Vec2::new(0., 0.);
        for j in 0..4 {
            for i in 0..4 {
                p = add(p, mul(net[j][i], bu[i] * bv[j]));
            }
        }
        p
    }
    pub fn apply_delta(
        &self,
        before: &Self,
        rows: u32,
        columns: u32,
        points: &[Vec2],
    ) -> Vec<Vec2> {
        points
            .iter()
            .enumerate()
            .map(|(i, p)| {
                let u = (i % (columns as usize + 1)) as f32 / columns as f32;
                let v = (i / (columns as usize + 1)) as f32 / rows as f32;
                add(*p, sub(self.sample(u, v), before.sample(u, v)))
            })
            .collect()
    }
}
