//! Shared clearance queries for optional support and fill sites.
use spade::Point2;
use std::collections::HashMap;

pub(super) struct Occupied {
    cell: f64,
    buckets: HashMap<(i64, i64), Vec<Point2<f64>>>,
}

impl Occupied {
    pub fn new(cell: f64) -> Self {
        Self {
            cell,
            buckets: HashMap::new(),
        }
    }

    fn bucket(&self, p: Point2<f64>) -> (i64, i64) {
        (
            (p.x / self.cell).floor() as i64,
            (p.y / self.cell).floor() as i64,
        )
    }

    pub fn insert(&mut self, p: Point2<f64>) {
        self.buckets.entry(self.bucket(p)).or_default().push(p);
    }

    pub fn nearby(&self, p: Point2<f64>, radius: f64) -> bool {
        debug_assert!(radius <= self.cell);
        let (x, y) = self.bucket(p);
        (-1..=1).any(|dx| {
            (-1..=1).any(|dy| {
                self.buckets.get(&(x + dx, y + dy)).is_some_and(|points| {
                    points
                        .iter()
                        .any(|q| (p.x - q.x).powi(2) + (p.y - q.y).powi(2) < radius * radius)
                })
            })
        })
    }
}
