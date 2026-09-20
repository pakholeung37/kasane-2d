use std::collections::HashMap;

use kasane_core::{
    evaluate_frame, BindingAxis, Canvas, Document, DrawableFrame, ImageAsset, Mesh, MeshBinding,
    MeshKeyform, Parameter, Vec2, VertexId,
};

fn id(n: i32) -> String {
    format!("{}1111111-1111-4111-8111-111111111111", n)
}

#[test]
fn test_keyform_data_editing_and_evaluation() {
    let mut d = Document::new();
    assert!(d
        .initialize(
            id(1),
            Canvas::new(100.0, 100.0, Vec2::new(50.0, 50.0), 100.0)
        )
        .is_ok());

    assert!(d
        .add_asset(ImageAsset {
            id: id(2),
            name: "asset".to_string(),
            source: "memory://unloaded".to_string(),
            width: 8,
            height: 8,
            ..Default::default()
        })
        .status
        .is_ok());

    let m = Mesh {
        id: id(3),
        name: "mesh".to_string(),
        texture_asset_id: id(2),
        vertex_ids: vec![9, 1, 7],
        base_positions: vec![
            Vec2::new(10.0, 10.0),
            Vec2::new(40.0, 10.0),
            Vec2::new(20.0, 40.0),
        ],
        uvs: vec![
            Vec2::new(0.0, 0.0),
            Vec2::new(1.0, 0.0),
            Vec2::new(0.5, 1.0),
        ],
        triangles: vec![[9, 1, 7]],
        runtime_id: "Mesh".to_string(),
        ..Default::default()
    };
    assert!(d.create_mesh(m.clone()).status.is_ok());

    let e = d.create_parameter(Parameter {
        id: id(4),
        name: "Param".to_string(),
        runtime_id: "parameter".to_string(),
        minimum: -1.0,
        maximum: 1.0,
        default_value: 0.0,
        decimal_places: 6,
        ..Default::default()
    });
    assert!(e.status.is_ok() && e.changes.object_ids == vec![id(4)]);

    let mut b = MeshBinding {
        id: id(5),
        mesh_id: id(3),
        axes: vec![BindingAxis {
            parameter_id: id(4),
            keys: vec![-1.0, 0.0, 1.0],
        }],
        keyforms: Vec::new(),
    };
    for key in [1.0f32, 0.0, -1.0] {
        let mut positions = m.base_positions.clone();
        for p in &mut positions {
            p.x += key * 20.0;
        }
        b.keyforms.push(MeshKeyform {
            keys: vec![key],
            positions,
            ..Default::default()
        });
    }
    assert!(d.create_binding(b).status.is_ok());
    assert_eq!(d.get_binding(&id(5)).unwrap().keyforms[0].keys[0], -1.0);

    d.mark_saved();
    let rev = d.revision();
    let mut frame = DrawableFrame::default();
    let mut preview = HashMap::new();
    preview.insert(id(4), 0.5f32);
    assert!(evaluate_frame(&d, &preview, &mut frame).is_ok());
    assert!((frame.drawables[0].positions[0].x + 0.3).abs() < 1e-6);
    assert_eq!(d.revision(), rev);
    assert!(!d.modified());

    let preserved = frame.drawables[0].positions.clone();
    let mut bad_preview = HashMap::new();
    bad_preview.insert(id(4), f32::NAN);
    assert!(!evaluate_frame(&d, &bad_preview, &mut frame).is_ok());
    assert_eq!(frame.drawables[0].positions, preserved);

    let mut unknown_preview = HashMap::new();
    unknown_preview.insert(id(6), 0.0f32);
    assert!(!evaluate_frame(&d, &unknown_preview, &mut frame).is_ok());

    let mut clamped_preview = HashMap::new();
    clamped_preview.insert(id(4), 10.0f32);
    assert!(evaluate_frame(&d, &clamped_preview, &mut frame).is_ok());
    assert!(frame.parameters[0].clamped && frame.parameters[0].value == 1.0);

    let vertex: VertexId = 9;
    let changed_base = Vec2::new(999.0, 10.0);
    assert!(d
        .set_vertex_positions(&id(3), &[vertex], &[changed_base])
        .status
        .is_ok());
    assert!(evaluate_frame(&d, &preview, &mut frame).is_ok());
    assert_eq!(frame.drawables[0].positions, preserved);

    let mut invalid = d.get_binding(&id(5)).unwrap().clone();
    invalid.axes[0].keys = vec![-1.0, 0.0, 0.0];
    let rev2 = d.revision();
    assert!(!d.replace_binding(invalid).status.is_ok());
    assert_eq!(d.revision(), rev2);

    let mut invalid = d.get_binding(&id(5)).unwrap().clone();
    invalid.keyforms[1].positions.pop();
    assert_eq!(d.replace_binding(invalid).status.code, "INVALID_LENGTH");

    assert_eq!(d.erase_object(&id(2)).referrers, vec![id(3)]);
    assert_eq!(d.erase_object(&id(3)).referrers, vec![id(5)]);
    assert_eq!(d.erase_object(&id(4)).referrers, vec![id(5)]);

    assert!(d.begin_transaction().is_ok());
    assert!(!d.erase_object(&id(5)).status.is_ok());
    assert!(d.cancel_transaction().is_ok());

    assert!(d.erase_object(&id(5)).status.is_ok());
    assert!(evaluate_frame(&d, &HashMap::new(), &mut frame).is_ok());
    assert!((frame.drawables[0].positions[0].x - 9.49).abs() < 1e-5);

    assert!(d.erase_object(&id(4)).status.is_ok());
    assert!(d.erase_object(&id(3)).status.is_ok());
    assert!(d.erase_object(&id(2)).status.is_ok());
    assert!(evaluate_frame(&d, &HashMap::new(), &mut frame).is_ok());
    assert!(frame.drawables.is_empty());
}
