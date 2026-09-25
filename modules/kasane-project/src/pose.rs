//! pose3 runtime IDs to persistent Part UUIDs and strict export.
use std::collections::{BTreeMap, HashMap};

use kasane_core::document::{PoseAsset, PoseEntry, PosePartRef};
use kasane_core::Document;
use kasane_live2d::pose3::{decode_pose3, encode_pose3, Pose3, Pose3Part};
use sha2::{Digest, Sha256};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PoseDiagnostic {
    pub code: String,
    pub path: String,
    pub message: String,
}

#[derive(Debug)]
pub struct PoseImport {
    pub candidate: Document,
    pub diagnostics: Vec<PoseDiagnostic>,
}

#[derive(Debug, Error)]
#[error("Pose {code} at {path}: {message}")]
pub struct PoseProjectError {
    pub code: String,
    pub path: String,
    pub message: String,
}

fn error(
    code: impl Into<String>,
    path: impl Into<String>,
    message: impl Into<String>,
) -> PoseProjectError {
    PoseProjectError {
        code: code.into(),
        path: path.into(),
        message: message.into(),
    }
}

fn namespace(document: &Document, pose: Option<&PoseAsset>) -> BTreeMap<String, String> {
    let mut ids = BTreeMap::new();
    for id in document.part_order() {
        ids.insert(
            id.clone(),
            document
                .get_part(id)
                .expect("ordered part")
                .runtime_id
                .clone(),
        );
    }
    if let Some(pose) = pose {
        for entry in pose.groups.iter().flatten() {
            for target in std::iter::once(&entry.part).chain(entry.links.iter().flatten()) {
                if let PosePartRef::Unresolved { runtime_id } = target {
                    ids.insert(format!("unresolved:{runtime_id}"), runtime_id.clone());
                }
            }
        }
    }
    ids
}

fn content_hash(pose: &PoseAsset) -> String {
    let bytes = serde_json::to_vec(&(
        &pose.file_type,
        pose.fade_in,
        &pose.groups,
        &pose.extensions,
    ))
    .expect("validated pose content serializes");
    format!("{:x}", Sha256::digest(bytes))
}

pub fn import_pose3(
    document: &Document,
    id: &str,
    text: &str,
) -> Result<PoseImport, PoseProjectError> {
    if !document.initialized() {
        return Err(error(
            "NOT_INITIALIZED",
            "$",
            "initialize the document first",
        ));
    }
    let wire =
        decode_pose3(text).map_err(|failure| error(failure.code, failure.path, failure.message))?;
    let parts: HashMap<_, _> = document
        .part_order()
        .iter()
        .map(|id| {
            (
                document
                    .get_part(id)
                    .expect("ordered part")
                    .runtime_id
                    .as_str(),
                id.as_str(),
            )
        })
        .collect();
    let mut diagnostics = Vec::new();
    let mut target = |runtime_id: String, path: String| -> PosePartRef {
        if let Some(id) = parts.get(runtime_id.as_str()) {
            PosePartRef::Resolved {
                part_id: (*id).into(),
            }
        } else {
            diagnostics.push(PoseDiagnostic {
                code: "UNRESOLVED_PART".into(),
                path,
                message: format!("{runtime_id} is absent from the model"),
            });
            PosePartRef::Unresolved { runtime_id }
        }
    };
    let groups = wire
        .groups
        .into_iter()
        .enumerate()
        .map(|(group_index, group)| {
            group
                .into_iter()
                .enumerate()
                .map(|(index, part)| {
                    let path = format!("$.Groups[{group_index}][{index}]");
                    PoseEntry {
                        part: target(part.id, format!("{path}.Id")),
                        links: part.links.map(|links| {
                            links
                                .into_iter()
                                .enumerate()
                                .map(|(link_index, id)| {
                                    target(id, format!("{path}.Link[{link_index}]"))
                                })
                                .collect()
                        }),
                        extensions: part.extensions,
                    }
                })
                .collect()
        })
        .collect();
    let mut asset = PoseAsset {
        id: id.into(),
        file_type: wire.file_type,
        fade_in: wire.fade_in,
        groups,
        extensions: wire.extensions,
        opaque_source_ids: None,
        opaque_source_content_hash: None,
    };
    if asset.has_extensions() {
        asset.opaque_source_ids = Some(namespace(document, Some(&asset)));
        asset.opaque_source_content_hash = Some(content_hash(&asset));
    }
    let mut candidate = document.fork_candidate();
    let result = candidate.set_pose(asset);
    if !result.status.is_ok() {
        return Err(error(result.status.code, "$", result.status.message));
    }
    Ok(PoseImport {
        candidate,
        diagnostics,
    })
}

pub fn export_pose3(document: &Document) -> Result<Option<String>, PoseProjectError> {
    let Some(pose) = document.pose() else {
        return Ok(None);
    };
    if pose.has_extensions() {
        if pose.opaque_source_ids.as_ref() != Some(&namespace(document, Some(pose))) {
            return Err(error(
                "OPAQUE_NAMESPACE_CHANGED",
                "$",
                "Part runtime IDs changed since import",
            ));
        }
        if pose.opaque_source_content_hash.as_deref() != Some(content_hash(pose).as_str()) {
            return Err(error(
                "OPAQUE_CONTENT_CHANGED",
                "$",
                "Pose content changed since import; reimport to establish an opaque-field baseline",
            ));
        }
    }
    let runtime_id = |target: &PosePartRef, path: String| -> Result<String, PoseProjectError> {
        match target {
            PosePartRef::Resolved { part_id } => document
                .get_part(part_id)
                .map(|part| part.runtime_id.clone())
                .ok_or_else(|| error("MISSING_PART", path, part_id)),
            PosePartRef::Unresolved { runtime_id } => {
                Err(error("UNRESOLVED_PART", path, runtime_id))
            }
        }
    };
    let groups = pose
        .groups
        .iter()
        .enumerate()
        .map(|(group_index, group)| {
            group
                .iter()
                .enumerate()
                .map(|(index, entry)| {
                    let path = format!("$.Groups[{group_index}][{index}]");
                    Ok(Pose3Part {
                        id: runtime_id(&entry.part, format!("{path}.Id"))?,
                        links: entry
                            .links
                            .as_ref()
                            .map(|links| {
                                links
                                    .iter()
                                    .enumerate()
                                    .map(|(link_index, target)| {
                                        runtime_id(target, format!("{path}.Link[{link_index}]"))
                                    })
                                    .collect::<Result<Vec<_>, _>>()
                            })
                            .transpose()?,
                        extensions: entry.extensions.clone(),
                    })
                })
                .collect::<Result<Vec<_>, PoseProjectError>>()
        })
        .collect::<Result<Vec<_>, _>>()?;
    let wire = Pose3 {
        file_type: pose.file_type.clone(),
        fade_in: pose.fade_in,
        groups,
        extensions: pose.extensions.clone(),
    };
    encode_pose3(&wire)
        .map(Some)
        .map_err(|failure| error(failure.code, failure.path, failure.message))
}
