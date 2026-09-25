//! CDI3 to Document boundary. Import builds an isolated candidate; export is pure.
use std::collections::{BTreeMap, HashMap, HashSet};

use kasane_core::document::{
    CdiCombinedSet, CdiNamespaceIds, CdiParameterEntry, CdiParameterGroup, CdiParameterRef,
    CdiPartEntry, DisplayInfo, DisplayInfoOrigin,
};
use kasane_core::Document;
use kasane_live2d::cdi3::{
    decode_cdi3, encode_cdi3, Cdi3, CdiDiagnostic, CdiParameter, CdiParameterGroup as WireGroup,
    CdiPart, DiagnosticDomain, DiagnosticSeverity,
};
use serde_json::Value;
use sha2::{Digest, Sha256};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CdiProjectDiagnostic {
    pub code: String,
    pub path: String,
    pub message: String,
}

#[derive(Debug)]
pub struct CdiImport {
    pub candidate: Document,
    pub diagnostics: Vec<CdiProjectDiagnostic>,
}

#[derive(Debug, Error)]
#[error("CDI {code} at {path}: {message}")]
pub struct CdiProjectError {
    pub code: String,
    pub path: String,
    pub message: String,
}

fn error(
    code: impl Into<String>,
    path: impl Into<String>,
    message: impl Into<String>,
) -> CdiProjectError {
    CdiProjectError {
        code: code.into(),
        path: path.into(),
        message: message.into(),
    }
}

fn from_wire_error(error_value: kasane_live2d::cdi3::CdiError) -> CdiProjectError {
    if let kasane_live2d::cdi3::CdiError::Diagnostics { diagnostics } = &error_value {
        if let Some(blocking) = diagnostics
            .iter()
            .find(|item| item.severity == DiagnosticSeverity::Error)
        {
            return from_diagnostic(blocking);
        }
    }
    let path = error_value.path().unwrap_or("$").to_owned();
    error("CDI_WIRE", path, error_value.to_string())
}

fn from_diagnostic(diagnostic: &CdiDiagnostic) -> CdiProjectError {
    error(
        format!("{:?}", diagnostic.code),
        &diagnostic.path,
        &diagnostic.message,
    )
}

fn check_string(text: &str, path: &str) -> Result<(), CdiProjectError> {
    if text.contains('\0') {
        Err(error(
            "CDI_NUL_UNPERSISTABLE",
            path,
            "project JSON does not permit embedded NUL",
        ))
    } else {
        Ok(())
    }
}

fn check_value(value: &Value, path: &str) -> Result<(), CdiProjectError> {
    match value {
        Value::String(text) => check_string(text, path),
        Value::Array(items) => {
            for (i, item) in items.iter().enumerate() {
                check_value(item, &format!("{path}[{i}]"))?;
            }
            Ok(())
        }
        Value::Object(items) => check_extensions(items, path),
        _ => Ok(()),
    }
}

fn check_extensions(
    extensions: &serde_json::Map<String, Value>,
    path: &str,
) -> Result<(), CdiProjectError> {
    for (key, value) in extensions {
        let member = format!("{path}.{key}");
        check_string(key, &member)?;
        check_value(value, &member)?;
    }
    Ok(())
}

fn check_btree_extensions(
    extensions: &BTreeMap<String, Value>,
    path: &str,
) -> Result<(), CdiProjectError> {
    for (key, value) in extensions {
        let member = format!("{path}.{key}");
        check_string(key, &member)?;
        check_value(value, &member)?;
    }
    Ok(())
}

fn check_persistable(wire: &Cdi3) -> Result<(), CdiProjectError> {
    check_btree_extensions(&wire.extensions, "$")?;
    if let Some(groups) = &wire.parameter_groups {
        for (i, group) in groups.iter().enumerate() {
            for (name, value) in [
                ("Id", &group.id),
                ("GroupId", &group.group_id),
                ("Name", &group.name),
            ] {
                check_string(value, &format!("$.ParameterGroups[{i}].{name}"))?;
            }
            check_btree_extensions(&group.extensions, &format!("$.ParameterGroups[{i}]"))?;
        }
    }
    if let Some(parameters) = &wire.parameters {
        for (i, parameter) in parameters.iter().enumerate() {
            for (name, value) in [
                ("Id", &parameter.id),
                ("GroupId", &parameter.group_id),
                ("Name", &parameter.name),
            ] {
                check_string(value, &format!("$.Parameters[{i}].{name}"))?;
            }
            check_btree_extensions(&parameter.extensions, &format!("$.Parameters[{i}]"))?;
        }
    }
    if let Some(parts) = &wire.parts {
        for (i, part) in parts.iter().enumerate() {
            for (name, value) in [("Id", &part.id), ("Name", &part.name)] {
                check_string(value, &format!("$.Parts[{i}].{name}"))?;
            }
            check_btree_extensions(&part.extensions, &format!("$.Parts[{i}]"))?;
        }
    }
    if let Some(sets) = &wire.combined_parameters {
        for (i, set) in sets.iter().enumerate() {
            for (j, id) in set.iter().enumerate() {
                check_string(id, &format!("$.CombinedParameters[{i}][{j}]"))?;
            }
        }
    }
    Ok(())
}

fn stable_uuid(
    document: &Document,
    category: &str,
    index: usize,
    runtime_id: &str,
    used: &mut HashSet<String>,
) -> String {
    for salt in 0u64.. {
        let mut digest = Sha256::new();
        digest.update(document.id());
        digest.update(category);
        digest.update(
            u64::try_from(index)
                .expect("CDI index fits u64")
                .to_le_bytes(),
        );
        digest.update(runtime_id);
        digest.update(salt.to_le_bytes());
        let mut bytes: [u8; 16] = digest.finalize()[..16]
            .try_into()
            .expect("SHA-256 has 16 bytes");
        // SHA-256-derived project identity: UUIDv8, not UUIDv5/SHA-1.
        bytes[6] = (bytes[6] & 0x0f) | 0x80;
        bytes[8] = (bytes[8] & 0x3f) | 0x80;
        let hex: String = bytes.iter().map(|byte| format!("{byte:02x}")).collect();
        let id = format!(
            "{}-{}-{}-{}-{}",
            &hex[0..8],
            &hex[8..12],
            &hex[12..16],
            &hex[16..20],
            &hex[20..32]
        );
        if !document.contains_id(&id) && used.insert(id.clone()) {
            return id;
        }
    }
    unreachable!("salt space exhausted")
}

/// Import CDI3 into an isolated candidate. The original document never changes.
/// Missing model targets are retained as runtime IDs and reported, but a
/// structurally invalid CDI fails before any candidate is returned.
pub fn import_cdi3(document: &Document, text: &str) -> Result<CdiImport, CdiProjectError> {
    if !document.initialized() {
        return Err(error(
            "NOT_INITIALIZED",
            "$",
            "initialize the document first",
        ));
    }
    let decoded = decode_cdi3(text).map_err(from_wire_error)?;
    if let Some(diagnostic) = decoded.diagnostics.iter().find(|diagnostic| {
        diagnostic.domain == DiagnosticDomain::FileStructure
            && diagnostic.severity == DiagnosticSeverity::Error
    }) {
        return Err(from_diagnostic(diagnostic));
    }
    let mut diagnostics: Vec<_> = decoded
        .diagnostics
        .iter()
        .filter(|item| item.domain == DiagnosticDomain::FrameworkEncoding)
        .map(|item| CdiProjectDiagnostic {
            code: format!("{:?}", item.code),
            path: item.path.clone(),
            message: item.message.clone(),
        })
        .collect();
    let wire = decoded.document;
    check_persistable(&wire)?;
    let mut candidate = document.fork_candidate();
    let mut info = DisplayInfo {
        origin: DisplayInfoOrigin::Imported,
        extensions: wire.extensions,
        ..DisplayInfo::default()
    };
    let mut used_ids = HashSet::new();
    let old_groups: HashMap<_, _> = document
        .display_info()
        .parameter_groups
        .as_ref()
        .into_iter()
        .flatten()
        .map(|group| (group.runtime_id.as_str(), group.id.as_str()))
        .collect();
    let mut group_ids = HashMap::new();
    if let Some(groups) = wire.parameter_groups {
        for (i, group) in groups.iter().enumerate() {
            let id = old_groups
                .get(group.id.as_str())
                .filter(|id| used_ids.insert((*id).to_string()))
                .map(|id| (*id).to_string())
                .unwrap_or_else(|| stable_uuid(document, "cdi_group", i, &group.id, &mut used_ids));
            group_ids.insert(group.id.clone(), id);
        }
        info.parameter_groups = Some(
            groups
                .into_iter()
                .map(|group| CdiParameterGroup {
                    id: group_ids[&group.id].clone(),
                    runtime_id: group.id,
                    name: group.name,
                    parent_id: (!group.group_id.is_empty())
                        .then(|| group_ids[&group.group_id].clone()),
                    extensions: group.extensions,
                })
                .collect(),
        );
    }
    let parameter_ids: HashMap<_, _> = document
        .parameter_order()
        .iter()
        .map(|id| {
            let parameter = document
                .get_parameter(id)
                .expect("ordered parameter exists");
            (parameter.runtime_id.as_str(), id.as_str())
        })
        .collect();
    if let Some(parameters) = wire.parameters {
        let mut entries = Vec::with_capacity(parameters.len());
        for (i, parameter) in parameters.into_iter().enumerate() {
            let group_id =
                (!parameter.group_id.is_empty()).then(|| group_ids[&parameter.group_id].clone());
            if let Some(id) = parameter_ids.get(parameter.id.as_str()) {
                let result = candidate.set_parameter_display_name(id, parameter.name);
                if !result.status.is_ok() {
                    return Err(error(
                        result.status.code,
                        format!("$.Parameters[{i}].Name"),
                        result.status.message,
                    ));
                }
                entries.push(CdiParameterEntry::Resolved {
                    parameter_id: (*id).into(),
                    group_id,
                    extensions: parameter.extensions,
                });
            } else {
                diagnostics.push(CdiProjectDiagnostic {
                    code: "UNRESOLVED_PARAMETER".into(),
                    path: format!("$.Parameters[{i}].Id"),
                    message: format!("{} is absent from the model", parameter.id),
                });
                entries.push(CdiParameterEntry::Unresolved {
                    runtime_id: parameter.id,
                    name: parameter.name,
                    group_id,
                    extensions: parameter.extensions,
                });
            }
        }
        info.parameters = Some(entries);
    }
    let part_ids: HashMap<_, _> = document
        .part_order()
        .iter()
        .map(|id| {
            let part = document.get_part(id).expect("ordered part exists");
            (part.runtime_id.as_str(), id.as_str())
        })
        .collect();
    if let Some(parts) = wire.parts {
        let mut entries = Vec::with_capacity(parts.len());
        for (i, part) in parts.into_iter().enumerate() {
            if let Some(id) = part_ids.get(part.id.as_str()) {
                let result = candidate.set_part_display_name(id, part.name);
                if !result.status.is_ok() {
                    return Err(error(
                        result.status.code,
                        format!("$.Parts[{i}].Name"),
                        result.status.message,
                    ));
                }
                entries.push(CdiPartEntry::Resolved {
                    part_id: (*id).into(),
                    extensions: part.extensions,
                });
            } else {
                diagnostics.push(CdiProjectDiagnostic {
                    code: "UNRESOLVED_PART".into(),
                    path: format!("$.Parts[{i}].Id"),
                    message: format!("{} is absent from the model", part.id),
                });
                entries.push(CdiPartEntry::Unresolved {
                    runtime_id: part.id,
                    name: part.name,
                    extensions: part.extensions,
                });
            }
        }
        info.parts = Some(entries);
    }
    let old_sets = document.display_info().combined_parameters.as_ref();
    if let Some(sets) = wire.combined_parameters {
        let mut combined = Vec::with_capacity(sets.len());
        for (i, set) in sets.into_iter().enumerate() {
            let id = old_sets
                .and_then(|old| old.get(i))
                .filter(|set| used_ids.insert(set.id.clone()))
                .map(|set| set.id.clone())
                .unwrap_or_else(|| stable_uuid(document, "cdi_set", i, "", &mut used_ids));
            let members = set
                .into_iter()
                .enumerate()
                .map(|(j, runtime_id)| {
                    if let Some(id) = parameter_ids.get(runtime_id.as_str()) {
                        CdiParameterRef::Resolved {
                            parameter_id: (*id).into(),
                        }
                    } else {
                        diagnostics.push(CdiProjectDiagnostic {
                            code: "UNRESOLVED_PARAMETER".into(),
                            path: format!("$.CombinedParameters[{i}][{j}]"),
                            message: format!("{runtime_id} is absent from the model"),
                        });
                        CdiParameterRef::Unresolved { runtime_id }
                    }
                })
                .collect();
            combined.push(CdiCombinedSet { id, members });
        }
        info.combined_parameters = Some(combined);
    }
    if info.has_extensions() {
        info.opaque_source_ids = Some(namespace_ids(&candidate, &info));
    }
    let result = candidate.replace_display_info(info);
    if !result.status.is_ok() {
        return Err(error(result.status.code, "$", result.status.message));
    }
    Ok(CdiImport {
        candidate,
        diagnostics,
    })
}

fn namespace_ids(document: &Document, info: &DisplayInfo) -> CdiNamespaceIds {
    let mut ids = CdiNamespaceIds::default();
    for id in document.parameter_order() {
        ids.parameters.insert(
            id.clone(),
            document
                .get_parameter(id)
                .expect("ordered parameter exists")
                .runtime_id
                .clone(),
        );
    }
    for id in document.part_order() {
        ids.parts.insert(
            id.clone(),
            document
                .get_part(id)
                .expect("ordered part exists")
                .runtime_id
                .clone(),
        );
    }
    if let Some(groups) = &info.parameter_groups {
        for group in groups {
            ids.groups
                .insert(group.id.clone(), group.runtime_id.clone());
        }
    }
    if let Some(entries) = &info.parameters {
        for entry in entries {
            if let CdiParameterEntry::Unresolved { runtime_id, .. } = entry {
                ids.parameters
                    .insert(format!("unresolved:{runtime_id}"), runtime_id.clone());
            }
        }
    }
    if let Some(entries) = &info.parts {
        for entry in entries {
            if let CdiPartEntry::Unresolved { runtime_id, .. } = entry {
                ids.parts
                    .insert(format!("unresolved:{runtime_id}"), runtime_id.clone());
            }
        }
    }
    if let Some(sets) = &info.combined_parameters {
        for set in sets {
            for member in &set.members {
                if let CdiParameterRef::Unresolved { runtime_id } = member {
                    ids.parameters
                        .insert(format!("unresolved:{runtime_id}"), runtime_id.clone());
                }
            }
        }
    }
    ids
}

fn group_runtime_id(
    group_id: Option<&str>,
    path: String,
    ids: &HashMap<&str, &str>,
) -> Result<String, CdiProjectError> {
    match group_id {
        None => Ok(String::new()),
        Some(id) => ids
            .get(id)
            .map(|runtime_id| (*runtime_id).to_owned())
            .ok_or_else(|| error("MISSING_CDI_GROUP", path, format!("group {id} is absent"))),
    }
}

/// Encode CDI3 from current model names and runtime IDs. Unresolved targets
/// and opaque-extension namespace changes are explicit errors.
pub fn export_cdi3(document: &Document) -> Result<String, CdiProjectError> {
    if !document.initialized() {
        return Err(error(
            "NOT_INITIALIZED",
            "$",
            "initialize the document first",
        ));
    }
    if let Some(issue) = document.validate_structure().into_iter().next() {
        return Err(error(issue.status.code, "$", issue.status.message));
    }
    let info = document.display_info();
    if info.has_extensions() {
        let Some(source) = &info.opaque_source_ids else {
            return Err(error(
                "OPAQUE_PROVENANCE_MISSING",
                info.first_extension_path().unwrap_or_else(|| "$".into()),
                "opaque extensions require a persisted source namespace",
            ));
        };
        if *source != namespace_ids(document, info) {
            return Err(error(
                "OPAQUE_NAMESPACE_CHANGED",
                info.first_extension_path().unwrap_or_else(|| "$".into()),
                "runtime namespace changed since opaque extensions were imported",
            ));
        }
    }
    let group_runtime: HashMap<_, _> = info
        .parameter_groups
        .as_ref()
        .into_iter()
        .flatten()
        .map(|group| (group.id.as_str(), group.runtime_id.as_str()))
        .collect();
    let groups = info
        .parameter_groups
        .as_ref()
        .map(|groups| {
            groups
                .iter()
                .enumerate()
                .map(|(i, group)| {
                    Ok(WireGroup {
                        id: group.runtime_id.clone(),
                        group_id: group_runtime_id(
                            group.parent_id.as_deref(),
                            format!("$.ParameterGroups[{i}].GroupId"),
                            &group_runtime,
                        )?,
                        name: group.name.clone(),
                        extensions: group.extensions.clone(),
                    })
                })
                .collect::<Result<Vec<_>, CdiProjectError>>()
        })
        .transpose()?;
    let parameter_entries =
        if info.origin == DisplayInfoOrigin::Generated && info.parameters.is_none() {
            Some(
                document
                    .parameter_order()
                    .iter()
                    .map(|id| CdiParameterEntry::Resolved {
                        parameter_id: id.clone(),
                        group_id: None,
                        extensions: BTreeMap::new(),
                    })
                    .collect(),
            )
        } else {
            info.parameters.clone()
        };
    let parameters = parameter_entries
        .map(|entries| {
            entries
                .into_iter()
                .enumerate()
                .map(|(i, entry)| match entry {
                    CdiParameterEntry::Resolved {
                        parameter_id,
                        group_id,
                        extensions,
                    } => {
                        let parameter = document.get_parameter(&parameter_id).ok_or_else(|| {
                            error(
                                "MISSING_PARAMETER",
                                format!("$.Parameters[{i}].Id"),
                                "resolved parameter absent",
                            )
                        })?;
                        Ok(CdiParameter {
                            id: parameter.runtime_id.clone(),
                            group_id: group_runtime_id(
                                group_id.as_deref(),
                                format!("$.Parameters[{i}].GroupId"),
                                &group_runtime,
                            )?,
                            name: parameter.name.clone(),
                            extensions,
                        })
                    }
                    CdiParameterEntry::Unresolved { runtime_id, .. } => Err(error(
                        "UNRESOLVED_PARAMETER",
                        format!("$.Parameters[{i}].Id"),
                        format!("{runtime_id} is absent from the model"),
                    )),
                })
                .collect::<Result<Vec<_>, _>>()
        })
        .transpose()?;
    let part_entries = if info.origin == DisplayInfoOrigin::Generated && info.parts.is_none() {
        Some(
            document
                .part_order()
                .iter()
                .map(|id| CdiPartEntry::Resolved {
                    part_id: id.clone(),
                    extensions: BTreeMap::new(),
                })
                .collect(),
        )
    } else {
        info.parts.clone()
    };
    let parts = part_entries
        .map(|entries| {
            entries
                .into_iter()
                .enumerate()
                .map(|(i, entry)| match entry {
                    CdiPartEntry::Resolved {
                        part_id,
                        extensions,
                    } => {
                        let part = document.get_part(&part_id).ok_or_else(|| {
                            error(
                                "MISSING_PART",
                                format!("$.Parts[{i}].Id"),
                                "resolved part absent",
                            )
                        })?;
                        Ok(CdiPart {
                            id: part.runtime_id.clone(),
                            name: part.name.clone(),
                            extensions,
                        })
                    }
                    // CDI is display metadata. Cubism samples can contain
                    // labels for Parts absent from the current MOC; retain
                    // those labels without changing the model namespace.
                    CdiPartEntry::Unresolved {
                        runtime_id,
                        name,
                        extensions,
                    } => Ok(CdiPart {
                        id: runtime_id,
                        name,
                        extensions,
                    }),
                })
                .collect::<Result<Vec<_>, _>>()
        })
        .transpose()?;
    let combined_parameters = info
        .combined_parameters
        .as_ref()
        .map(|sets| {
            sets.iter()
                .enumerate()
                .map(|(i, set)| {
                    set.members
                        .iter()
                        .enumerate()
                        .map(|(j, member)| match member {
                            CdiParameterRef::Resolved { parameter_id } => document
                                .get_parameter(parameter_id)
                                .map(|parameter| parameter.runtime_id.clone())
                                .ok_or_else(|| {
                                    error(
                                        "MISSING_PARAMETER",
                                        format!("$.CombinedParameters[{i}][{j}]"),
                                        "resolved parameter absent",
                                    )
                                }),
                            CdiParameterRef::Unresolved { runtime_id } => Err(error(
                                "UNRESOLVED_PARAMETER",
                                format!("$.CombinedParameters[{i}][{j}]"),
                                format!("{runtime_id} is absent from the model"),
                            )),
                        })
                        .collect::<Result<Vec<_>, _>>()
                })
                .collect::<Result<Vec<_>, _>>()
        })
        .transpose()?;
    encode_cdi3(&Cdi3 {
        version: 3,
        parameters,
        parameter_groups: groups,
        parts,
        combined_parameters,
        extensions: info.extensions.clone(),
    })
    .map_err(from_wire_error)
}
