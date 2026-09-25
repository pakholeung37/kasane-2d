//! Boundary conversion between model3 runtime IDs and typed document references.
use kasane_core::document::{Model3Settings, ModelHitArea, ModelParameterGroup, ModelTargetRef};
use kasane_core::{types::Status, Document};
use serde::Deserialize;
use serde_json::{Map, Value};
use std::collections::{BTreeMap, HashSet};

pub fn runtime_namespace(document: &Document) -> BTreeMap<String, String> {
    let mut ids = BTreeMap::new();
    for id in document.parameter_order() {
        ids.insert(
            id.clone(),
            document
                .get_parameter(id)
                .expect("parameter")
                .runtime_id
                .clone(),
        );
    }
    for id in document.part_order() {
        ids.insert(
            id.clone(),
            document.get_part(id).expect("part").runtime_id.clone(),
        );
    }
    for id in document.mesh_order() {
        ids.insert(
            id.clone(),
            document.get_mesh(id).expect("mesh").runtime_id.clone(),
        );
    }
    ids
}

#[derive(Deserialize)]
struct WireGroup {
    #[serde(rename = "Target")]
    target: String,
    #[serde(rename = "Name")]
    name: String,
    #[serde(rename = "Ids")]
    ids: Vec<String>,
    #[serde(flatten)]
    extensions: BTreeMap<String, Value>,
}
#[derive(Deserialize)]
struct WireHitArea {
    #[serde(rename = "Name")]
    name: String,
    #[serde(rename = "Id")]
    id: String,
    #[serde(flatten)]
    extensions: BTreeMap<String, Value>,
}

pub fn import_settings(document: &Document, model3: &Value) -> Result<Model3Settings, Status> {
    import_with_namespace(document, model3, &runtime_namespace(document))
}

fn import_with_namespace(
    document: &Document,
    model3: &Value,
    namespace: &BTreeMap<String, String>,
) -> Result<Model3Settings, Status> {
    let resolve = |runtime_id: String, parameter: bool| {
        let order = if parameter {
            document.parameter_order()
        } else {
            document.mesh_order()
        };
        order
            .iter()
            .find(|id| namespace.get(*id) == Some(&runtime_id))
            .map(|id| ModelTargetRef::Resolved {
                object_id: id.clone(),
            })
            .unwrap_or(ModelTargetRef::Unresolved { runtime_id })
    };
    let groups = model3
        .get("Groups")
        .map(|value| -> Result<_, Status> {
            let groups: Vec<WireGroup> = serde_json::from_value(value.clone())
                .map_err(|e| Status::error("INVALID_MODEL3_GROUPS", e.to_string()))?;
            groups
                .into_iter()
                .map(|group| {
                    if group.target != "Parameter" {
                        return Err(Status::error("INVALID_MODEL3_GROUPS", group.target));
                    }
                    Ok(ModelParameterGroup {
                        name: group.name,
                        parameters: group.ids.into_iter().map(|id| resolve(id, true)).collect(),
                        extensions: group.extensions,
                    })
                })
                .collect::<Result<Vec<_>, Status>>()
        })
        .transpose()?;
    let hit_areas = model3
        .get("HitAreas")
        .map(|value| -> Result<_, Status> {
            let areas: Vec<WireHitArea> = serde_json::from_value(value.clone())
                .map_err(|e| Status::error("INVALID_MODEL3_HIT_AREAS", e.to_string()))?;
            Ok(areas
                .into_iter()
                .map(|area| ModelHitArea {
                    name: area.name,
                    mesh: resolve(area.id, false),
                    extensions: area.extensions,
                })
                .collect())
        })
        .transpose()?;
    let layout = model3
        .get("Layout")
        .map(|value| {
            serde_json::from_value(value.clone())
                .map_err(|e| Status::error("INVALID_MODEL3_LAYOUT", e.to_string()))
        })
        .transpose()?;
    let user_data = model3
        .get("FileReferences")
        .and_then(|refs| refs.get("UserData"))
        .map(|value| {
            value
                .as_str()
                .map(str::to_string)
                .ok_or_else(|| Status::error("INVALID_MODEL3_JSON", "UserData must be a string"))
        })
        .transpose()?;
    let mut settings = Model3Settings {
        groups,
        hit_areas,
        layout,
        user_data,
        extensions: model3
            .as_object()
            .into_iter()
            .flat_map(|object| object.iter())
            .filter(|(key, _)| {
                !matches!(
                    key.as_str(),
                    "Version" | "FileReferences" | "Groups" | "Layout" | "HitAreas"
                )
            })
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect(),
        ..Default::default()
    };
    if let Some(references) = model3.get("FileReferences").and_then(Value::as_object) {
        let extensions: Map<String, Value> = references
            .iter()
            .filter(|(key, _)| {
                !matches!(
                    key.as_str(),
                    "Moc"
                        | "Textures"
                        | "Physics"
                        | "Pose"
                        | "DisplayInfo"
                        | "Expressions"
                        | "Motions"
                        | "UserData"
                )
            })
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect();
        if !extensions.is_empty() {
            settings
                .extensions
                .insert("FileReferences".into(), Value::Object(extensions));
        }
    }
    if settings.has_extensions() || settings.user_data.is_some() {
        settings.source_runtime_ids = Some(namespace.clone());
    }
    if settings.has_extensions() {
        settings.source_content = Some(serde_json::to_string(model3).expect("JSON content"));
    }
    Ok(settings)
}

/// v5 stored raw JSON and source namespace; convert only at the codec boundary.
pub(crate) fn migrate_v5_settings(
    document: &Document,
    value: Value,
) -> Result<Model3Settings, Status> {
    #[derive(Deserialize, Default)]
    #[serde(deny_unknown_fields)]
    struct Legacy {
        groups: Option<Value>,
        layout: Option<Value>,
        hit_areas: Option<Value>,
        user_data: Option<String>,
        #[serde(default)]
        extensions: BTreeMap<String, Value>,
        source_runtime_ids: Option<BTreeMap<String, String>>,
        source_content: Option<String>,
    }
    let old: Legacy = serde_json::from_value(value)
        .map_err(|e| Status::error("INVALID_PROJECT", e.to_string()))?;
    let mut wire = Map::new();
    for (key, value) in [
        ("Groups", old.groups),
        ("Layout", old.layout),
        ("HitAreas", old.hit_areas),
    ] {
        if let Some(value) = value {
            wire.insert(key.into(), value);
        }
    }
    if let Some(path) = old.user_data {
        wire.insert(
            "FileReferences".into(),
            serde_json::json!({"UserData": path}),
        );
    }
    let namespace = old
        .source_runtime_ids
        .unwrap_or_else(|| runtime_namespace(document));
    let mut settings = import_with_namespace(document, &Value::Object(wire), &namespace)?;
    settings.extensions = old.extensions;
    settings.source_content = old.source_content;
    if settings.has_extensions() || settings.user_data.is_some() {
        settings.source_runtime_ids = Some(namespace);
    }
    Ok(settings)
}

pub fn export_settings(document: &Document) -> Result<Map<String, Value>, Status> {
    let settings = document.model3_settings();
    // Known fields are remapped by identity. Only opaque payloads need a
    // namespace-wide guard because their embedded IDs cannot be rewritten.
    if (settings.has_extensions() || settings.user_data.is_some())
        && settings
            .source_runtime_ids
            .as_ref()
            .is_some_and(|source| source != &runtime_namespace(document))
    {
        return Err(Status::error(
            "MODEL3_NAMESPACE_CHANGED",
            "Opaque model3/UserData namespace changed",
        ));
    }
    if settings.has_extensions() {
        return Err(Status::error(
            "UNSUPPORTED_MODEL3_FIELD",
            "Unmodeled model3 fields require explicit resolution",
        ));
    }
    let runtime_id = |target: &ModelTargetRef, parameter: bool| -> Result<String, Status> {
        if let ModelTargetRef::Resolved { object_id } = target {
            let id = if parameter {
                document.get_parameter(object_id).map(|p| &p.runtime_id)
            } else {
                document.get_mesh(object_id).map(|m| &m.runtime_id)
            };
            if let Some(id) = id {
                return Ok(id.clone());
            }
        }
        Err(Status::error(
            "UNRESOLVED_MODEL3_TARGET",
            format!("{target:?}"),
        ))
    };
    let mut output = Map::new();
    if let Some(groups) = &settings.groups {
        let wire = groups
            .iter()
            .map(|group| {
                let ids = group
                    .parameters
                    .iter()
                    .map(|target| runtime_id(target, true))
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(serde_json::json!({"Target": "Parameter", "Name": group.name, "Ids": ids}))
            })
            .collect::<Result<Vec<Value>, Status>>()?;
        output.insert("Groups".into(), Value::Array(wire));
    }
    if let Some(layout) = &settings.layout {
        output.insert(
            "Layout".into(),
            serde_json::to_value(layout)
                .map_err(|e| Status::error("INVALID_MODEL3_LAYOUT", e.to_string()))?,
        );
    }
    if let Some(areas) = &settings.hit_areas {
        let wire = areas
            .iter()
            .map(|area| {
                Ok(serde_json::json!({"Name": area.name, "Id": runtime_id(&area.mesh, false)?}))
            })
            .collect::<Result<Vec<Value>, Status>>()?;
        output.insert("HitAreas".into(), Value::Array(wire));
    }
    Ok(output)
}

pub fn validate_user_data(document: &Document, bytes: &[u8]) -> Result<(), Status> {
    let value: Value = serde_json::from_slice(bytes)
        .map_err(|error| Status::error("INVALID_USERDATA_JSON", error.to_string()))?;
    let items = value
        .get("UserData")
        .and_then(Value::as_array)
        .ok_or_else(|| Status::error("INVALID_USERDATA_JSON", "UserData must be an array"))?;
    let drawables: HashSet<_> = document
        .mesh_order()
        .iter()
        .map(|id| document.get_mesh(id).expect("mesh").runtime_id.as_str())
        .collect();
    for (index, item) in items.iter().enumerate() {
        if item.get("Target").and_then(Value::as_str) != Some("ArtMesh")
            || !item
                .get("Id")
                .and_then(Value::as_str)
                .is_some_and(|id| drawables.contains(id))
        {
            return Err(Status::error(
                "UNRESOLVED_USERDATA_TARGET",
                format!("UserData[{index}]"),
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_file_references_are_preserved_and_block_strict_export() {
        let mut document = Document::new();
        assert!(document
            .initialize(
                "00000000-0000-4000-8000-000000000001",
                kasane_core::Canvas::new(100.0, 100.0, kasane_core::Vec2::default(), 10.0),
            )
            .is_ok());
        let source = serde_json::json!({"FileReferences": {
            "Moc": "model.moc3", "VendorAsset": {"File": "vendor.bin"}
        }});
        let settings = import_settings(&document, &source).unwrap();
        assert_eq!(
            settings.extensions["FileReferences"],
            serde_json::json!({
                "VendorAsset": {"File": "vendor.bin"}
            })
        );
        assert!(document.set_model3_settings(settings).status.is_ok());
        assert_eq!(
            export_settings(&document).unwrap_err().code,
            "UNSUPPORTED_MODEL3_FIELD"
        );
    }
}
