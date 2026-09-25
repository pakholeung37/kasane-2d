//! Expression wire to persistent document UUIDs, and strict export.
use std::collections::{BTreeMap, HashMap};

use kasane_core::document::{ExpressionAsset, ExpressionBlend, ExpressionEntry, ExpressionTarget};
use kasane_core::Document;
use kasane_live2d::exp3::{
    decode_exp3, encode_exp3, Expression3, Expression3Parameter, ExpressionError,
};
use sha2::{Digest, Sha256};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExpressionDiagnostic {
    pub code: String,
    pub path: String,
    pub message: String,
}

#[derive(Debug)]
pub struct ExpressionImport {
    pub candidate: Document,
    pub diagnostics: Vec<ExpressionDiagnostic>,
}

#[derive(Debug, Error)]
#[error("Expression {code} at {path}: {message}")]
pub struct ExpressionProjectError {
    pub code: String,
    pub path: String,
    pub message: String,
}

fn error(
    code: impl Into<String>,
    path: impl Into<String>,
    message: impl Into<String>,
) -> ExpressionProjectError {
    ExpressionProjectError {
        code: code.into(),
        path: path.into(),
        message: message.into(),
    }
}

fn wire_error(value: ExpressionError) -> ExpressionProjectError {
    error(value.code, value.path, value.message)
}

fn parameter_ids(document: &Document, asset: Option<&ExpressionAsset>) -> BTreeMap<String, String> {
    let mut ids = BTreeMap::new();
    for id in document.parameter_order() {
        ids.insert(
            id.clone(),
            document
                .get_parameter(id)
                .expect("ordered parameter exists")
                .runtime_id
                .clone(),
        );
    }
    if let Some(asset) = asset {
        for entry in &asset.entries {
            if let ExpressionTarget::Unresolved { runtime_id } = &entry.target {
                ids.insert(format!("unresolved:{runtime_id}"), runtime_id.clone());
            }
        }
    }
    ids
}

fn content_hash(asset: &ExpressionAsset) -> String {
    let payload = serde_json::to_vec(&(
        &asset.name,
        &asset.file_type,
        asset.fade_in,
        asset.fade_out,
        &asset.entries,
        &asset.extensions,
    ))
    .expect("validated expression content serializes");
    format!("{:x}", Sha256::digest(payload))
}

/// Build an isolated candidate. Absent parameter targets remain repairable in
/// the project and are returned as diagnostics; strict export rejects them.
pub fn import_expression3(
    document: &Document,
    id: &str,
    name: &str,
    text: &str,
) -> Result<ExpressionImport, ExpressionProjectError> {
    if !document.initialized() {
        return Err(error(
            "NOT_INITIALIZED",
            "$",
            "initialize the document first",
        ));
    }
    let wire = decode_exp3(text).map_err(wire_error)?;
    let parameters: HashMap<_, _> = document
        .parameter_order()
        .iter()
        .map(|id| {
            let parameter = document
                .get_parameter(id)
                .expect("ordered parameter exists");
            (parameter.runtime_id.as_str(), id.as_str())
        })
        .collect();
    let mut diagnostics = Vec::new();
    let entries = wire
        .parameters
        .unwrap_or_default()
        .into_iter()
        .enumerate()
        .map(|(index, entry)| {
            let target = if let Some(parameter_id) = parameters.get(entry.id.as_str()) {
                ExpressionTarget::Resolved {
                    parameter_id: (*parameter_id).into(),
                }
            } else {
                diagnostics.push(ExpressionDiagnostic {
                    code: "UNRESOLVED_PARAMETER".into(),
                    path: format!("$.Parameters[{index}].Id"),
                    message: format!("{} is absent from the model", entry.id),
                });
                ExpressionTarget::Unresolved {
                    runtime_id: entry.id,
                }
            };
            ExpressionEntry {
                target,
                value: entry.value,
                blend: entry.blend.map(|blend| match blend {
                    kasane_live2d::exp3::ExpressionBlend::Add => ExpressionBlend::Add,
                    kasane_live2d::exp3::ExpressionBlend::Multiply => ExpressionBlend::Multiply,
                    kasane_live2d::exp3::ExpressionBlend::Overwrite => ExpressionBlend::Overwrite,
                }),
                extensions: entry.extensions,
            }
        })
        .collect();
    let mut asset = ExpressionAsset {
        id: id.into(),
        name: name.into(),
        file_type: wire.file_type,
        fade_in: wire.fade_in,
        fade_out: wire.fade_out,
        entries,
        extensions: wire.extensions,
        opaque_source_ids: None,
        opaque_source_content_hash: None,
    };
    if asset.has_extensions() {
        asset.opaque_source_ids = Some(parameter_ids(document, Some(&asset)));
        asset.opaque_source_content_hash = Some(content_hash(&asset));
    }
    let mut candidate = document.fork_candidate();
    let result = if candidate.get_expression(id).is_some() {
        candidate.replace_expression(asset)
    } else {
        candidate.create_expression(asset)
    };
    if !result.status.is_ok() {
        return Err(error(result.status.code, "$", result.status.message));
    }
    Ok(ExpressionImport {
        candidate,
        diagnostics,
    })
}

/// Emit a Framework-compatible exp3 file using current model runtime IDs.
pub fn export_expression3(document: &Document, id: &str) -> Result<String, ExpressionProjectError> {
    let asset = document
        .get_expression(id)
        .ok_or_else(|| error("MISSING_EXPRESSION", "$", id))?;
    if asset.has_extensions() {
        let Some(source) = &asset.opaque_source_ids else {
            return Err(error(
                "OPAQUE_PROVENANCE_MISSING",
                "$",
                "expression extensions need a source namespace",
            ));
        };
        if *source != parameter_ids(document, Some(asset)) {
            return Err(error(
                "OPAQUE_NAMESPACE_CHANGED",
                "$",
                "parameter runtime namespace changed since import",
            ));
        }
        if asset.opaque_source_content_hash.as_deref() != Some(content_hash(asset).as_str()) {
            return Err(error("OPAQUE_CONTENT_CHANGED", "$", "expression content changed since import; reimport to establish a new opaque-field baseline"));
        }
    }
    let mut parameters = Vec::with_capacity(asset.entries.len());
    for (index, entry) in asset.entries.iter().enumerate() {
        let runtime_id = match &entry.target {
            ExpressionTarget::Resolved { parameter_id } => document
                .get_parameter(parameter_id)
                .ok_or_else(|| {
                    error(
                        "MISSING_PARAMETER",
                        format!("$.Parameters[{index}].Id"),
                        parameter_id,
                    )
                })?
                .runtime_id
                .clone(),
            ExpressionTarget::Unresolved { runtime_id } => {
                return Err(error(
                    "UNRESOLVED_PARAMETER",
                    format!("$.Parameters[{index}].Id"),
                    runtime_id,
                ))
            }
        };
        parameters.push(Expression3Parameter {
            id: runtime_id,
            value: entry.value,
            blend: entry.blend.map(|blend| match blend {
                ExpressionBlend::Add => kasane_live2d::exp3::ExpressionBlend::Add,
                ExpressionBlend::Multiply => kasane_live2d::exp3::ExpressionBlend::Multiply,
                ExpressionBlend::Overwrite => kasane_live2d::exp3::ExpressionBlend::Overwrite,
            }),
            extensions: entry.extensions.clone(),
        });
    }
    encode_exp3(&Expression3 {
        file_type: asset.file_type.clone(),
        fade_in: asset.fade_in,
        fade_out: asset.fade_out,
        parameters: Some(parameters),
        extensions: asset.extensions.clone(),
    })
    .map_err(wire_error)
}
