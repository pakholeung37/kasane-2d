use kasane_core::{RotationTransform, TransformData, WarpTransform};
use std::collections::HashMap;

use kasane_core::{
    evaluate_frame, evaluation::evaluate_frame_including_hidden, Appearance, Canvas, Document,
    DrawableFrame, ImageAsset, Mesh, Part, RotationPose, Transform, Vec2,
};

fn id(n: i32) -> String {
    format!("{:08x}-1111-4111-8111-111111111111", n)
}

#[test]
fn test_hierarchical_transforms_and_warp() {
    hierarchical_transforms_and_warp(1);
    hierarchical_transforms_and_warp(0);
}

fn hierarchical_transforms_and_warp(flag: u8) {
    let mut doc = Document::new();
    assert!(doc
        .initialize(
            id(1),
            Canvas::with_flag(640.0, 480.0, Vec2::new(320.0, 240.0), 100.0, flag)
        )
        .is_ok());

    assert!(doc
        .add_asset(ImageAsset {
            id: id(2),
            name: "texture".to_string(),
            source: "textures/0.png".to_string(),
            width: 128,
            height: 128,
            ..Default::default()
        })
        .status
        .is_ok());

    // Root Part
    assert!(doc
        .create_part(Part {
            id: id(3),
            runtime_id: "RootPart".to_string(),
            name: "Root Part".to_string(),
            parent_id: String::new(),
            enabled: true,
            draw_order: 1.0,
        })
        .status
        .is_ok());

    // Root Rotation Transform
    let mut root = Transform {
        id: id(4),
        runtime_id: "RootRot".to_string(),
        name: "Root Rotation".to_string(),
        part_id: kasane_core::PartId::optional(id(3)),
        parent_id: kasane_core::TransformId::optional(String::new()),
        appearance: Appearance {
            opacity: 1.0,
            multiply: [1.0, 1.0, 1.0],
            screen: [0.0, 0.0, 0.0],
        },
        data: TransformData::Rotation(RotationTransform {
            base_angle: 0.0,
            pose: RotationPose {
                origin: Vec2::new(320.0, 240.0).into(),
                angle: 0.0,
                scale: 1.0,
                reflect_x: false,
                reflect_y: false,
            },
        }),
        ..Default::default()
    };
    assert!(doc.create_transform(root.clone()).status.is_ok());

    // Child Warp Transform (2x2 grid = 9 points)
    let child = Transform {
        id: id(5),
        runtime_id: "ChildWarp".to_string(),
        name: "Child Warp".to_string(),
        part_id: kasane_core::PartId::optional(id(3)),
        parent_id: kasane_core::TransformId::optional(id(4)),
        appearance: Appearance {
            opacity: 0.8,
            multiply: [0.9, 0.9, 0.9],
            screen: [0.1, 0.0, 0.0],
        },
        data: TransformData::Warp(WarpTransform {
            bezier: None,
            rows: 2,
            columns: 2,
            quad: true,
            points: vec![
                Vec2::new(-50.0, -50.0),
                Vec2::new(0.0, -50.0),
                Vec2::new(50.0, -50.0),
                Vec2::new(-50.0, 0.0),
                Vec2::new(0.0, 0.0),
                Vec2::new(50.0, 0.0),
                Vec2::new(-50.0, 50.0),
                Vec2::new(0.0, 50.0),
                Vec2::new(50.0, 50.0),
            ],
        }),
        ..Default::default()
    };
    assert!(doc.create_transform(child.clone()).status.is_ok());

    // Mesh under child warp
    let mesh = Mesh {
        id: id(6),
        runtime_id: "TestMesh".to_string(),
        name: "Test Mesh".to_string(),
        texture_asset_id: id(2),
        part_id: id(3),
        deformer_id: id(5),
        vertex_ids: vec![0, 1, 2, 3],
        base_positions: vec![
            Vec2::new(0.0, 0.0),
            Vec2::new(0.5, 0.0),
            Vec2::new(0.5, 0.5),
            Vec2::new(0.0, 0.5),
        ],
        uvs: vec![
            Vec2::new(0.0, 0.0),
            Vec2::new(1.0, 0.0),
            Vec2::new(1.0, 1.0),
            Vec2::new(0.0, 1.0),
        ],
        triangles: vec![[0, 1, 2], [0, 2, 3]],
        appearance: Appearance {
            opacity: 1.0,
            multiply: [1.0, 1.0, 1.0],
            screen: [0.0, 0.0, 0.0],
        },
        ..Default::default()
    };
    assert!(doc.create_mesh(mesh).status.is_ok());

    // Frame evaluation
    let mut frame = DrawableFrame::default();
    assert!(evaluate_frame(&doc, &HashMap::new(), &mut frame).is_ok());
    assert_eq!(frame.drawables.len(), 1);
    let d = &frame.drawables[0];
    assert!(d.visible);
    assert_eq!(d.positions.len(), 4);
    assert!((d.opacity - 0.8).abs() < 1e-4);

    // Rotate root transform 90 degrees
    root.rotation_mut().unwrap().pose.angle = 90.0;
    assert!(doc.replace_transform(root).status.is_ok());
    assert!(evaluate_frame(&doc, &HashMap::new(), &mut frame).is_ok());
    let d2 = &frame.drawables[0];
    assert!(d2.visible);

    // Both kinds of evaluated controls must survive a frame round trip,
    // including their final coordinates for either canvas Y convention.
    assert_eq!(frame.deformers.len(), 2);
    let rotation = frame.deformers.iter().find(|d| d.id == id(4)).unwrap();
    let warp = frame.deformers.iter().find(|d| d.id == id(5)).unwrap();
    assert!(rotation.enabled && warp.enabled);
    assert_eq!(rotation.points.len(), 3);
    assert_eq!(warp.points.len(), 9);
    let json = serde_json::to_string(&frame).unwrap();
    let restored: DrawableFrame = serde_json::from_str(&json).unwrap();
    assert_eq!(restored, frame);

    // A PSD still needs the mesh's transformed pixels when its parent is
    // disabled. The normal preview continues to leave that geometry empty.
    let visible_positions = d2.positions.clone();
    let mut disabled_root = doc.get_transform(&id(4)).unwrap().clone();
    disabled_root.enabled = false;
    assert!(doc.replace_transform(disabled_root).status.is_ok());
    assert!(evaluate_frame(&doc, &HashMap::new(), &mut frame).is_ok());
    assert!(!frame.drawables[0].visible);
    assert!(frame.drawables[0]
        .positions
        .iter()
        .all(|point| *point == Vec2::default()));
    assert!(evaluate_frame_including_hidden(&doc, &HashMap::new(), &mut frame).is_ok());
    assert!(!frame.drawables[0].visible);
    assert_eq!(frame.drawables[0].positions, visible_positions);
}
