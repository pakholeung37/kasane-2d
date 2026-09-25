//! Build a self-contained animation package from the repository's small model.
use std::path::PathBuf;

use kasane_core::document::{MotionGroup, MotionRegistration, PackageAttachment};
use kasane_core::ChangeKind;
use kasane_project::{
    import_expression3, import_motion3, import_physics3, import_pose3, DocumentSession,
};

const EXPRESSION: &str = "00000000-0000-4000-8000-000000000e21";
const MOTION: &str = "00000000-0000-4000-8000-000000000e22";
const PHYSICS: &str = "00000000-0000-4000-8000-000000000e23";
const POSE: &str = "00000000-0000-4000-8000-000000000e24";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let destination = PathBuf::from(
        std::env::args()
            .nth(1)
            .ok_or("package destination required")?,
    );
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or("modules directory")?
        .parent()
        .ok_or("repo root")?
        .to_path_buf();
    let mut session = DocumentSession::new();
    let (result, _) =
        session.import_model3(&root.join("tests/fixtures/external_v50/model.model3.json"));
    if !result.status.is_ok() {
        return Err(result.status.message.into());
    }
    let mut candidate = session.document().fork_candidate();
    if !candidate.set_missing_attachments(Vec::new()).status.is_ok() {
        return Err("cannot discard absent fixture references".into());
    }
    candidate = import_expression3(
        &candidate,
        EXPRESSION,
        "Smile",
        &std::fs::read_to_string(root.join("tests/fixtures/animation_cpu/minimal.exp3.json"))?,
    )?
    .candidate;
    candidate = import_motion3(
        &candidate,
        MOTION,
        "Idle",
        &std::fs::read_to_string(root.join("tests/fixtures/animation_cpu/loop.motion3.json"))?,
    )?
    .candidate;
    candidate = import_physics3(
        &candidate,
        PHYSICS,
        &std::fs::read_to_string(root.join("tests/fixtures/animation_cpu/thirty.physics3.json"))?,
    )?
    .candidate;
    candidate = import_pose3(
        &candidate,
        POSE,
        r#"{"Type":"Live2D Pose","Groups":[[{"Id":"Part0"}]]}"#,
    )?
    .candidate;
    let groups = vec![MotionGroup {
        name: "Idle".into(),
        entries: vec![MotionRegistration {
            clip_id: MOTION.into(),
            fade_in: None,
            fade_out: None,
            sound: Some("sounds/probe.wav".into()),
            extensions: Default::default(),
        }],
    }];
    if !candidate.set_motion_groups(groups).status.is_ok() {
        return Err("cannot register motion".into());
    }
    let param = candidate
        .get_parameter(&candidate.parameter_order()[0])
        .ok_or("parameter")?
        .runtime_id
        .clone();
    let hit = candidate
        .get_mesh(&candidate.mesh_order()[0])
        .ok_or("drawable")?
        .runtime_id
        .clone();
    let settings = kasane_project::model3::import_settings(
        &candidate,
        &serde_json::json!({
            "Groups": [{"Target":"Parameter","Name":"EyeBlink","Ids":[param]}],
            "Layout": {"CenterX": 0.0, "Width": 2.0},
            "HitAreas": [{"Id": hit, "Name": "Head"}],
            "FileReferences": {"UserData": "data.userdata3.json"}
        }),
    )
    .map_err(|status| status.message)?;
    if !candidate.set_model3_settings(settings).status.is_ok() {
        return Err("cannot set model3 metadata".into());
    }
    if !candidate
        .set_package_attachments(vec![
            PackageAttachment {
                path: "sounds/probe.wav".into(),
                bytes: b"RIFF\0\0\0\0WAVE".to_vec(),
            },
            PackageAttachment {
                path: "data.userdata3.json".into(),
                bytes: br#"{"Version":3,"UserData":[]}"#.to_vec(),
            },
        ])
        .status
        .is_ok()
    {
        return Err("cannot add attachments".into());
    }
    session
        .publish_authoring_candidate(candidate, ChangeKind::Metadata, false)
        .map_err(|status| status.message)?;
    let result = session.export_package(&destination);
    if !result.status.is_ok() {
        return Err(result.status.message.into());
    }
    println!("{}", destination.display());
    Ok(())
}
