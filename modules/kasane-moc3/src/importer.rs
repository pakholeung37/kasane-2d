use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use kasane_core::types::Status;
use kasane_core::Document;

use crate::decoder::{decode_moc3, texture_slot_uuid, DecodedMoc3, ImportReport, TextureSlotInfo};
use crate::inspector::inspect_moc3;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiagnosticSeverity {
    Warning,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportDiagnostic {
    pub severity: DiagnosticSeverity,
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone)]
pub struct ImportResult {
    pub document: Document,
    pub report: ImportReport,
    pub diagnostics: Vec<ImportDiagnostic>,
    pub textures_complete: bool,
}

fn compute_sha256(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let hash = hasher.finalize();
    let mut s = String::with_capacity(64);
    for b in hash {
        use std::fmt::Write;
        write!(&mut s, "{:02x}", b).unwrap();
    }
    s
}

pub fn import_from_model3_json(
    json_content: &str,
    base_dir: &Path,
) -> Result<ImportResult, Status> {
    let root: Value = serde_json::from_str(json_content).map_err(|e| {
        Status::error(
            "INVALID_MODEL3_JSON",
            format!("Failed to parse model3.json: {e}"),
        )
    })?;

    let file_refs = root.get("FileReferences").ok_or_else(|| {
        Status::error(
            "INVALID_MODEL3_JSON",
            "Missing 'FileReferences' object in model3.json",
        )
    })?;

    let moc_rel = file_refs
        .get("Moc")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            Status::error(
                "INVALID_MODEL3_JSON",
                "Missing 'FileReferences.Moc' in model3.json",
            )
        })?;

    let moc_path = base_dir.join(moc_rel);
    let moc_bytes = fs::read(&moc_path).map_err(|e| {
        Status::error(
            "MOC3_IO_ERROR",
            format!("Failed to read MOC3 file {}: {}", moc_path.display(), e),
        )
    })?;

    // Collect unimported attachments
    let mut unimported = Vec::new();
    if let Some(obj) = file_refs.as_object() {
        for (key, val) in obj {
            if key != "Moc" && key != "Textures" {
                if let Some(s) = val.as_str() {
                    unimported.push(format!("{key}: {s}"));
                } else if let Some(arr) = val.as_array() {
                    for item in arr {
                        if let Some(s) = item.as_str() {
                            unimported.push(format!("{key}: {s}"));
                        } else {
                            unimported.push(format!("{key}: {}", item));
                        }
                    }
                } else {
                    unimported.push(format!("{key}: {}", val));
                }
            }
        }
    }

    // Inspect MOC3 before building textures
    let inspection = inspect_moc3(&moc_bytes)?;

    // Textures
    let mut diagnostics = Vec::new();
    let mut textures = Vec::new();
    let mut textures_complete = true;

    if let Some(tex_array) = file_refs.get("Textures").and_then(|v| v.as_array()) {
        for (slot, item) in tex_array.iter().enumerate() {
            let tex_rel = item.as_str().unwrap_or_default();
            if tex_rel.is_empty() {
                diagnostics.push(ImportDiagnostic {
                    severity: DiagnosticSeverity::Error,
                    code: "EMPTY_TEXTURE_PATH".to_string(),
                    message: format!("Texture slot {slot} has empty path in model3.json"),
                });
                textures_complete = false;
                continue;
            }

            let full_path = if let Ok(canon) = base_dir.join(tex_rel).canonicalize() {
                canon
            } else if base_dir.is_relative() {
                if let Ok(cwd) = std::env::current_dir() {
                    cwd.join(base_dir).join(tex_rel)
                } else {
                    base_dir.join(tex_rel)
                }
            } else {
                base_dir.join(tex_rel)
            };
            let slot_id = texture_slot_uuid(slot);
            let source_str = full_path.to_string_lossy().to_string();
            match fs::read(&full_path) {
                Ok(bytes) => match kasane_core::image::decode_png(&bytes) {
                    Ok(image) => {
                        let sha = compute_sha256(&bytes);
                        textures.push(TextureSlotInfo {
                            slot,
                            asset_id: slot_id,
                            name: format!("Texture {slot}"),
                            source: source_str,
                            width: image.width,
                            height: image.height,
                            sha256: sha,
                        });
                    }
                    Err(s) => {
                        diagnostics.push(ImportDiagnostic {
                            severity: DiagnosticSeverity::Warning,
                            code: "CORRUPT_TEXTURE".to_string(),
                            message: format!(
                                "Texture slot {slot} ({tex_rel}) is not a valid PNG: {}",
                                s.message
                            ),
                        });
                        textures_complete = false;
                        textures.push(TextureSlotInfo {
                            slot,
                            asset_id: slot_id,
                            name: format!("Texture {slot}"),
                            source: source_str,
                            width: 1,
                            height: 1,
                            sha256: String::new(),
                        });
                    }
                },
                Err(e) => {
                    diagnostics.push(ImportDiagnostic {
                        severity: DiagnosticSeverity::Warning,
                        code: "MISSING_TEXTURE".to_string(),
                        message: format!(
                            "Texture slot {slot} missing at {}: {}",
                            full_path.display(),
                            e
                        ),
                    });
                    textures_complete = false;
                    textures.push(TextureSlotInfo {
                        slot,
                        asset_id: slot_id,
                        name: format!("Texture {slot}"),
                        source: source_str,
                        width: 1,
                        height: 1,
                        sha256: String::new(),
                    });
                }
            }
        }
    } else {
        diagnostics.push(ImportDiagnostic {
            severity: DiagnosticSeverity::Warning,
            code: "NO_TEXTURES".to_string(),
            message: "model3.json specifies no 'Textures' array".to_string(),
        });
        textures_complete = false;
    }

    let DecodedMoc3 {
        document,
        mut report,
    } = decode_moc3(&moc_bytes, &inspection, &textures)?;

    report.unimported_attachments = unimported;

    // Check if any meshes reference unmapped texture slots
    for id in document.mesh_order() {
        if let Some(m) = document.get_mesh(id) {
            if let Some(asset) = document.get_asset(&m.texture_asset_id) {
                if asset.source.starts_with("unmapped_slot_") {
                    if !diagnostics
                        .iter()
                        .any(|d| d.code == "UNMAPPED_TEXTURE_SLOT")
                    {
                        diagnostics.push(ImportDiagnostic {
                            severity: DiagnosticSeverity::Warning,
                            code: "UNMAPPED_TEXTURE_SLOT".to_string(),
                            message: format!(
                                "Mesh '{}' references texture slot not provided; path was not guessed",
                                m.name
                            ),
                        });
                        textures_complete = false;
                    }
                }
            }
        }
    }

    Ok(ImportResult {
        document,
        report,
        diagnostics,
        textures_complete,
    })
}

pub fn import_from_bare_moc3(
    moc3_bytes: &[u8],
    texture_paths: &HashMap<usize, PathBuf>,
) -> Result<ImportResult, Status> {
    let inspection = inspect_moc3(moc3_bytes)?;

    let mut diagnostics = Vec::new();
    let mut textures = Vec::new();
    let mut textures_complete = true;

    // Find the max texture slot used or declared
    // We inspect each texture path provided in the explicit map
    let mut slots: Vec<usize> = texture_paths.keys().copied().collect();
    slots.sort();

    for &slot in &slots {
        let path = &texture_paths[&slot];
        let full_path = if let Ok(canon) = path.canonicalize() {
            canon
        } else if path.is_relative() {
            if let Ok(cwd) = std::env::current_dir() {
                cwd.join(path)
            } else {
                path.to_path_buf()
            }
        } else {
            path.to_path_buf()
        };
        let slot_id = texture_slot_uuid(slot);
        let source_str = full_path.to_string_lossy().to_string();
        match fs::read(&full_path) {
            Ok(bytes) => match kasane_core::image::decode_png(&bytes) {
                Ok(image) => {
                    let sha = compute_sha256(&bytes);
                    textures.push(TextureSlotInfo {
                        slot,
                        asset_id: slot_id,
                        name: format!("Texture {slot}"),
                        source: source_str.clone(),
                        width: image.width,
                        height: image.height,
                        sha256: sha,
                    });
                }
                Err(s) => {
                    diagnostics.push(ImportDiagnostic {
                        severity: DiagnosticSeverity::Warning,
                        code: "CORRUPT_TEXTURE".to_string(),
                        message: format!(
                            "Texture slot {slot} ({}) is not a valid PNG: {}",
                            path.display(),
                            s.message
                        ),
                    });
                    textures_complete = false;
                    textures.push(TextureSlotInfo {
                        slot,
                        asset_id: slot_id,
                        name: format!("Texture {slot}"),
                        source: path.to_string_lossy().to_string(),
                        width: 1,
                        height: 1,
                        sha256: String::new(),
                    });
                }
            },
            Err(e) => {
                diagnostics.push(ImportDiagnostic {
                    severity: DiagnosticSeverity::Warning,
                    code: "MISSING_TEXTURE".to_string(),
                    message: format!(
                        "Texture slot {slot} cannot be read at {}: {}",
                        path.display(),
                        e
                    ),
                });
                textures_complete = false;
                textures.push(TextureSlotInfo {
                    slot,
                    asset_id: slot_id,
                    name: format!("Texture {slot}"),
                    source: path.to_string_lossy().to_string(),
                    width: 1,
                    height: 1,
                    sha256: String::new(),
                });
            }
        }
    }

    let DecodedMoc3 { document, report } = decode_moc3(moc3_bytes, &inspection, &textures)?;

    // Check if any meshes reference a texture slot not provided in the map
    for id in document.mesh_order() {
        if let Some(m) = document.get_mesh(id) {
            if let Some(asset) = document.get_asset(&m.texture_asset_id) {
                if asset.source.starts_with("unmapped_slot_") {
                    if !diagnostics
                        .iter()
                        .any(|d| d.code == "UNMAPPED_TEXTURE_SLOT")
                    {
                        diagnostics.push(ImportDiagnostic {
                            severity: DiagnosticSeverity::Warning,
                            code: "UNMAPPED_TEXTURE_SLOT".to_string(),
                            message: format!(
                                "Mesh '{}' references texture slot not provided in texture map; path was not guessed",
                                m.name
                            ),
                        });
                        textures_complete = false;
                    }
                }
            }
        }
    }

    Ok(ImportResult {
        document,
        report,
        diagnostics,
        textures_complete,
    })
}

pub fn import_from_model3_file(model3_path: &Path) -> Result<ImportResult, Status> {
    let content = fs::read_to_string(model3_path).map_err(|e| {
        Status::error(
            "PROJECT_IO",
            format!("Failed to read {}: {}", model3_path.display(), e),
        )
    })?;
    let base_dir = model3_path.parent().unwrap_or_else(|| Path::new("."));
    import_from_model3_json(&content, base_dir)
}

pub fn import_from_bare_moc3_file(
    moc3_path: &Path,
    texture_paths: &HashMap<usize, PathBuf>,
) -> Result<ImportResult, Status> {
    let bytes = fs::read(moc3_path).map_err(|e| {
        Status::error(
            "PROJECT_IO",
            format!("Failed to read {}: {}", moc3_path.display(), e),
        )
    })?;
    import_from_bare_moc3(&bytes, texture_paths)
}
