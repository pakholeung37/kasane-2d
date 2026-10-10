use std::collections::{BTreeMap, BTreeSet};

use kasane_core::Vec2;
use kasane_sdk::{alpha_mesh_geometry, AlphaMask, AlphaMeshOptions, MeshGeometry};

fn pixels(width: u32, height: u32, f: impl Fn(u32, u32) -> u8) -> Vec<u8> {
    (0..height)
        .flat_map(|y| (0..width).map(move |x| (x, y)))
        .map(|(x, y)| f(x, y))
        .collect()
}

fn options() -> AlphaMeshOptions {
    AlphaMeshOptions {
        outside_spacing: 5.0,
        inside_spacing: 6.0,
        outside_margin: 0.0,
        inside_margin: 0.0,
        minimum_margin: 0.0,
        minimum_boundary_points: 4,
        preserve_holes: true,
        ..AlphaMeshOptions::default()
    }
}

fn generate(width: u32, height: u32, alpha: &[u8], options: &AlphaMeshOptions) -> MeshGeometry {
    alpha_mesh_geometry(
        AlphaMask {
            width,
            height,
            alpha,
        },
        options,
    )
    .unwrap()
}

fn cross(a: Vec2, b: Vec2, c: Vec2) -> f64 {
    (b.x as f64 - a.x as f64) * (c.y as f64 - a.y as f64)
        - (b.y as f64 - a.y as f64) * (c.x as f64 - a.x as f64)
}

fn covers(mesh: &MeshGeometry, x: f32, y: f32) -> bool {
    let p = Vec2::new(x, y);
    mesh.triangles.iter().any(|t| {
        let [a, b, c] = t.map(|i| mesh.positions[i as usize]);
        cross(a, b, p) >= -1e-5 && cross(b, c, p) >= -1e-5 && cross(c, a, p) >= -1e-5
    })
}

fn area(mesh: &MeshGeometry) -> f64 {
    mesh.triangles
        .iter()
        .map(|t| {
            let [a, b, c] = t.map(|i| mesh.positions[i as usize]);
            cross(a, b, c) * 0.5
        })
        .sum()
}

/// Independent checks on exported geometry, including manifold boundary cycles.
fn valid(mesh: &MeshGeometry, width: u32, height: u32) {
    assert!(!mesh.triangles.is_empty());
    assert_eq!(mesh.positions.len(), mesh.uvs.len());
    assert_eq!(
        mesh.vertex_ids,
        (0..mesh.positions.len() as u32).collect::<Vec<_>>()
    );
    let mut edges = BTreeMap::<_, Vec<_>>::new();
    let mut used = BTreeSet::new();
    for t in &mesh.triangles {
        assert!(t.iter().all(|&i| (i as usize) < mesh.positions.len()));
        let [a, b, c] = t.map(|i| mesh.positions[i as usize]);
        assert!(cross(a, b, c) > 0.0, "degenerate or reversed triangle");
        for i in 0..3 {
            let (a, b) = (t[i], t[(i + 1) % 3]);
            edges.entry((a.min(b), a.max(b))).or_default().push((a, b));
            used.insert(a);
        }
    }
    assert_eq!(used.len(), mesh.positions.len(), "unused vertices");
    let mut degree = BTreeMap::<_, (usize, usize)>::new();
    for directed in edges.values() {
        assert!(directed.len() <= 2, "non-manifold edge");
        if directed.len() == 2 {
            assert_eq!(directed[0], (directed[1].1, directed[1].0));
        } else {
            let (a, b) = directed[0];
            degree.entry(a).or_default().0 += 1;
            degree.entry(b).or_default().1 += 1;
        }
    }
    // Point-touching components can share a vertex, but every incoming boundary
    // edge must still have an outgoing edge (no open boundary).
    assert!(degree.values().all(|&(a, b)| a > 0 && a == b));
    for (p, uv) in mesh.positions.iter().zip(&mesh.uvs) {
        assert!(p.x.is_finite() && p.y.is_finite());
        assert!((0.0..=width as f32).contains(&p.x));
        assert!((0.0..=height as f32).contains(&p.y));
        assert!((0.0..=1.0).contains(&uv.x) && (0.0..=1.0).contains(&uv.y));
        assert_eq!(uv.x, p.x / width as f32);
        assert_eq!(uv.y, p.y / height as f32);
    }
}

fn coverage(
    mesh: &MeshGeometry,
    width: u32,
    height: u32,
    alpha: &[u8],
    threshold: u8,
    exact: bool,
) {
    for row in 0..height {
        for x in 0..width {
            let opaque = alpha[(row * width + x) as usize] > threshold;
            if opaque || exact {
                // Probe multiple positions within pixel squares, not just centers.
                for (dx, dy) in [(0.05, 0.05), (0.5, 0.5), (0.95, 0.95)] {
                    assert_eq!(
                        covers(mesh, x as f32 + dx, height as f32 - row as f32 - dy),
                        opaque,
                        "pixel ({x},{row}) probe ({dx},{dy})"
                    );
                }
            }
        }
    }
}

#[test]
fn opaque_rectangle_has_correct_area_uvs_and_closed_topology() {
    let alpha = vec![255; 32 * 20];
    let mesh = generate(32, 20, &alpha, &options());
    valid(&mesh, 32, 20);
    assert!((area(&mesh) - 640.0).abs() < 1e-4);
    coverage(&mesh, 32, 20, &alpha, 0, true);
}

#[test]
fn half_pixel_padding_repairs_self_touching_offset_contours() {
    let mut alpha = vec![0; 32 * 32];
    for (x, y) in [
        (9, 20),
        (11, 20),
        (14, 21),
        (11, 22),
        (12, 22),
        (30, 22),
        (14, 23),
        (16, 23),
        (28, 23),
        (11, 24),
        (29, 24),
        (11, 25),
        (13, 25),
        (14, 25),
        (8, 27),
        (10, 27),
        (11, 27),
    ] {
        alpha[y * 32 + x] = 255;
    }
    for preserve_holes in [false, true] {
        for minimum_margin in [0.0, 0.5] {
            let opts = AlphaMeshOptions {
                outside_spacing: 8.0,
                inside_spacing: 8.0,
                outside_margin: 0.5,
                minimum_margin,
                inside_margin: 0.0,
                clip_to_image: false,
                preserve_holes,
                ..AlphaMeshOptions::standard()
            };
            let mesh = generate(32, 32, &alpha, &opts);
            valid(&mesh, 32, 32);
            coverage(&mesh, 32, 32, &alpha, 0, false);
            assert_eq!(mesh, generate(32, 32, &alpha, &opts));
        }
    }
}

#[test]
fn separated_islands_at_vertex_budget_keep_exact_pixel_coverage() {
    let alpha = pixels(
        512,
        512,
        |x, y| if x % 4 == 0 && y % 4 == 0 { 255 } else { 0 },
    );
    let opts = AlphaMeshOptions {
        outside_spacing: 85.0,
        inside_spacing: 85.0,
        ..options()
    };
    let start = std::time::Instant::now();
    let mesh = generate(512, 512, &alpha, &opts);
    eprintln!("16384 islands: {:?}", start.elapsed());
    assert_eq!(mesh.positions.len(), 65_536);
    assert_eq!(mesh.triangles.len(), 32_768);
    valid(&mesh, 512, 512);
    assert_eq!(area(&mesh), 16_384.0);
    // Each unit square gets exactly two triangles, with no bridges across gaps.
    let mut faces_per_pixel = BTreeMap::new();
    for t in &mesh.triangles {
        let p = t.map(|i| mesh.positions[i as usize]);
        let x = ((p[0].x + p[1].x + p[2].x) / 3.0).floor() as u32;
        let y = ((p[0].y + p[1].y + p[2].y) / 3.0).floor() as u32;
        assert_eq!(alpha[((511 - y) * 512 + x) as usize], 255);
        assert!(p.iter().all(|p| p.x >= x as f32
            && p.x <= (x + 1) as f32
            && p.y >= y as f32
            && p.y <= (y + 1) as f32));
        *faces_per_pixel.entry((x, y)).or_insert(0) += 1;
    }
    assert_eq!(faces_per_pixel.len(), 16_384);
    assert!(faces_per_pixel.values().all(|&count| count == 2));
}

#[test]
fn single_pixel_thin_lines_concavity_and_separate_islands_survive() {
    let alpha = pixels(24, 24, |x, y| {
        u8::from(
            (x == 2 && y == 3)
                || (x == 5 && (2..21).contains(&y))
                || ((10..20).contains(&x)
                    && ((3..6).contains(&y) || (x < 13 && (3..15).contains(&y))))
                || (x == 21 && y == 20),
        ) * 255
    });
    let mesh = generate(24, 24, &alpha, &options());
    valid(&mesh, 24, 24);
    coverage(&mesh, 24, 24, &alpha, 0, true);
    assert!((area(&mesh) - alpha.iter().filter(|&&v| v > 0).count() as f64).abs() < 1e-4);
    for t in &mesh.triangles {
        let p = t.map(|i| mesh.positions[i as usize]);
        assert!((p[0].x - p[1].x).abs() <= 10.0, "bridged separate islands");
    }
}

#[test]
fn holes_and_nested_islands_are_preserved_or_explicitly_filled() {
    let alpha = pixels(40, 40, |x, y| {
        u8::from(
            (3..37).contains(&x)
                && (3..37).contains(&y)
                && (!((10..30).contains(&x) && (10..30).contains(&y))
                    || ((17..23).contains(&x) && (17..23).contains(&y))),
        ) * 255
    });
    let preserved = generate(40, 40, &alpha, &options());
    valid(&preserved, 40, 40);
    coverage(&preserved, 40, 40, &alpha, 0, true);
    let filled = generate(
        40,
        40,
        &alpha,
        &AlphaMeshOptions {
            preserve_holes: false,
            ..options()
        },
    );
    assert!((area(&filled) - 34.0 * 34.0).abs() < 1e-3);
    assert!(covers(&filled, 12.0, 12.0));
}

#[test]
fn threshold_is_inclusive_and_filters_dust_without_removing_opaque_specks() {
    let alpha = pixels(16, 16, |x, y| match (x, y) {
        (1, 1) => 10,
        (4, 4) => 11,
        (8, 8) => 255,
        _ => 0,
    });
    let mesh = generate(
        16,
        16,
        &alpha,
        &AlphaMeshOptions {
            alpha_threshold: 10,
            ..options()
        },
    );
    valid(&mesh, 16, 16);
    coverage(&mesh, 16, 16, &alpha, 10, true);
    assert!((area(&mesh) - 2.0).abs() < 1e-5);
}

#[test]
fn pixel_rows_are_flipped_once_and_rectangular_images_keep_pixel_aspect() {
    let alpha = pixels(40, 12, |x, y| u8::from(x < 8 && y < 3) * 255);
    let mesh = generate(40, 12, &alpha, &options());
    assert!(mesh.positions.iter().all(|p| p.x <= 8.0 && p.y >= 9.0));
    valid(&mesh, 40, 12);
    coverage(&mesh, 40, 12, &alpha, 0, true);
}

#[test]
fn margin_expands_coverage_and_clips_at_image_edges() {
    let alpha = pixels(40, 40, |x, y| {
        u8::from((10..30).contains(&x) && (10..30).contains(&y)) * 255
    });
    let opts = AlphaMeshOptions {
        outside_margin: 4.0,
        minimum_margin: 2.0,
        inside_margin: 3.0,
        ..options()
    };
    let mesh = generate(40, 40, &alpha, &opts);
    valid(&mesh, 40, 40);
    coverage(&mesh, 40, 40, &alpha, 0, false);
    for x in 8..32 {
        for y in 10..30 {
            assert!(covers(&mesh, x as f32 + 0.01, y as f32 + 0.01));
        }
    }
    assert!(area(&mesh) > 600.0);
    let full = vec![255; 40 * 40];
    let clipped = generate(40, 40, &full, &opts);
    valid(&clipped, 40, 40);
    assert!((area(&clipped) - 1600.0).abs() < 1e-3);
}

#[test]
fn inner_support_rings_do_not_cut_out_the_middle() {
    let alpha = pixels(64, 64, |x, y| {
        u8::from((8..56).contains(&x) && (8..56).contains(&y)) * 255
    });
    let mesh = generate(
        64,
        64,
        &alpha,
        &AlphaMeshOptions {
            inside_margin: 8.0,
            inside_spacing: 12.0,
            ..options()
        },
    );
    valid(&mesh, 64, 64);
    coverage(&mesh, 64, 64, &alpha, 0, true);
    assert!(mesh
        .positions
        .iter()
        .any(|p| p.x == 16.0 && p.y > 16.0 && p.y < 48.0));
    assert!((area(&mesh) - 48.0 * 48.0).abs() < 1e-3);
}

#[test]
fn collapsed_inner_offset_does_not_delete_thin_artwork() {
    let alpha = pixels(32, 32, |x, y| {
        u8::from(x == 15 && (2..30).contains(&y)) * 255
    });
    let mesh = generate(
        32,
        32,
        &alpha,
        &AlphaMeshOptions {
            inside_margin: 14.0,
            ..options()
        },
    );
    coverage(&mesh, 32, 32, &alpha, 0, true);
}

#[test]
fn denser_settings_add_vertices_and_generation_is_repeatable() {
    let alpha = pixels(96, 64, |x, y| {
        u8::from((5..91).contains(&x) && (5..59).contains(&y)) * 255
    });
    let sparse = AlphaMeshOptions {
        outside_spacing: 24.0,
        inside_spacing: 24.0,
        ..options()
    };
    let dense = AlphaMeshOptions {
        outside_spacing: 8.0,
        inside_spacing: 8.0,
        ..options()
    };
    let a = generate(96, 64, &alpha, &sparse);
    let b = generate(96, 64, &alpha, &dense);
    assert!(b.positions.len() > a.positions.len() * 2);
    assert_eq!(b, generate(96, 64, &alpha, &dense));
    assert_eq!(a, generate(96, 64, &alpha, &sparse));
}

#[test]
fn minimum_points_applies_even_to_tiny_boundaries() {
    let mesh = generate(
        1,
        1,
        &[255],
        &AlphaMeshOptions {
            minimum_boundary_points: 10,
            ..options()
        },
    );
    let boundary_points = mesh
        .positions
        .iter()
        .filter(|p| p.x == 0.0 || p.y == 0.0 || p.x == 1.0 || p.y == 1.0)
        .count();
    assert!(boundary_points >= 10);
    valid(&mesh, 1, 1);
}

#[test]
fn diagonal_contacts_and_checkerboards_keep_their_pixel_area() {
    for size in [2, 3, 8] {
        let alpha = pixels(size, size, |x, y| u8::from((x + y) % 2 == 0) * 255);
        let mesh = generate(size, size, &alpha, &options());
        valid(&mesh, size, size);
        coverage(&mesh, size, size, &alpha, 0, true);
        assert!((area(&mesh) - alpha.iter().filter(|&&v| v > 0).count() as f64).abs() < 1e-4);
    }
}

#[test]
fn curved_and_sharp_shapes_keep_coverage_with_all_presets() {
    let alpha = pixels(128, 128, |x, y| {
        let dx = x as f64 - 64.0;
        let dy = y as f64 - 64.0;
        u8::from(dx * dx + dy * dy < 44.0 * 44.0 || (x > 63 && x < 110 && y > 63 && y < x)) * 255
    });
    for opts in [
        AlphaMeshOptions::standard(),
        AlphaMeshOptions::deformation_small(),
        AlphaMeshOptions::deformation_large(),
    ] {
        let mesh = generate(128, 128, &alpha, &opts);
        valid(&mesh, 128, 128);
        coverage(&mesh, 128, 128, &alpha, 0, false);
    }
}

#[test]
fn malformed_empty_and_excessive_inputs_return_errors() {
    let call = |w, h, a: &[u8], o: &AlphaMeshOptions| {
        alpha_mesh_geometry(
            AlphaMask {
                width: w,
                height: h,
                alpha: a,
            },
            o,
        )
        .unwrap_err()
        .code
        .to_string()
    };
    assert_eq!(call(0, 0, &[], &options()), "INVALID_ALPHA_MASK");
    assert_eq!(call(2, 2, &[255], &options()), "INVALID_ALPHA_MASK");
    assert_eq!(
        call(u32::MAX, u32::MAX, &[], &options()),
        "INVALID_ALPHA_MASK"
    );
    assert_eq!(call(1, 1, &[0], &options()), "EMPTY_ALPHA_MASK");
    assert_eq!(
        call(
            1,
            1,
            &[255],
            &AlphaMeshOptions {
                alpha_threshold: 255,
                ..options()
            }
        ),
        "EMPTY_ALPHA_MASK"
    );
    for value in [0.0, -1.0, f64::NAN, f64::INFINITY, 1e100] {
        assert_eq!(
            call(
                1,
                1,
                &[255],
                &AlphaMeshOptions {
                    inside_spacing: value,
                    ..options()
                }
            ),
            "INVALID_ALPHA_MESH_OPTIONS"
        );
    }
    assert_eq!(
        call(
            1,
            1,
            &[255],
            &AlphaMeshOptions {
                minimum_margin: 1.0,
                ..options()
            }
        ),
        "INVALID_ALPHA_MESH_OPTIONS"
    );
    assert_eq!(
        call(
            1,
            1,
            &[255],
            &AlphaMeshOptions {
                max_vertices: 3,
                minimum_boundary_points: 3,
                ..options()
            }
        ),
        "ALPHA_MESH_LIMIT"
    );
    assert_eq!(
        call(
            1,
            1,
            &[255],
            &AlphaMeshOptions {
                inside_spacing: 1e-10,
                ..options()
            }
        ),
        "ALPHA_MESH_LIMIT"
    );
}

#[test]
fn many_small_masks_preserve_area_and_foreground_without_panics() {
    // Fixed PRNG seed; exercise arbitrary holes, islands and point contacts.
    let mut state = 0x12ab34cdu32;
    for _ in 0..64 {
        let alpha: Vec<_> = (0..64)
            .map(|_| {
                state ^= state << 13;
                state ^= state >> 17;
                state ^= state << 5;
                if state & 1 == 0 {
                    0
                } else {
                    255
                }
            })
            .collect();
        let mesh = generate(8, 8, &alpha, &options());
        valid(&mesh, 8, 8);
        coverage(&mesh, 8, 8, &alpha, 0, true);
        assert!((area(&mesh) - alpha.iter().filter(|&&v| v > 0).count() as f64).abs() < 1e-4);
    }
}

#[test]
fn padding_can_merge_islands_and_close_holes_without_losing_foreground() {
    let alpha = pixels(32, 32, |x, y| {
        u8::from(
            (3..29).contains(&y)
                && ((3..14).contains(&x) || (17..29).contains(&x))
                && !(x == 8 && y == 8),
        ) * 255
    });
    let mesh = generate(
        32,
        32,
        &alpha,
        &AlphaMeshOptions {
            outside_margin: 3.0,
            minimum_margin: 1.0,
            inside_margin: 2.0,
            ..options()
        },
    );
    valid(&mesh, 32, 32);
    coverage(&mesh, 32, 32, &alpha, 0, false);
    assert!(covers(&mesh, 15.5, 16.5));
    assert!(covers(&mesh, 8.5, 23.5));
}

#[test]
fn minimum_margin_survives_simplification_at_sharp_corners() {
    let alpha = pixels(48, 48, |x, y| {
        u8::from((10..38).contains(&x) && (10..38).contains(&y) && x + y < 51) * 255
    });
    let opts = AlphaMeshOptions {
        outside_spacing: 40.0,
        outside_margin: 5.0,
        minimum_margin: 3.0,
        ..options()
    };
    let mesh = generate(48, 48, &alpha, &opts);
    valid(&mesh, 48, 48);
    for row in 0..48 {
        for x in 0..48 {
            if alpha[(row * 48 + x) as usize] == 0 {
                continue;
            }
            for (dx, dy) in [(2.8, 0.0), (-2.8, 0.0), (0.0, 2.8), (0.0, -2.8)] {
                assert!(covers(
                    &mesh,
                    x as f32 + 0.5 + dx,
                    48.0 - row as f32 - 0.5 + dy
                ));
            }
        }
    }
}

#[test]
fn padded_fragmented_masks_do_not_create_degenerate_meshes() {
    let mut state = 0x532ea90du32;
    for _ in 0..24 {
        let alpha: Vec<_> = (0..24 * 24)
            .map(|_| {
                state ^= state << 13;
                state ^= state >> 17;
                state ^= state << 5;
                if state.is_multiple_of(5) {
                    255
                } else {
                    0
                }
            })
            .collect();
        let opts = AlphaMeshOptions {
            outside_margin: 1.5,
            minimum_margin: 0.5,
            inside_margin: 0.5,
            ..options()
        };
        let mesh = generate(24, 24, &alpha, &opts);
        valid(&mesh, 24, 24);
        coverage(&mesh, 24, 24, &alpha, 0, false);
    }
}

#[test]
fn generated_geometry_is_accepted_by_sdk_and_undo_redo_restores_it() {
    use kasane_core::Canvas;
    use kasane_sdk::{prepare_png_asset, rectangle_mesh, AuthoringSession, TopologyReplacement};
    const DOCUMENT: &str = "00000000-0000-4000-8000-000000000701";
    const ASSET: &str = "00000000-0000-4000-8000-000000000702";
    const MESH: &str = "00000000-0000-4000-8000-000000000703";
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/asymmetric-2x2.png");
    let asset = prepare_png_asset(ASSET, "source", &path).unwrap();
    let original = rectangle_mesh(
        MESH,
        "source",
        ASSET,
        Vec2::new(0.0, 0.0),
        Vec2::new(2.0, 2.0),
    )
    .unwrap();
    let mut session =
        AuthoringSession::new(DOCUMENT, Canvas::new(2.0, 2.0, Vec2::new(1.0, 1.0), 1.0)).unwrap();
    session
        .edit("source", None, |edit| {
            edit.create_asset(asset)?;
            edit.create_mesh(original.clone())
        })
        .unwrap();
    let original = session.mesh(MESH).unwrap();
    let snapshot = session.geometry(MESH).unwrap();
    let geometry = generate(2, 2, &[255, 0, 255, 255], &options());
    let mut replacement = original.clone();
    replacement.vertex_ids = geometry.vertex_ids;
    replacement.base_positions = geometry.positions;
    replacement.uvs = geometry.uvs;
    replacement.triangles = geometry.triangles;
    session
        .edit("alpha topology", Some(snapshot.version), |edit| {
            edit.replace_topology(
                &snapshot,
                TopologyReplacement {
                    mesh: replacement.clone(),
                    binding: None,
                    blend_bindings: vec![],
                    glues: vec![],
                    vertex_mapping: snapshot.vertex_ids.iter().map(|&id| (id, None)).collect(),
                },
            )
        })
        .unwrap();
    assert!(session.validate_structure().is_empty());
    assert_eq!(session.mesh(MESH).unwrap(), replacement);
    session.undo().unwrap();
    assert_eq!(session.mesh(MESH).unwrap(), original);
    session.redo().unwrap();
    assert_eq!(session.mesh(MESH).unwrap(), replacement);
}

#[test]
fn large_asymmetric_image_uses_local_pixel_geometry_without_atlas_scaling() {
    let alpha = pixels(4096, 1024, |x, y| {
        u8::from((3700..4050).contains(&x) && (27..977).contains(&y)) * 255
    });
    let opts = AlphaMeshOptions {
        inside_spacing: 60.0,
        outside_spacing: 40.0,
        outside_margin: 3.0,
        minimum_margin: 2.0,
        inside_margin: 2.0,
        ..options()
    };
    let mesh = generate(4096, 1024, &alpha, &opts);
    valid(&mesh, 4096, 1024);
    let min_x = mesh
        .positions
        .iter()
        .map(|p| p.x)
        .fold(f32::INFINITY, f32::min);
    let max_x = mesh
        .positions
        .iter()
        .map(|p| p.x)
        .fold(f32::NEG_INFINITY, f32::max);
    assert!((min_x - 3697.0).abs() < 0.001);
    assert!((max_x - 4053.0).abs() < 0.001);
    assert!(mesh.positions.len() < 5000);
    assert!(covers(&mesh, 3700.1, 47.1));
    assert!(covers(&mesh, 4049.9, 996.9));
}

fn mesh_edges(mesh: &MeshGeometry) -> BTreeMap<(u32, u32), usize> {
    let mut edges = BTreeMap::new();
    for t in &mesh.triangles {
        for i in 0..3 {
            let (a, b) = (t[i], t[(i + 1) % 3]);
            *edges.entry((a.min(b), a.max(b))).or_default() += 1;
        }
    }
    edges
}

#[test]
fn continuous_inner_ring_separates_boundary_from_deep_vertices() {
    let alpha = pixels(256, 256, |x, y| {
        u8::from((32..224).contains(&x) && (32..224).contains(&y)) * 255
    });
    let mesh = generate(256, 256, &alpha, &AlphaMeshOptions::deformation_small());
    valid(&mesh, 256, 256);
    let edges = mesh_edges(&mesh);
    let boundary: BTreeSet<_> = edges
        .iter()
        .filter(|(_, n)| **n == 1)
        .flat_map(|(&(a, b), _)| [a, b])
        .collect();
    // The inset rectangle is [46,210]^2. No boundary edge may skip its ring
    // and reach a vertex strictly inside that rectangle.
    for &(a, b) in edges.keys() {
        for (outer, other) in [(a, b), (b, a)] {
            let p = mesh.positions[other as usize];
            if boundary.contains(&outer) {
                assert!(
                    !(p.x > 46.01 && p.x < 209.99 && p.y > 46.01 && p.y < 209.99),
                    "boundary bypassed the inner ring: {p:?}"
                );
            }
        }
    }
    // Every inset side is a continuous chain of mesh edges (with shared corners).
    for axis in 0..2 {
        for side in [46.0, 210.0] {
            let mut length = 0.0;
            for &(a, b) in edges.keys() {
                let a = mesh.positions[a as usize];
                let b = mesh.positions[b as usize];
                let (u, v) = if axis == 0 {
                    ([a.x, a.y], [b.x, b.y])
                } else {
                    ([a.y, a.x], [b.y, b.x])
                };
                if (u[0] - side).abs() < 0.001
                    && (v[0] - side).abs() < 0.001
                    && (46.0..=210.0).contains(&u[1])
                    && (46.0..=210.0).contains(&v[1])
                {
                    length += (u[1] - v[1]).abs();
                }
            }
            assert!((length - 164.0).abs() < 0.01, "broken inset side: {length}");
        }
    }
    assert!(covers(&mesh, 128.0, 128.0), "inner ring became a hole");
}

#[test]
fn collapsed_insets_have_distributed_support_in_isolated_and_attached_strips() {
    for attached in [false, true] {
        let alpha = pixels(256, 200, |x, y| {
            u8::from(
                ((20..236).contains(&x) && (96..104).contains(&y))
                    || (attached && (20..120).contains(&x) && (20..180).contains(&y)),
            ) * 255
        });
        let mesh = generate(256, 200, &alpha, &AlphaMeshOptions::deformation_small());
        valid(&mesh, 256, 200);
        for (left, right) in [(130.0, 190.0), (190.0, 236.0)] {
            assert!(
                mesh.positions
                    .iter()
                    .any(|p| p.x >= left && p.x <= right && p.y > 96.0 && p.y < 104.0),
                "unsupported strip span {left}..{right}, attached={attached}"
            );
        }
        coverage(&mesh, 256, 200, &alpha, 0, false);
        assert_eq!(
            mesh,
            generate(256, 200, &alpha, &AlphaMeshOptions::deformation_small())
        );
    }
}

#[test]
fn internal_constraints_preserve_real_holes_and_nested_islands() {
    let alpha = pixels(80, 80, |x, y| {
        u8::from(
            (4..76).contains(&x)
                && (4..76).contains(&y)
                && (!((24..56).contains(&x) && (24..56).contains(&y))
                    || ((36..44).contains(&x) && (36..44).contains(&y))),
        ) * 255
    });
    let mesh = generate(
        80,
        80,
        &alpha,
        &AlphaMeshOptions {
            inside_margin: 6.0,
            inside_spacing: 12.0,
            outside_spacing: 12.0,
            ..options()
        },
    );
    valid(&mesh, 80, 80);
    coverage(&mesh, 80, 80, &alpha, 0, true);
}

#[test]
fn dense_interior_retains_equilateral_lattice() {
    let alpha = pixels(512, 512, |x, y| {
        u8::from((20..492).contains(&x) && (20..492).contains(&y)) * 255
    });
    // Exercise an explicit dense lattice independently of UI preset tuning.
    let mesh = generate(
        512,
        512,
        &alpha,
        &AlphaMeshOptions {
            outside_spacing: 25.0,
            inside_spacing: 25.0,
            ..AlphaMeshOptions::deformation_large()
        },
    );
    valid(&mesh, 512, 512);
    let mut count = 0;
    for t in &mesh.triangles {
        let p = t.map(|i| mesh.positions[i as usize]);
        if p.iter()
            .all(|p| (80.0..432.0).contains(&p.x) && (80.0..432.0).contains(&p.y))
        {
            for i in 0..3 {
                let a = p[i];
                let b = p[(i + 1) % 3];
                assert!(((a.x - b.x).hypot(a.y - b.y) - 25.0).abs() < 0.001);
            }
            count += 1;
        }
    }
    assert!(
        count > 300,
        "dense core has only {count} equilateral triangles"
    );
}

#[test]
fn spanned_large_holes_keep_island_support_without_a_void_lattice() {
    let alpha = pixels(128, 128, |x, y| {
        u8::from(
            (12..116).contains(&x)
                && (12..116).contains(&y)
                && (!((32..96).contains(&x) && (32..96).contains(&y))
                    || ((60..68).contains(&x) && (60..68).contains(&y))),
        ) * 255
    });
    let opts = AlphaMeshOptions::deformation_large();
    assert!(!opts.preserve_holes);
    let mesh = generate(128, 128, &alpha, &opts);
    valid(&mesh, 128, 128);
    assert!(
        covers(&mesh, 48.0, 64.0),
        "transparent hole was not spanned"
    );
    assert!(
        mesh.positions
            .iter()
            .any(|p| (60.0..68.0).contains(&p.x) && (60.0..68.0).contains(&p.y)),
        "foreground island inside the hole lost all support"
    );
    for p in &mesh.positions {
        if (40.0..88.0).contains(&p.x) && (40.0..88.0).contains(&p.y) {
            assert!(
                (58.0..70.0).contains(&p.x) && (58.0..70.0).contains(&p.y),
                "unnecessary lattice vertex inside large transparent hole: {p:?}"
            );
        }
    }
    coverage(&mesh, 128, 128, &alpha, 0, false);
    let preserved = generate(
        128,
        128,
        &alpha,
        &AlphaMeshOptions {
            preserve_holes: true,
            ..opts
        },
    );
    assert!(!covers(&preserved, 48.0, 64.0));
    assert!(covers(&preserved, 64.0, 64.0));
}

#[test]
fn sub_spacing_holes_do_not_add_dense_internal_rings() {
    let filled = pixels(160, 160, |x, y| {
        u8::from((12..148).contains(&x) && (12..148).contains(&y)) * 255
    });
    let perforated = pixels(160, 160, |x, y| {
        if (72..80).contains(&x) && (60..100).contains(&y) {
            0
        } else {
            filled[(y * 160 + x) as usize]
        }
    });
    let opts = AlphaMeshOptions::deformation_large();
    let expected = generate(160, 160, &filled, &opts);
    let mesh = generate(160, 160, &perforated, &opts);
    valid(&mesh, 160, 160);
    assert_eq!(
        mesh, expected,
        "a sub-spacing hole changed the interior support layout"
    );
}

#[test]
fn tiny_island_in_a_spanned_hole_gets_a_local_support_vertex() {
    let alpha = pixels(128, 128, |x, y| {
        u8::from(
            ((12..116).contains(&x)
                && (12..116).contains(&y)
                && !((32..96).contains(&x) && (32..96).contains(&y)))
                || (x == 64 && y == 64),
        ) * 255
    });
    let mesh = generate(128, 128, &alpha, &AlphaMeshOptions::deformation_large());
    valid(&mesh, 128, 128);
    assert!(
        mesh.positions
            .iter()
            .any(|p| p.x > 64.0 && p.x < 65.0 && p.y > 63.0 && p.y < 64.0),
        "covered island has no local deformation support"
    );
}

#[test]
fn cropped_diagonal_artwork_does_not_densify_the_whole_boundary() {
    let (width, height) = (512, 256);
    let alpha = pixels(width, height, |x, y| {
        let taper = 0.38 * height as f64 * (1. - ((x as f64 + 0.5) / width as f64 * 2. - 1.).abs());
        if y as f64 + 0.5 >= taper && (y as f64 + 0.5) < height as f64 - taper {
            255
        } else {
            0
        }
    });
    let opts = AlphaMeshOptions {
        minimum_boundary_points: 15,
        ..AlphaMeshOptions::deformation_small()
    };
    let mesh = generate(width, height, &alpha, &opts);
    valid(&mesh, width, height);
    let boundary = mesh_edges(&mesh).values().filter(|&&n| n == 1).count();
    assert!(
        boundary < 60,
        "clipping forced {boundary} boundary samples at spacing 80"
    );
    for y in 0..height {
        for x in 0..width {
            if alpha[(y * width + x) as usize] != 0 {
                assert!(covers(
                    &mesh,
                    x as f32 + 0.5,
                    height as f32 - y as f32 - 0.5
                ));
            }
        }
    }
}

#[test]
fn unbounded_margins_cover_edge_artwork_without_flattening_the_ring() {
    let opts = AlphaMeshOptions {
        outside_margin: 8.,
        minimum_margin: 3.,
        inside_margin: 4.,
        clip_to_image: false,
        ..options()
    };
    let mask = vec![255; 40 * 60];
    let mesh = generate(40, 60, &mask, &opts);
    coverage(&mesh, 40, 60, &mask, 0, false);
    for predicate in [
        |p: &Vec2| p.x < -3.,
        |p: &Vec2| p.y < -3.,
        |p: &Vec2| p.x > 43.,
        |p: &Vec2| p.y > 63.,
    ] {
        assert!(mesh.positions.iter().any(predicate));
    }
    for (p, uv) in mesh.positions.iter().zip(&mesh.uvs) {
        assert_eq!(uv.x, p.x / 40.);
        assert_eq!(uv.y, p.y / 60.);
    }
    assert!(mesh.triangles.iter().all(|t| {
        let [a, b, c] = t.map(|i| mesh.positions[i as usize]);
        cross(a, b, c) > 0.
    }));
}

#[test]
fn faint_island_filter_preserves_small_opaque_and_connected_faint_artwork() {
    let alpha = pixels(96, 64, |x, y| {
        if ((32..70).contains(&x) && (10..40).contains(&y)) || (x == 5 && y == 50) {
            255
        } else if (12..32).contains(&x) && y == 20 {
            1
        } else if (3..6).contains(&x) && (3..6).contains(&y) {
            40
        } else if (72..94).contains(&x) && (42..62).contains(&y) {
            4
        } else {
            0
        }
    });
    let filtered = generate(
        96,
        64,
        &alpha,
        &AlphaMeshOptions {
            remove_faint_islands: true,
            ..options()
        },
    );
    let exact = generate(96, 64, &alpha, &options());
    assert!(covers(&exact, 4.5, 64. - 4.5));
    assert!(!covers(&filtered, 4.5, 64. - 4.5));
    for (x, y) in [(5.5, 50.5), (12.5, 20.5), (72.5, 42.5), (33.5, 11.5)] {
        assert!(covers(&filtered, x, 64. - y));
    }
    let faint = pixels(12, 12, |x, y| if x == y { 1 } else { 0 });
    let all_faint = generate(
        12,
        12,
        &faint,
        &AlphaMeshOptions {
            remove_faint_islands: true,
            ..options()
        },
    );
    coverage(&all_faint, 12, 12, &faint, 0, true);
}

#[test]
fn target_spacing_does_not_bisect_every_slightly_longer_edge() {
    let alpha = pixels(128, 128, |x, y| {
        u8::from((20..101).contains(&x) && (20..101).contains(&y)) * 255
    });
    let mesh = generate(
        128,
        128,
        &alpha,
        &AlphaMeshOptions {
            outside_spacing: 80.,
            inside_spacing: 1000.,
            ..options()
        },
    );
    valid(&mesh, 128, 128);
    coverage(&mesh, 128, 128, &alpha, 0, true);
    let lengths: Vec<_> = mesh_edges(&mesh)
        .iter()
        .filter(|(_, n)| **n == 1)
        .map(|(&(a, b), _)| {
            let (a, b) = (mesh.positions[a as usize], mesh.positions[b as usize]);
            (a.x - b.x).hypot(a.y - b.y)
        })
        .collect();
    assert!(
        lengths.iter().all(|&length| length > 60. && length < 100.),
        "oversampled ring: {lengths:?}"
    );
}

#[test]
fn smooth_circle_samples_follow_spacing_without_losing_padding() {
    let alpha = pixels(256, 256, |x, y| {
        if (x as f64 + 0.5 - 128.0).hypot(y as f64 + 0.5 - 128.0) <= 115.0 {
            255
        } else {
            0
        }
    });
    let options = AlphaMeshOptions {
        outside_spacing: 40.0,
        inside_spacing: 40.0,
        outside_margin: 7.0,
        inside_margin: 7.0,
        minimum_margin: 2.5,
        minimum_boundary_points: 15,
        ..AlphaMeshOptions::deformation_small()
    };
    let mesh = generate(256, 256, &alpha, &options);
    valid(&mesh, 256, 256);
    coverage(&mesh, 256, 256, &alpha, 0, false);
    let mut edges = BTreeMap::new();
    for t in &mesh.triangles {
        for i in 0..3 {
            let (a, b) = (t[i], t[(i + 1) % 3]);
            *edges.entry((a.min(b), a.max(b))).or_insert(0) += 1;
        }
    }
    let boundary: Vec<_> = edges.iter().filter(|(_, n)| **n == 1).collect();
    // Circumference / target spacing is about 20. RDP used to leave 16
    // slightly-long edges, each split again, producing 32 boundary vertices.
    assert!(
        (19..=23).contains(&boundary.len()),
        "boundary has {} edges",
        boundary.len()
    );
    let mean_radius = boundary
        .iter()
        .map(|((a, _), _)| {
            let p = mesh.positions[*a as usize];
            (p.x - 128.0).hypot(p.y - 128.0)
        })
        .sum::<f32>()
        / boundary.len() as f32;
    assert!(
        (121.4..=123.0).contains(&mean_radius),
        "resampling shrank the requested offset: radius {mean_radius}"
    );
    for (&(a, b), _) in boundary {
        let a = mesh.positions[a as usize];
        let b = mesh.positions[b as usize];
        let length = (a.x - b.x).hypot(a.y - b.y);
        assert!(
            (24.0..=44.0).contains(&length),
            "uneven smooth contour edge {length}"
        );
    }
    for degree in 0..360 {
        let angle = (degree as f32).to_radians();
        assert!(
            covers(
                &mesh,
                128.0 + 116.5 * angle.cos(),
                128.0 + 116.5 * angle.sin()
            ),
            "minimum padding cut at {degree} degrees"
        );
    }
    let repeated = generate(256, 256, &alpha, &options);
    assert_eq!(mesh.positions, repeated.positions);
    assert_eq!(mesh.triangles, repeated.triangles);
    let dense = generate(
        256,
        256,
        &alpha,
        &AlphaMeshOptions {
            minimum_boundary_points: 48,
            ..options
        },
    );
    valid(&dense, 256, 256);
    assert!(dense.positions.len() > mesh.positions.len());
}
