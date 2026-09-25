//! physics3 import and export with stable parameter bindings.
use std::collections::BTreeMap;

use kasane_core::document::PhysicsAsset;
use kasane_core::Document;
use kasane_live2d::physics3::{decode_physics3, encode_physics3};
use sha2::{Digest, Sha256};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PhysicsDiagnostic {
    pub code: String,
    pub path: String,
    pub message: String,
}

pub struct PhysicsImport {
    pub candidate: Document,
    pub diagnostics: Vec<PhysicsDiagnostic>,
}

#[derive(Debug, Error)]
#[error("Physics {code} at {path}: {message}")]
pub struct PhysicsProjectError {
    pub code: String,
    pub path: String,
    pub message: String,
}

fn error(
    code: impl Into<String>,
    path: impl Into<String>,
    message: impl Into<String>,
) -> PhysicsProjectError {
    PhysicsProjectError {
        code: code.into(),
        path: path.into(),
        message: message.into(),
    }
}

fn namespace(document: &Document, asset: &PhysicsAsset) -> BTreeMap<String, String> {
    asset
        .parameter_bindings
        .iter()
        .filter_map(|(runtime, id)| {
            document
                .get_parameter(id)
                .map(|parameter| (runtime.clone(), parameter.runtime_id.clone()))
        })
        .collect()
}

fn content_hash(asset: &PhysicsAsset) -> String {
    let bytes = serde_json::to_vec(&(&asset.data, &asset.parameter_bindings))
        .expect("physics content serializes");
    format!("{:x}", Sha256::digest(bytes))
}

pub fn import_physics3(
    document: &Document,
    id: &str,
    text: &str,
) -> Result<PhysicsImport, PhysicsProjectError> {
    if !document.initialized() {
        return Err(error(
            "NOT_INITIALIZED",
            "$",
            "initialize the document first",
        ));
    }
    let data = decode_physics3(text)
        .map_err(|failure| error(failure.code, failure.path, failure.message))?;
    let mut bindings = BTreeMap::new();
    let mut diagnostics = Vec::new();
    for (rig_index, rig) in data.settings.iter().enumerate() {
        for (index, input) in rig.inputs.iter().enumerate() {
            bind(
                document,
                &mut bindings,
                &mut diagnostics,
                &input.source.id,
                format!("$.PhysicsSettings[{rig_index}].Input[{index}].Source.Id"),
            );
        }
        for (index, output) in rig.outputs.iter().enumerate() {
            bind(
                document,
                &mut bindings,
                &mut diagnostics,
                &output.destination.id,
                format!("$.PhysicsSettings[{rig_index}].Output[{index}].Destination.Id"),
            );
        }
    }
    let mut asset = PhysicsAsset {
        id: id.into(),
        data,
        parameter_bindings: bindings,
        opaque_source_ids: None,
        opaque_source_content_hash: None,
    };
    // Unknown extensions cannot be remapped safely when model IDs change.
    if has_extensions(&asset) {
        asset.opaque_source_ids = Some(namespace(document, &asset));
        asset.opaque_source_content_hash = Some(content_hash(&asset));
    }
    let mut candidate = document.fork_candidate();
    let result = candidate.set_physics(asset);
    if !result.status.is_ok() {
        return Err(error(result.status.code, "$", result.status.message));
    }
    Ok(PhysicsImport {
        candidate,
        diagnostics,
    })
}

fn bind(
    document: &Document,
    bindings: &mut BTreeMap<String, String>,
    diagnostics: &mut Vec<PhysicsDiagnostic>,
    runtime_id: &str,
    path: String,
) {
    if bindings.contains_key(runtime_id) {
        return;
    }
    if let Some(id) = document.parameter_order().iter().find(|id| {
        document
            .get_parameter(id)
            .is_some_and(|p| p.runtime_id == runtime_id)
    }) {
        bindings.insert(runtime_id.into(), id.clone());
    } else {
        diagnostics.push(PhysicsDiagnostic {
            code: "UNRESOLVED_PARAMETER".into(),
            path,
            message: format!("{runtime_id} is absent from the model"),
        });
    }
}

fn has_extensions(asset: &PhysicsAsset) -> bool {
    let data = &asset.data;
    !data.extensions.is_empty()
        || !data.meta.extensions.is_empty()
        || !data.meta.forces.extensions.is_empty()
        || data
            .meta
            .dictionary
            .iter()
            .any(|item| !item.extensions.is_empty())
        || data.settings.iter().any(|rig| {
            !rig.extensions.is_empty()
                || !rig.normalization.extensions.is_empty()
                || rig
                    .inputs
                    .iter()
                    .any(|item| !item.extensions.is_empty() || !item.source.extensions.is_empty())
                || rig.outputs.iter().any(|item| {
                    !item.extensions.is_empty() || !item.destination.extensions.is_empty()
                })
                || rig.vertices.iter().any(|item| !item.extensions.is_empty())
        })
}

pub fn export_physics3(document: &Document) -> Result<Option<String>, PhysicsProjectError> {
    let Some(asset) = document.physics() else {
        return Ok(None);
    };
    if has_extensions(asset) {
        if asset.opaque_source_ids.as_ref() != Some(&namespace(document, asset)) {
            return Err(error(
                "OPAQUE_NAMESPACE_CHANGED",
                "$",
                "Parameter runtime IDs changed since import",
            ));
        }
        if asset.opaque_source_content_hash.as_deref() != Some(content_hash(asset).as_str()) {
            return Err(error(
                "OPAQUE_CONTENT_CHANGED",
                "$",
                "Physics content changed since import",
            ));
        }
    }
    let mut data = asset.data.clone();
    for (rig_index, rig) in data.settings.iter_mut().enumerate() {
        for (index, input) in rig.inputs.iter_mut().enumerate() {
            input.source.id = mapped_id(
                document,
                asset,
                &input.source.id,
                format!("$.PhysicsSettings[{rig_index}].Input[{index}].Source.Id"),
            )?;
        }
        for (index, output) in rig.outputs.iter_mut().enumerate() {
            output.destination.id = mapped_id(
                document,
                asset,
                &output.destination.id,
                format!("$.PhysicsSettings[{rig_index}].Output[{index}].Destination.Id"),
            )?;
        }
    }
    encode_physics3(&data)
        .map(Some)
        .map_err(|failure| error(failure.code, failure.path, failure.message))
}

fn mapped_id(
    document: &Document,
    asset: &PhysicsAsset,
    runtime_id: &str,
    path: String,
) -> Result<String, PhysicsProjectError> {
    let id = asset
        .parameter_bindings
        .get(runtime_id)
        .ok_or_else(|| error("UNRESOLVED_PARAMETER", &path, runtime_id))?;
    document
        .get_parameter(id)
        .map(|parameter| parameter.runtime_id.clone())
        .ok_or_else(|| error("MISSING_PARAMETER", path, id))
}
