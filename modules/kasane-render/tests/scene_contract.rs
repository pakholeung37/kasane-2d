use kasane_core::evaluation::{Drawable, DrawableFrame, OffscreenFrame, RenderCommand};
use kasane_core::types::{Canvas, Vec2};
use kasane_render::{MaskId, MeshId, ScenePlan, TargetId, TargetItem, TextureInfo};
use std::collections::HashMap;
use std::sync::Arc;

fn textures() -> HashMap<String, TextureInfo> {
    HashMap::from([(
        "atlas".into(),
        TextureInfo {
            width: 16,
            height: 16,
        },
    )])
}
fn mesh(id: &str) -> Drawable {
    Drawable {
        id: id.into(),
        texture_asset_id: "atlas".into(),
        positions: vec![Vec2::new(0., 0.), Vec2::new(1., 0.), Vec2::new(0., 1.)],
        uvs: Arc::from([Vec2::default(); 3]),
        indices: Arc::from([0, 1, 2]),
        ..Default::default()
    }
}
fn frame() -> DrawableFrame {
    DrawableFrame {
        canvas: Canvas::new(64., 64., Vec2::new(32., 32.), 10.),
        drawables: vec![
            mesh("root-before"),
            mesh("source"),
            mesh("child"),
            mesh("root-after"),
        ],
        offscreens: vec![OffscreenFrame {
            id: "group".into(),
            enabled: true,
            opacity: 1.,
            masks: vec!["source".into()],
            ..Default::default()
        }],
        render_plan: vec![
            RenderCommand::DrawMesh {
                mesh_id: "root-before".into(),
            },
            RenderCommand::BeginOffscreen {
                offscreen_id: "group".into(),
            },
            RenderCommand::DrawMesh {
                mesh_id: "source".into(),
            },
            RenderCommand::DrawMesh {
                mesh_id: "child".into(),
            },
            RenderCommand::EndOffscreen {
                offscreen_id: "group".into(),
            },
            RenderCommand::DrawMesh {
                mesh_id: "root-after".into(),
            },
        ],
        ..Default::default()
    }
}

#[test]
fn targets_preserve_order_and_mask_inputs_do_not_depend_on_their_color_target() {
    let mut frame = frame();
    frame.drawables[1].visible = false;
    frame.drawables[1].opacity = 0.;
    frame.drawables[2].raw_blend_mode = Some(0);
    let mut scene = ScenePlan::default();
    scene.update(&frame, &textures()).unwrap();
    assert_eq!(
        scene.targets()[0].items,
        vec![
            TargetItem::Draw(MeshId(0)),
            TargetItem::Composite(TargetId(1)),
            TargetItem::Draw(MeshId(3))
        ]
    );
    assert_eq!(
        scene.targets()[1].items,
        vec![TargetItem::Draw(MeshId(1)), TargetItem::Draw(MeshId(2))]
    );
    assert_eq!(scene.targets()[1].parent, TargetId::MAIN);
    assert_eq!(scene.targets()[1].mask, Some(MaskId(0)));
    assert_eq!(scene.masks()[0].sources, vec![MeshId(1)]);
    assert!(scene.meshes()[2].reads_destination);
    assert!(scene.targets()[1].active);
    assert_eq!(scene.masks()[0].bounds.position, Vec2::new(32., 22.));
    assert_eq!(scene.masks()[0].bounds.size, Vec2::new(10., 10.));
}

#[test]
fn masks_share_logical_identity_across_consumers_and_ignore_duplicate_sources() {
    let mut frame = frame();
    frame.drawables[0].masks = vec!["source".into(), "source".into()];
    frame.drawables[2].masks = vec!["source".into()];
    let mut scene = ScenePlan::default();
    scene.update(&frame, &textures()).unwrap();
    assert_eq!(scene.masks().len(), 1);
    assert_eq!(scene.meshes()[0].mask, scene.meshes()[2].mask);
    assert_eq!(scene.meshes()[2].mask, scene.targets()[1].mask);
    assert_ne!(scene.meshes()[0].target, scene.meshes()[2].target);
    assert_eq!(scene.masks()[0].sources, vec![MeshId(1)]);
}

#[test]
fn frame_updates_change_dynamic_order_bounds_and_activity_without_retaining_frame() {
    let mut scene = ScenePlan::default();
    let mut frame = frame();
    scene.update(&frame, &textures()).unwrap();
    let name_storage = scene.meshes()[0].id.as_ptr();
    let item_storage = scene.targets()[0].items.as_ptr();
    frame.drawables[1].positions[1].x = 2.;
    frame.render_plan.swap(0, 5);
    frame.offscreens[0].opacity = 0.;
    let published = Arc::new(frame);
    scene.update(&published, &textures()).unwrap();
    assert_eq!(Arc::strong_count(&published), 1);
    assert!(Arc::try_unwrap(published).is_ok());
    assert_eq!(scene.meshes()[0].id.as_ptr(), name_storage);
    assert_eq!(scene.targets()[0].items.as_ptr(), item_storage);
    assert_eq!(scene.targets()[0].items[0], TargetItem::Draw(MeshId(3)));
    assert_eq!(scene.masks()[0].bounds.size.x, 20.);
    assert_eq!(scene.active_target_count(), 0);
}

#[test]
fn rejected_inputs_preserve_the_last_valid_scene_and_replacement_resolves_new_slots() {
    let mut scene = ScenePlan::default();
    let mut frame = frame();
    scene.update(&frame, &textures()).unwrap();
    let old_items = scene.targets()[0].items.clone();
    frame.drawables[0].id = "replacement".into(); // render stream is now dangling
    assert!(scene.update(&frame, &textures()).is_err());
    assert_eq!(scene.targets()[0].items, old_items);
    assert_eq!(scene.meshes()[0].id, "root-before");
    frame.render_plan[0] = RenderCommand::DrawMesh {
        mesh_id: "replacement".into(),
    };
    frame.drawables.swap(0, 3);
    scene.update(&frame, &textures()).unwrap();
    assert!(scene.mesh_id("root-before").is_none());
    assert_eq!(scene.mesh_id("replacement"), Some(MeshId(3)));
    assert_eq!(scene.targets()[0].items[0], TargetItem::Draw(MeshId(3)));
}

#[test]
fn flat_fallback_preserves_ties_and_empty_model_is_valid() {
    let mut frame = frame();
    frame.offscreens.clear();
    frame.render_plan.clear();
    frame.drawables[0].render_order = 3;
    let mut scene = ScenePlan::default();
    scene.update(&frame, &textures()).unwrap();
    assert_eq!(
        scene.targets()[0].items,
        vec![
            TargetItem::Draw(MeshId(1)),
            TargetItem::Draw(MeshId(2)),
            TargetItem::Draw(MeshId(3)),
            TargetItem::Draw(MeshId(0))
        ]
    );
    frame.drawables.clear();
    scene.update(&frame, &textures()).unwrap();
    assert!(scene.targets()[0].items.is_empty());
    assert!(scene.masks().is_empty());
}
