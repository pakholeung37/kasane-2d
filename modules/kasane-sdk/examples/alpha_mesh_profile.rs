//! cargo run --release --locked -p kasane-sdk --example alpha_mesh_profile -- 9
//! Deterministic synthetic inputs; timing excludes mask construction and QA.
use kasane_sdk::{alpha_mesh_geometry, AlphaMask, AlphaMeshOptions, MeshGeometry};
use std::{hint::black_box, time::Instant};

fn quality(mesh: &MeshGeometry) -> (f64, f64) {
    let mut minimum = 180.0_f64;
    let mut skinny = 0;
    for triangle in &mesh.triangles {
        let p = triangle.map(|i| mesh.positions[i as usize]);
        let mut angle = 180.0_f64;
        for i in 0..3 {
            let a = p[i];
            let b = p[(i + 1) % 3];
            let c = p[(i + 2) % 3];
            let (ux, uy) = (b.x as f64 - a.x as f64, b.y as f64 - a.y as f64);
            let (vx, vy) = (c.x as f64 - a.x as f64, c.y as f64 - a.y as f64);
            angle = angle.min(
                (ux * vy - uy * vx)
                    .abs()
                    .atan2(ux * vx + uy * vy)
                    .to_degrees(),
            );
        }
        minimum = minimum.min(angle);
        skinny += usize::from(angle < 10.0);
    }
    (minimum, skinny as f64 * 100.0 / mesh.triangles.len() as f64)
}

fn main() {
    let runs: usize = std::env::args()
        .nth(1)
        .map(|s| s.parse().unwrap())
        .unwrap_or(9);
    assert!(runs > 0);
    println!("case,median_ms,min_ms,max_ms,vertices,triangles,min_angle,under_10_deg_pct");
    for name in [
        "rectangle",
        "circle",
        "equal_margin",
        "curved_sharp",
        "islands",
        "holes",
        "thin",
        "sparse_canvas",
    ] {
        let size = if name == "sparse_canvas" { 2048 } else { 512 };
        let alpha: Vec<_> = (0..size)
            .flat_map(|y| {
                (0..size).map(move |x| {
                    let radius = (x as f64 + 0.5 - 256.0).hypot(y as f64 + 0.5 - 256.0);
                    let foreground = match name {
                        "rectangle" => (16..496).contains(&x) && (16..496).contains(&y),
                        "circle" | "equal_margin" => radius < 220.0,
                        "curved_sharp" => {
                            radius < 170.0 || ((248..264).contains(&x) && (8..400).contains(&y))
                        }
                        "islands" => x % 48 >= 12 && x % 48 < 28 && y % 48 >= 12 && y % 48 < 28,
                        "holes" => radius < 230.0 && (radius > 130.0 || radius < 24.0),
                        "thin" => {
                            (20..492).contains(&x)
                                && (y as f64 - (256.0 + (x as f64 * 0.03).sin() * 100.0)).abs()
                                    < 4.0
                        }
                        "sparse_canvas" => (900..1100).contains(&x) && (900..1100).contains(&y),
                        _ => unreachable!(),
                    };
                    u8::from(foreground) * 255
                })
            })
            .collect();
        let mut options = AlphaMeshOptions::standard();
        if name == "equal_margin" {
            options.minimum_margin = options.outside_margin;
        }
        if name == "rectangle" {
            options.inside_spacing = 12.0;
        }
        if name == "holes" {
            options.preserve_holes = true;
        }
        let generate = || {
            alpha_mesh_geometry(
                AlphaMask {
                    width: size,
                    height: size,
                    alpha: &alpha,
                },
                &options,
            )
            .unwrap()
        };
        let mesh = generate();
        let mut times = Vec::new();
        for _ in 0..runs {
            let start = Instant::now();
            black_box(generate());
            times.push(start.elapsed().as_secs_f64() * 1000.0);
        }
        times.sort_by(f64::total_cmp);
        let (angle, skinny) = quality(&mesh);
        println!(
            "{name},{:.3},{:.3},{:.3},{},{},{angle:.4},{skinny:.2}",
            times[runs / 2],
            times[0],
            times[runs - 1],
            mesh.positions.len(),
            mesh.triangles.len(),
        );
    }
}
