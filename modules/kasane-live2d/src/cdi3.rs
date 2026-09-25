//! CDI3 display-information wire format and local structural diagnostics.
//!
//! IDs here are external Live2D runtime IDs. Resolving them to document UUIDs
//! or checking them against a MOC belongs to the project layer.

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt;
use std::rc::Rc;

use serde::de::{DeserializeSeed, Deserializer, MapAccess, SeqAccess, Visitor};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Cdi3 {
    #[serde(rename = "Version")]
    pub version: u32,
    #[serde(
        rename = "Parameters",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub parameters: Option<Vec<CdiParameter>>,
    #[serde(
        rename = "ParameterGroups",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub parameter_groups: Option<Vec<CdiParameterGroup>>,
    #[serde(rename = "Parts", default, skip_serializing_if = "Option::is_none")]
    pub parts: Option<Vec<CdiPart>>,
    #[serde(
        rename = "CombinedParameters",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub combined_parameters: Option<Vec<Vec<String>>>,
    #[serde(flatten)]
    pub extensions: BTreeMap<String, Value>,
}

impl Default for Cdi3 {
    fn default() -> Self {
        Self {
            version: 3,
            parameters: None,
            parameter_groups: None,
            parts: None,
            combined_parameters: None,
            extensions: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CdiParameter {
    #[serde(rename = "Id")]
    pub id: String,
    #[serde(rename = "GroupId")]
    pub group_id: String,
    #[serde(rename = "Name")]
    pub name: String,
    #[serde(flatten)]
    pub extensions: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CdiParameterGroup {
    #[serde(rename = "Id")]
    pub id: String,
    #[serde(rename = "GroupId")]
    pub group_id: String,
    #[serde(rename = "Name")]
    pub name: String,
    #[serde(flatten)]
    pub extensions: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CdiPart {
    #[serde(rename = "Id")]
    pub id: String,
    #[serde(rename = "Name")]
    pub name: String,
    #[serde(flatten)]
    pub extensions: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticDomain {
    FileStructure,
    ModelDependency,
    FrameworkEncoding,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticSeverity {
    Error,
    Warning,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticCode {
    UnsupportedVersion,
    EmptyId,
    DuplicateId,
    MissingGroup,
    GroupCycle,
    EmptyCombination,
    DuplicateCombinationMember,
    CombinationMemberNeedsModel,
    ExtensionKeyCollision,
    UnsupportedExtensionNumber,
    UnsupportedControlCharacter,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CdiDiagnostic {
    pub domain: DiagnosticDomain,
    pub severity: DiagnosticSeverity,
    pub code: DiagnosticCode,
    /// JSONPath-like location, rooted at `$`.
    pub path: String,
    pub message: String,
}

impl CdiDiagnostic {
    fn error(
        domain: DiagnosticDomain,
        code: DiagnosticCode,
        path: String,
        message: String,
    ) -> Self {
        Self {
            domain,
            severity: DiagnosticSeverity::Error,
            code,
            path,
            message,
        }
    }

    fn warning(
        domain: DiagnosticDomain,
        code: DiagnosticCode,
        path: String,
        message: String,
    ) -> Self {
        Self {
            domain,
            severity: DiagnosticSeverity::Warning,
            code,
            path,
            message,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct DecodedCdi3 {
    pub document: Cdi3,
    pub diagnostics: Vec<CdiDiagnostic>,
}

#[derive(Debug, Error)]
pub enum CdiError {
    #[error("invalid CDI3 JSON at {path}: {message}")]
    InvalidJson { path: String, message: String },
    #[error("invalid CDI3 field at {path}: {message}")]
    InvalidField { path: String, message: String },
    #[error("CDI3 has blocking diagnostics")]
    Diagnostics { diagnostics: Vec<CdiDiagnostic> },
    #[error("could not serialize CDI3 at {path}: {message}")]
    Serialization { path: String, message: String },
}

impl CdiError {
    /// Returns the first field path for adapters that expose one primary error.
    pub fn path(&self) -> Option<&str> {
        match self {
            Self::InvalidJson { path, .. }
            | Self::InvalidField { path, .. }
            | Self::Serialization { path, .. } => Some(path),
            Self::Diagnostics { diagnostics } => diagnostics.first().map(|d| d.path.as_str()),
        }
    }
}

/// Decode known CDI3 fields and preserve unknown fields at the root and item level.
/// Structural and model-dependent findings are returned alongside the document.
pub fn decode_cdi3(text: &str) -> Result<DecodedCdi3, CdiError> {
    check_json_members(text)?;
    let mut deserializer = serde_json::Deserializer::from_str(text);
    let document: Cdi3 = serde_path_to_error::deserialize(&mut deserializer).map_err(|error| {
        CdiError::InvalidField {
            path: format!("$.{}", error.path()),
            message: error.inner().to_string(),
        }
    })?;
    deserializer.end().map_err(|error| CdiError::InvalidJson {
        path: "$".to_owned(),
        message: error.to_string(),
    })?;
    let diagnostics = validate_cdi3(&document);
    Ok(DecodedCdi3 {
        document,
        diagnostics,
    })
}

/// Encode only when the writer can preserve all known and unknown fields in
/// the subset verified against this Framework. Model-dependent warnings do
/// not block encoding; resolving runtime IDs needs a model at a higher layer.
pub fn encode_cdi3(document: &Cdi3) -> Result<String, CdiError> {
    let diagnostics = validate_cdi3(document);
    if diagnostics
        .iter()
        .any(|d| d.severity == DiagnosticSeverity::Error)
    {
        return Err(CdiError::Diagnostics { diagnostics });
    }
    serde_json::to_string_pretty(document).map_err(|error| CdiError::Serialization {
        path: "$".into(),
        message: error.to_string(),
    })
}

/// Check only CDI-local structure and the writer's supported string/extension
/// subset. Warnings in `ModelDependency` defer resolution to the model layer.
pub fn validate_cdi3(document: &Cdi3) -> Vec<CdiDiagnostic> {
    let mut diagnostics = Vec::new();
    if document.version != 3 {
        diagnostics.push(CdiDiagnostic::error(
            DiagnosticDomain::FileStructure,
            DiagnosticCode::UnsupportedVersion,
            "$.Version".into(),
            format!("CDI3 requires Version 3, got {}", document.version),
        ));
    }
    validate_extensions(
        &document.extensions,
        &[
            "Version",
            "Parameters",
            "ParameterGroups",
            "Parts",
            "CombinedParameters",
        ],
        "$",
        &mut diagnostics,
    );

    let mut parameter_ids = HashSet::new();
    if let Some(parameters) = &document.parameters {
        for (i, parameter) in parameters.iter().enumerate() {
            let base = format!("$.Parameters[{i}]");
            validate_id(
                &parameter.id,
                format!("{base}.Id"),
                &mut parameter_ids,
                &mut diagnostics,
            );
            validate_text(
                &parameter.group_id,
                &format!("{base}.GroupId"),
                &mut diagnostics,
            );
            validate_text(&parameter.name, &format!("{base}.Name"), &mut diagnostics);
            validate_extensions(
                &parameter.extensions,
                &["Id", "GroupId", "Name"],
                &base,
                &mut diagnostics,
            );
        }
    }

    let mut group_ids = HashMap::new();
    if let Some(groups) = &document.parameter_groups {
        let mut seen = HashSet::new();
        for (i, group) in groups.iter().enumerate() {
            let base = format!("$.ParameterGroups[{i}]");
            validate_id(&group.id, format!("{base}.Id"), &mut seen, &mut diagnostics);
            group_ids.entry(group.id.as_str()).or_insert(i);
            validate_text(
                &group.group_id,
                &format!("{base}.GroupId"),
                &mut diagnostics,
            );
            validate_text(&group.name, &format!("{base}.Name"), &mut diagnostics);
            validate_extensions(
                &group.extensions,
                &["Id", "GroupId", "Name"],
                &base,
                &mut diagnostics,
            );
        }
        for (i, group) in groups.iter().enumerate() {
            if !group.group_id.is_empty() && !group_ids.contains_key(group.group_id.as_str()) {
                diagnostics.push(CdiDiagnostic::error(
                    DiagnosticDomain::FileStructure,
                    DiagnosticCode::MissingGroup,
                    format!("$.ParameterGroups[{i}].GroupId"),
                    format!("parent group {:?} is absent from this CDI", group.group_id),
                ));
            }
        }
        // A parent edge is visited once, including edges that lead into an
        // already completed chain. A long valid hierarchy must remain linear.
        let mut completed = HashSet::new();
        for (i, _) in groups.iter().enumerate() {
            if completed.contains(&i) {
                continue;
            }
            let mut path = Vec::new();
            let mut positions = HashMap::new();
            let mut current = Some(i);
            while let Some(index) = current {
                if completed.contains(&index) {
                    break;
                }
                if let Some(start) = positions.get(&index) {
                    diagnostics.push(CdiDiagnostic::error(
                        DiagnosticDomain::FileStructure,
                        DiagnosticCode::GroupCycle,
                        format!("$.ParameterGroups[{}].GroupId", path[*start]),
                        "parameter-group parent chain contains a cycle".into(),
                    ));
                    break;
                }
                positions.insert(index, path.len());
                path.push(index);
                current = group_ids.get(groups[index].group_id.as_str()).copied();
            }
            completed.extend(path);
        }
    }
    if let Some(parameters) = &document.parameters {
        for (i, parameter) in parameters.iter().enumerate() {
            if !parameter.group_id.is_empty()
                && !group_ids.contains_key(parameter.group_id.as_str())
            {
                diagnostics.push(CdiDiagnostic::error(
                    DiagnosticDomain::FileStructure,
                    DiagnosticCode::MissingGroup,
                    format!("$.Parameters[{i}].GroupId"),
                    format!("group {:?} is absent from this CDI", parameter.group_id),
                ));
            }
        }
    }

    let mut part_ids = HashSet::new();
    if let Some(parts) = &document.parts {
        for (i, part) in parts.iter().enumerate() {
            let base = format!("$.Parts[{i}]");
            validate_id(
                &part.id,
                format!("{base}.Id"),
                &mut part_ids,
                &mut diagnostics,
            );
            validate_text(&part.name, &format!("{base}.Name"), &mut diagnostics);
            validate_extensions(&part.extensions, &["Id", "Name"], &base, &mut diagnostics);
        }
    }

    if let Some(combinations) = &document.combined_parameters {
        for (i, combination) in combinations.iter().enumerate() {
            if combination.is_empty() {
                diagnostics.push(CdiDiagnostic::error(
                    DiagnosticDomain::FileStructure,
                    DiagnosticCode::EmptyCombination,
                    format!("$.CombinedParameters[{i}]"),
                    "combined parameter set is empty".into(),
                ));
            }
            let mut seen = HashSet::new();
            for (j, id) in combination.iter().enumerate() {
                let path = format!("$.CombinedParameters[{i}][{j}]");
                validate_text(id, &path, &mut diagnostics);
                if id.is_empty() {
                    diagnostics.push(CdiDiagnostic::error(
                        DiagnosticDomain::FileStructure,
                        DiagnosticCode::EmptyId,
                        path.clone(),
                        "combined parameter ID is empty".into(),
                    ));
                } else if !seen.insert(id.as_str()) {
                    diagnostics.push(CdiDiagnostic::error(
                        DiagnosticDomain::FileStructure,
                        DiagnosticCode::DuplicateCombinationMember,
                        path.clone(),
                        format!("parameter {id:?} occurs twice in one combination"),
                    ));
                }
                if !id.is_empty() && !parameter_ids.contains(id.as_str()) {
                    diagnostics.push(CdiDiagnostic::warning(
                        DiagnosticDomain::ModelDependency,
                        DiagnosticCode::CombinationMemberNeedsModel,
                        path,
                        format!(
                            "parameter {id:?} is not listed in CDI; resolve against model later"
                        ),
                    ));
                }
            }
        }
    }
    diagnostics
}

fn validate_id<'a>(
    id: &'a str,
    path: String,
    seen: &mut HashSet<&'a str>,
    diagnostics: &mut Vec<CdiDiagnostic>,
) {
    validate_text(id, &path, diagnostics);
    if id.is_empty() {
        diagnostics.push(CdiDiagnostic::error(
            DiagnosticDomain::FileStructure,
            DiagnosticCode::EmptyId,
            path,
            "runtime ID is empty".into(),
        ));
    } else if !seen.insert(id) {
        diagnostics.push(CdiDiagnostic::error(
            DiagnosticDomain::FileStructure,
            DiagnosticCode::DuplicateId,
            path,
            format!("runtime ID {id:?} occurs more than once in this CDI namespace"),
        ));
    }
}

fn validate_text(text: &str, path: &str, diagnostics: &mut Vec<CdiDiagnostic>) {
    if let Some(character) = text
        .chars()
        .find(|c| *c <= '\u{1f}' && !matches!(*c, '\u{8}' | '\t' | '\n' | '\u{c}' | '\r'))
    {
        diagnostics.push(CdiDiagnostic::error(
            DiagnosticDomain::FrameworkEncoding,
            DiagnosticCode::UnsupportedControlCharacter,
            path.into(),
            format!(
                "control character U+{:04X} cannot be emitted for this Framework parser",
                character as u32
            ),
        ));
    }
}

fn validate_extensions(
    extensions: &BTreeMap<String, Value>,
    reserved: &[&str],
    base: &str,
    diagnostics: &mut Vec<CdiDiagnostic>,
) {
    for (key, value) in extensions {
        let path = format!("{base}.{key}");
        validate_text(key, &path, diagnostics);
        if reserved.contains(&key.as_str()) {
            diagnostics.push(CdiDiagnostic::error(
                DiagnosticDomain::FileStructure,
                DiagnosticCode::ExtensionKeyCollision,
                path.clone(),
                format!("extension key {key:?} collides with a typed CDI field"),
            ));
        }
        validate_extension_value(value, &path, diagnostics);
    }
}

fn validate_extension_value(value: &Value, path: &str, diagnostics: &mut Vec<CdiDiagnostic>) {
    match value {
        Value::Number(_) => diagnostics.push(CdiDiagnostic::error(
            DiagnosticDomain::FrameworkEncoding,
            DiagnosticCode::UnsupportedExtensionNumber,
            path.into(),
            "P1-A writer has not verified numeric extension encoding".into(),
        )),
        Value::String(text) => validate_text(text, path, diagnostics),
        Value::Array(items) => {
            for (i, item) in items.iter().enumerate() {
                validate_extension_value(item, &format!("{path}[{i}]"), diagnostics);
            }
        }
        Value::Object(items) => {
            for (key, item) in items {
                let member_path = format!("{path}.{key}");
                validate_text(key, &member_path, diagnostics);
                validate_extension_value(item, &member_path, diagnostics);
            }
        }
        Value::Null | Value::Bool(_) => {}
    }
}

struct UniqueMemberSeed {
    path: String,
    depth: usize,
    duplicate_path: Rc<RefCell<Option<String>>>,
}

impl<'de> DeserializeSeed<'de> for UniqueMemberSeed {
    type Value = ();

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_any(self)
    }
}

impl<'de> Visitor<'de> for UniqueMemberSeed {
    type Value = ();
    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("JSON without duplicate members")
    }
    fn visit_map<M>(self, mut map: M) -> Result<(), M::Error>
    where
        M: MapAccess<'de>,
    {
        if self.depth > 128 {
            return Err(serde::de::Error::custom(format!(
                "JSON nesting exceeds 128 at {}",
                self.path
            )));
        }
        let mut seen = HashSet::new();
        while let Some(key) = map.next_key::<String>()? {
            let path = format!("{}.{}", self.path, key);
            if !seen.insert(key) {
                *self.duplicate_path.borrow_mut() = Some(path.clone());
                return Err(serde::de::Error::custom(format!(
                    "duplicate JSON member at {path}"
                )));
            }
            map.next_value_seed(UniqueMemberSeed {
                path,
                depth: self.depth + 1,
                duplicate_path: self.duplicate_path.clone(),
            })?;
        }
        Ok(())
    }
    fn visit_seq<S>(self, mut sequence: S) -> Result<(), S::Error>
    where
        S: SeqAccess<'de>,
    {
        if self.depth > 128 {
            return Err(serde::de::Error::custom(format!(
                "JSON nesting exceeds 128 at {}",
                self.path
            )));
        }
        let mut index = 0;
        while sequence
            .next_element_seed(UniqueMemberSeed {
                path: format!("{}[{index}]", self.path),
                depth: self.depth + 1,
                duplicate_path: self.duplicate_path.clone(),
            })?
            .is_some()
        {
            index += 1;
        }
        Ok(())
    }
    fn visit_bool<E>(self, _: bool) -> Result<(), E> {
        Ok(())
    }
    fn visit_i64<E>(self, _: i64) -> Result<(), E> {
        Ok(())
    }
    fn visit_u64<E>(self, _: u64) -> Result<(), E> {
        Ok(())
    }
    fn visit_f64<E>(self, _: f64) -> Result<(), E> {
        Ok(())
    }
    fn visit_str<E>(self, _: &str) -> Result<(), E> {
        Ok(())
    }
    fn visit_unit<E>(self) -> Result<(), E> {
        Ok(())
    }
}

pub(crate) fn check_json_members(text: &str) -> Result<(), CdiError> {
    let mut deserializer = serde_json::Deserializer::from_str(text);
    let duplicate_path = Rc::new(RefCell::new(None));
    UniqueMemberSeed {
        path: "$".into(),
        depth: 0,
        duplicate_path: duplicate_path.clone(),
    }
    .deserialize(&mut deserializer)
    .map_err(|error| CdiError::InvalidJson {
        path: duplicate_path
            .borrow()
            .clone()
            .unwrap_or_else(|| "$".into()),
        message: error.to_string(),
    })?;
    deserializer.end().map_err(|error| CdiError::InvalidJson {
        path: "$".into(),
        message: error.to_string(),
    })
}
