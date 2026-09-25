//! Immutable, inspectable package bytes and references, prepared before publication.
use super::{validate_package_paths, RuntimeValidation};
use crate::filesystem as io;
use crate::store::{asset_path, read_project_asset};
use crate::{export_cdi3, export_expression3, export_motion3, export_physics3, export_pose3};
use kasane_core::{types::Status, Document};
use kasane_moc3::encode_moc3;
use serde_json::Value;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFileKind {
    Moc,
    Model,
    Texture,
    DisplayInfo,
    Expression,
    Motion,
    Physics,
    Pose,
    Attachment,
}

#[derive(Debug)]
pub struct ExportFile {
    kind: ExportFileKind,
    bytes: Vec<u8>,
    sha256: String,
}
impl ExportFile {
    pub fn kind(&self) -> ExportFileKind {
        self.kind
    }
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
    pub fn sha256(&self) -> &str {
        &self.sha256
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageReference {
    pub from: String,
    pub to: String,
}

/// The constructor validates all paths/references and captures source bytes once.
/// Fields are private: validation and publication observe exactly these bytes.
#[derive(Debug)]
pub struct ExportPlan {
    pub(super) files: BTreeMap<String, ExportFile>,
    pub(super) report: Value,
    pub(super) source_paths: Vec<PathBuf>,
    references: Vec<PackageReference>,
    revision: u64,
}
impl ExportPlan {
    pub fn files(&self) -> &BTreeMap<String, ExportFile> {
        &self.files
    }
    pub fn references(&self) -> &[PackageReference] {
        &self.references
    }
    pub fn source_revision(&self) -> u64 {
        self.revision
    }
}

pub fn build_export_plan(doc: &Document, asset_root: &Path) -> Result<ExportPlan, Status> {
    if doc.transaction_active() {
        return Err(Status::error(
            "TRANSACTION_ACTIVE",
            "Commit or cancel edits first",
        ));
    }
    if !doc.missing_attachments().is_empty() {
        return Err(Status::error(
            "MISSING_PACKAGE_ATTACHMENT",
            doc.missing_attachments().join(", "),
        ));
    }
    let artifact = encode_moc3(doc)?;
    let cdi_json = export_cdi3(doc)
        .map_err(|error| Status::error(error.code, format!("{}: {}", error.path, error.message)))?;
    let mut expressions = Vec::new();
    for id in doc.expression_order() {
        let asset = doc.get_expression(id).expect("ordered expression exists");
        let content = export_expression3(doc, id).map_err(|error| {
            Status::error(error.code, format!("{}: {}", error.path, error.message))
        })?;
        expressions.push((
            format!("expressions/{id}.exp3.json"),
            asset.name.clone(),
            content,
        ));
    }
    let mut motions = Vec::new();
    for id in doc.motion_order() {
        let clip = doc.get_motion(id).expect("ordered motion exists");
        let content = export_motion3(doc, id).map_err(|error| {
            Status::error(error.code, format!("{}: {}", error.path, error.message))
        })?;
        motions.push((
            format!("motions/{id}.motion3.json"),
            clip.name.clone(),
            content,
        ));
    }
    let pose = export_pose3(doc)
        .map_err(|error| Status::error(error.code, format!("{}: {}", error.path, error.message)))?;
    let physics = export_physics3(doc)
        .map_err(|error| Status::error(error.code, format!("{}: {}", error.path, error.message)))?;
    let mut model3: Value = serde_json::from_str(&artifact.model3_json)
        .map_err(|error| Status::error("INVALID_MODEL3_JSON", error.to_string()))?;
    model3["FileReferences"]["DisplayInfo"] = Value::String("model.cdi3.json".into());
    if pose.is_some() {
        model3["FileReferences"]["Pose"] = Value::String("model.pose3.json".into());
    }
    if physics.is_some() {
        model3["FileReferences"]["Physics"] = Value::String("model.physics3.json".into());
    }
    if !expressions.is_empty() {
        model3["FileReferences"]["Expressions"] = Value::Array(
            expressions
                .iter()
                .map(|(path, name, _)| serde_json::json!({"Name": name, "File": path}))
                .collect(),
        );
    }
    if !motions.is_empty() {
        let mut groups = serde_json::Map::new();
        let mut registered = std::collections::HashSet::new();
        for group in doc.motion_groups() {
            let mut entries = Vec::new();
            for entry in &group.entries {
                let path = format!("motions/{}.motion3.json", entry.clip_id);
                let mut value = serde_json::Map::new();
                value.insert("File".into(), Value::String(path));
                if let Some(sound) = &entry.sound {
                    value.insert("Sound".into(), Value::String(sound.clone()));
                }
                if let Some(fade) = entry.fade_in {
                    value.insert("FadeInTime".into(), serde_json::json!(fade));
                }
                if let Some(fade) = entry.fade_out {
                    value.insert("FadeOutTime".into(), serde_json::json!(fade));
                }
                for (key, extension) in &entry.extensions {
                    if value.contains_key(key)
                        || matches!(
                            key.as_str(),
                            "File" | "FadeInTime" | "FadeOutTime" | "Sound"
                        )
                    {
                        return Err(Status::error(
                            "EXTENSION_KEY_COLLISION",
                            format!("motion registration {key}"),
                        ));
                    }
                    value.insert(key.clone(), extension.clone());
                }
                entries.push(Value::Object(value));
                registered.insert(entry.clip_id.as_str());
            }
            groups.insert(group.name.clone(), Value::Array(entries));
        }
        let remaining: Vec<_> = doc
            .motion_order()
            .iter()
            .filter(|id| !registered.contains(id.as_str()))
            .map(|id| serde_json::json!({"File": format!("motions/{id}.motion3.json")}))
            .collect();
        if !remaining.is_empty() {
            groups
                .entry("Default".to_string())
                .or_insert_with(|| Value::Array(Vec::new()))
                .as_array_mut()
                .expect("Default group is an array")
                .extend(remaining);
        }
        model3["FileReferences"]["Motions"] = Value::Object(groups);
    }
    for (key, value) in crate::model3::export_settings(doc)? {
        model3[key] = value;
    }
    if let Some(path) = &doc.model3_settings().user_data {
        model3["FileReferences"]["UserData"] = Value::String(path.clone());
    }
    let mut referenced = std::collections::HashSet::new();
    if let Some(path) = &doc.model3_settings().user_data {
        referenced.insert(path.as_str());
    }
    for group in doc.motion_groups() {
        for entry in &group.entries {
            if let Some(path) = &entry.sound {
                referenced.insert(path.as_str());
            }
        }
    }
    let managed: std::collections::HashMap<_, _> = doc
        .package_attachments()
        .iter()
        .map(|attachment| (attachment.path.as_str(), attachment))
        .collect();
    if let Some(path) = managed.keys().find(|path| !referenced.contains(**path)) {
        return Err(Status::error("UNREFERENCED_ATTACHMENT", *path));
    }
    let mut reserved = std::collections::HashSet::from([
        "model.moc3".to_string(),
        "model.model3.json".to_string(),
        "model.cdi3.json".to_string(),
        "model.pose3.json".to_string(),
        "model.physics3.json".to_string(),
        "export-report.json".to_string(),
    ]);
    reserved.extend(
        artifact
            .textures
            .iter()
            .map(|item| item.package_path.clone()),
    );
    reserved.extend(expressions.iter().map(|(path, _, _)| path.clone()));
    reserved.extend(motions.iter().map(|(path, _, _)| path.clone()));
    validate_package_paths(
        reserved
            .iter()
            .map(String::as_str)
            .chain(referenced.iter().copied()),
    )?;
    for path in &referenced {
        if !kasane_core::document::valid_attachment_path(path) || reserved.contains(*path) {
            return Err(Status::error("ATTACHMENT_PATH_COLLISION", *path));
        }
        let attachment = managed
            .get(path)
            .ok_or_else(|| Status::error("MISSING_PACKAGE_ATTACHMENT", *path))?;
        if doc.model3_settings().user_data.as_deref() == Some(*path) {
            crate::model3::validate_user_data(doc, &attachment.bytes)?;
        }
    }
    let model3_json = serde_json::to_string_pretty(&model3)
        .map_err(|error| Status::error("INVALID_MODEL3_JSON", error.to_string()))?;
    // Structural validity is an invariant of publication, not something a
    // caller-supplied validator may accidentally omit or mislabel.
    kasane_moc3::inspect_moc3_safety(&artifact.bytes)?;
    // Verify even unused project assets, as in the existing DocumentSession contract.
    // The bytes checked here are the exact bytes written below; never reopen after validation.
    let mut verified = std::collections::HashMap::new();
    for id in doc.asset_order() {
        let data = read_project_asset(asset_root, doc.get_asset(id).unwrap())?;
        verified.insert(id.as_str(), data.bytes);
    }
    let moc_version = artifact.bytes[4];
    let mut managed_paths = referenced.iter().copied().collect::<Vec<_>>();
    managed_paths.sort();
    let report = serde_json::json!({
        "status": "published",
        "encoding": "passed",
        "structural_validation": "passed",
        "runtime_validation": RuntimeValidation::NotPerformed.report_value(),
        "framework_model3_loader_validation": "not_performed",
        "framework_animation_validation": "not_performed",
        "framework_render_validation": "not_performed",
        "cdi_validation": "passed",
        "expression_validation": "passed",
        "motion_validation": "passed",
        "pose_validation": if pose.is_some() { "passed" } else { "not_present" },
        "physics_validation": if physics.is_some() { "passed" } else { "not_present" },
        "managed_attachments": managed_paths,
        "expressions": expressions.iter().map(|(path, name, _)| serde_json::json!({"path": path, "name": name})).collect::<Vec<_>>(),
        "motions": motions.iter().map(|(path, name, _)| serde_json::json!({"path": path, "name": name})).collect::<Vec<_>>(),
        "moc_version": moc_version,
        "source_revision": doc.revision(),
        "package_reference_validation": "passed",
        "textures": artifact.textures.iter().map(|slot| serde_json::json!({
            "asset_id": slot.asset_id, "source": slot.source,
            "path": slot.package_path, "width": slot.width, "height": slot.height,
        })).collect::<Vec<_>>(),
    });
    let mut files = BTreeMap::new();
    let mut add = |path: String, kind, bytes: Vec<u8>| -> Result<(), Status> {
        let sha256 = crate::content_sha256(&bytes);
        if files
            .insert(
                path.clone(),
                ExportFile {
                    kind,
                    bytes,
                    sha256,
                },
            )
            .is_some()
        {
            return Err(Status::error("ATTACHMENT_PATH_COLLISION", path));
        }
        Ok(())
    };
    add("model.moc3".into(), ExportFileKind::Moc, artifact.bytes)?;
    add(
        "model.model3.json".into(),
        ExportFileKind::Model,
        model3_json.into_bytes(),
    )?;
    add(
        "model.cdi3.json".into(),
        ExportFileKind::DisplayInfo,
        cdi_json.into_bytes(),
    )?;
    if let Some(content) = pose {
        add(
            "model.pose3.json".into(),
            ExportFileKind::Pose,
            content.into_bytes(),
        )?;
    }
    if let Some(content) = physics {
        add(
            "model.physics3.json".into(),
            ExportFileKind::Physics,
            content.into_bytes(),
        )?;
    }
    for (path, _, content) in expressions {
        add(path, ExportFileKind::Expression, content.into_bytes())?;
    }
    for (path, _, content) in motions {
        add(path, ExportFileKind::Motion, content.into_bytes())?;
    }
    for path in referenced {
        add(
            path.into(),
            ExportFileKind::Attachment,
            managed[path].bytes.clone(),
        )?;
    }
    for slot in artifact.textures {
        add(
            slot.package_path,
            ExportFileKind::Texture,
            verified
                .remove(slot.asset_id.as_str())
                .expect("verified texture"),
        )?;
    }
    let references = model_references(&model3)?;
    for reference in &references {
        if !files.contains_key(&reference.to) {
            return Err(Status::error("MISSING_PACKAGE_ATTACHMENT", &reference.to));
        }
    }
    let mut source_paths = Vec::new();
    if !asset_root.as_os_str().is_empty() {
        source_paths.push(io::local_path(asset_root)?);
    }
    for id in doc.asset_order() {
        source_paths.push(asset_path(asset_root, doc.get_asset(id).expect("asset"))?);
    }
    Ok(ExportPlan {
        files,
        report,
        source_paths,
        references,
        revision: doc.revision(),
    })
}

fn model_references(model: &Value) -> Result<Vec<PackageReference>, Status> {
    let refs = &model["FileReferences"];
    let mut targets = Vec::new();
    for key in ["Moc", "DisplayInfo", "Physics", "Pose", "UserData"] {
        if let Some(value) = refs.get(key) {
            targets.push(value);
        }
    }
    if let Some(textures) = refs.get("Textures").and_then(Value::as_array) {
        targets.extend(textures);
    }
    if let Some(expressions) = refs.get("Expressions").and_then(Value::as_array) {
        for item in expressions {
            targets.push(&item["File"]);
        }
    }
    if let Some(groups) = refs.get("Motions").and_then(Value::as_object) {
        for entries in groups.values().filter_map(Value::as_array) {
            for item in entries {
                targets.push(&item["File"]);
                if let Some(sound) = item.get("Sound") {
                    targets.push(sound);
                }
            }
        }
    }
    targets
        .into_iter()
        .map(|target| {
            target
                .as_str()
                .map(|to| PackageReference {
                    from: "model.model3.json".into(),
                    to: to.into(),
                })
                .ok_or_else(|| {
                    Status::error("INVALID_MODEL3_JSON", "File reference must be a string")
                })
        })
        .collect()
}
