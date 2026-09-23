use super::*;
use std::collections::HashMap;
use std::sync::Arc;

use kasane_core::evaluation::{Drawable, RenderCommand};
use kasane_core::types::{Canvas, Vec2};
use kasane_render::{Affine2, TextureInfo};

fn drawable(id: &str) -> Drawable {
    Drawable {
        id: id.to_owned(),
        texture_asset_id: "texture".to_owned(),
        positions: vec![
            Vec2::new(0.0, 0.0),
            Vec2::new(1.0, 0.0),
            Vec2::new(0.0, 1.0),
        ],
        uvs: Arc::from([
            Vec2::new(0.0, 0.0),
            Vec2::new(1.0, 0.0),
            Vec2::new(0.0, 1.0),
        ]),
        indices: Arc::from([0, 1, 2]),
        ..Default::default()
    }
}

fn planner() -> WgpuFramePlanner {
    WgpuFramePlanner::new(WgpuTargetConfig {
        width: 640,
        height: 480,
        ..Default::default()
    })
    .unwrap()
}

fn viewport() -> ViewportConfig {
    ViewportConfig {
        transform: Affine2::IDENTITY,
        target_extent: Vec2::new(640.0, 480.0),
        mask_scale: 1.0,
    }
}

fn textures() -> HashMap<String, TextureInfo> {
    HashMap::from([(
        "texture".to_owned(),
        TextureInfo {
            width: 64,
            height: 64,
        },
    )])
}

#[test]
fn prepares_flat_normal_draws_without_a_gpu_device() {
    let frame = DrawableFrame {
        canvas: Canvas::new(640.0, 480.0, Vec2::new(320.0, 240.0), 100.0),
        drawables: vec![drawable("mesh")],
        ..Default::default()
    };
    let prepared = planner()
        .prepare_basic(&frame, &textures(), viewport())
        .unwrap();
    assert_eq!(prepared.target.width, 640);
    assert_eq!(prepared.draws[0].drawable_id, "mesh");
    assert_eq!(prepared.draws[0].index_count, 3);
}

#[test]
fn rejects_offscreen_passes_before_gpu_allocation() {
    let frame = DrawableFrame {
        canvas: Canvas::new(640.0, 480.0, Vec2::new(320.0, 240.0), 100.0),
        drawables: vec![drawable("mesh")],
        render_plan: vec![
            kasane_core::evaluation::RenderCommand::BeginOffscreen {
                offscreen_id: "surface".to_owned(),
            },
            kasane_core::evaluation::RenderCommand::DrawMesh {
                mesh_id: "mesh".to_owned(),
            },
            kasane_core::evaluation::RenderCommand::EndOffscreen {
                offscreen_id: "surface".to_owned(),
            },
        ],
        offscreens: vec![kasane_core::evaluation::OffscreenFrame {
            id: "surface".to_owned(),
            enabled: true,
            opacity: 1.0,
            ..Default::default()
        }],
        ..Default::default()
    };
    let failure = planner()
        .prepare_basic(&frame, &textures(), viewport())
        .unwrap_err();
    assert_eq!(failure.code, "UNSUPPORTED_WGPU_FEATURE");
}

#[test]
fn rejects_non_normal_blends_at_the_backend_boundary() {
    let mut mesh = drawable("mesh");
    mesh.blend_mode = BlendMode::Additive;
    let frame = DrawableFrame {
        canvas: Canvas::new(640.0, 480.0, Vec2::new(320.0, 240.0), 100.0),
        drawables: vec![mesh],
        ..Default::default()
    };
    let failure = planner()
        .prepare_basic(&frame, &textures(), viewport())
        .unwrap_err();
    assert_eq!(failure.code, "UNSUPPORTED_WGPU_FEATURE");
}

#[test]
fn omits_invisible_and_transparent_draws_from_basic_plan() {
    let mut invisible = drawable("invisible");
    invisible.visible = false;
    let mut transparent = drawable("transparent");
    transparent.opacity = 0.0;
    let frame = DrawableFrame {
        canvas: Canvas::new(640.0, 480.0, Vec2::new(320.0, 240.0), 100.0),
        drawables: vec![invisible, transparent],
        ..Default::default()
    };

    let prepared = planner()
        .prepare_basic(&frame, &textures(), viewport())
        .unwrap();
    assert!(prepared.draws.is_empty());
}

#[test]
fn reverses_triangle_winding_for_y_down_targets() {
    assert_eq!(
        triangle_indices(&[0, 1, 2, 3, 4, 5]),
        vec![0, 2, 1, 3, 5, 4]
    );
}

#[test]
fn prepares_normal_offscreen_scene_without_a_gpu_device() {
    let frame = DrawableFrame {
        canvas: Canvas::new(640.0, 480.0, Vec2::new(320.0, 240.0), 100.0),
        drawables: vec![drawable("mesh")],
        offscreens: vec![kasane_core::evaluation::OffscreenFrame {
            id: "surface".to_owned(),
            enabled: true,
            opacity: 1.0,
            ..Default::default()
        }],
        render_plan: vec![
            RenderCommand::BeginOffscreen {
                offscreen_id: "surface".to_owned(),
            },
            RenderCommand::DrawMesh {
                mesh_id: "mesh".to_owned(),
            },
            RenderCommand::EndOffscreen {
                offscreen_id: "surface".to_owned(),
            },
        ],
        ..Default::default()
    };

    let prepared = planner()
        .prepare_scene(&frame, &textures(), viewport())
        .unwrap();
    assert!(prepared.active_offscreens.contains("surface"));
    assert_eq!(prepared.surface_size.width, 640);
    assert_eq!(
        build_scene(&prepared).unwrap().main,
        vec![SceneEvent::Composite("surface")]
    );
}

#[test]
fn prepares_destination_read_scene_without_a_gpu_device() {
    let mut mesh = drawable("mesh");
    mesh.raw_blend_mode = Some(1);
    let frame = DrawableFrame {
        canvas: Canvas::new(640.0, 480.0, Vec2::new(320.0, 240.0), 100.0),
        drawables: vec![mesh],
        ..Default::default()
    };

    let prepared = planner()
        .prepare_scene(&frame, &textures(), viewport())
        .unwrap();
    assert!(prepared.destination_reads.contains("mesh"));
    assert_eq!(
        build_scene(&prepared).unwrap().main,
        vec![SceneEvent::Draw(DrawItem {
            drawable_id: "mesh",
            texture_id: "texture"
        })]
    );
    assert_eq!(
        planner()
            .prepare_basic(&frame, &textures(), viewport())
            .unwrap_err()
            .code,
        "UNSUPPORTED_WGPU_FEATURE"
    );
}

#[test]
fn prepares_masked_scene_and_keeps_mask_out_of_scene_order() {
    let source = drawable("source");
    let mut target = drawable("target");
    target.masks.push("source".to_owned());
    let frame = DrawableFrame {
        canvas: Canvas::new(640.0, 480.0, Vec2::new(320.0, 240.0), 100.0),
        drawables: vec![source, target],
        ..Default::default()
    };

    let prepared = planner()
        .prepare_scene(&frame, &textures(), viewport())
        .unwrap();
    assert!(prepared.mask_consumers.is_empty());
    let scene = build_scene(&prepared).unwrap();
    assert_eq!(
        scene.main,
        vec![
            SceneEvent::Draw(DrawItem {
                drawable_id: "source",
                texture_id: "texture"
            }),
            SceneEvent::Draw(DrawItem {
                drawable_id: "target",
                texture_id: "texture"
            })
        ]
    );
    assert_eq!(
        mask_layout(&frame, &["source".to_owned()], 1.0)
            .unwrap()
            .logical_size,
        Vec2::new(108.0, 108.0)
    );
    assert_eq!(
        planner()
            .prepare_basic(&frame, &textures(), viewport())
            .unwrap_err()
            .code,
        "UNSUPPORTED_WGPU_FEATURE"
    );
}

#[test]
fn scene_parser_keeps_nested_composites_in_parent_order() {
    let prepared = PreparedFrame {
        active_offscreens: HashSet::from(["outer", "inner"]),
        mask_consumers: HashMap::new(),
        destination_reads: HashSet::new(),
        surface_size: Size2 {
            width: 640,
            height: 480,
        },
        surface_transform: Affine2::IDENTITY,
        passes: vec![
            RenderPass::Main,
            RenderPass::Draw(DrawItem {
                drawable_id: "root",
                texture_id: "texture",
            }),
            RenderPass::Offscreen {
                id: "outer",
                parent: None,
            },
            RenderPass::Composite {
                id: "outer",
                parent: None,
            },
            RenderPass::Draw(DrawItem {
                drawable_id: "outer-mesh",
                texture_id: "texture",
            }),
            RenderPass::Offscreen {
                id: "inner",
                parent: Some("outer"),
            },
            RenderPass::Composite {
                id: "inner",
                parent: Some("outer"),
            },
            RenderPass::Draw(DrawItem {
                drawable_id: "inner-mesh",
                texture_id: "texture",
            }),
            RenderPass::EndOffscreen { id: "inner" },
            RenderPass::EndOffscreen { id: "outer" },
        ],
    };

    let scene = build_scene(&prepared).unwrap();
    assert_eq!(
        scene.main,
        vec![
            SceneEvent::Draw(DrawItem {
                drawable_id: "root",
                texture_id: "texture"
            }),
            SceneEvent::Composite("outer")
        ]
    );
    assert_eq!(
        scene.surfaces["outer"],
        vec![
            SceneEvent::Draw(DrawItem {
                drawable_id: "outer-mesh",
                texture_id: "texture"
            }),
            SceneEvent::Composite("inner")
        ]
    );
    assert_eq!(
        scene.surfaces["inner"],
        vec![SceneEvent::Draw(DrawItem {
            drawable_id: "inner-mesh",
            texture_id: "texture"
        })]
    );
}

#[test]
fn validates_surface_extent_and_affine_round_trip() {
    assert_eq!(
        surface_extent(Size2 {
            width: 3,
            height: 4
        }),
        Ok((3, 4))
    );
    assert!(surface_extent(Size2 {
        width: 0,
        height: 4
    })
    .is_err());
    assert!(surface_extent(Size2 {
        width: 3,
        height: -1
    })
    .is_err());

    let transform = Affine2 {
        a: Vec2::new(2.0, 0.25),
        b: Vec2::new(-0.5, 1.5),
        origin: Vec2::new(12.0, -7.0),
    };
    let inverse = inverse_affine(transform).unwrap();
    let point = Vec2::new(8.0, 13.0);
    let round_trip = compose_affine(inverse, transform).transform_point(point);
    assert!((round_trip.x - point.x).abs() < 1.0e-5);
    assert!((round_trip.y - point.y).abs() < 1.0e-5);
}
