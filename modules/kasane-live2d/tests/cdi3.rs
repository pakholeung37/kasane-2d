use std::collections::BTreeMap;

use kasane_live2d::cdi3::{
    decode_cdi3, encode_cdi3, validate_cdi3, Cdi3, CdiDiagnostic, CdiError, CdiParameterGroup,
    DiagnosticCode, DiagnosticDomain, DiagnosticSeverity,
};
use serde_json::json;

fn has(diagnostics: &[CdiDiagnostic], code: DiagnosticCode, domain: DiagnosticDomain) -> bool {
    diagnostics
        .iter()
        .any(|d| d.code == code && d.domain == domain)
}

#[test]
fn roundtrip_preserves_utf8_order_optional_collections_and_extensions() {
    let source = r#"{
      "Version": 3,
      "Parameters": [
        {"Id":"ParamA","GroupId":"Face","Name":"角度 X","FutureLabel":"保持"},
        {"Id":"ParamB","GroupId":"Face","Name":"角度 X"}
      ],
      "ParameterGroups": [{"Id":"Face","GroupId":"","Name":"顔"}],
      "Parts": [{"Id":"PartA","Name":"头发"}],
      "CombinedParameters": [["ParamA","ParamB"]],
      "Future": {"Enabled":true,"Tags":["甲",null]}
    }"#;
    let decoded = decode_cdi3(source).unwrap();
    assert!(decoded.diagnostics.is_empty());
    assert_eq!(
        decoded.document.parameters.as_ref().unwrap()[0].name,
        "角度 X"
    );
    assert_eq!(
        decoded.document.parameters.as_ref().unwrap()[1].name,
        "角度 X"
    );
    let encoded = encode_cdi3(&decoded.document).unwrap();
    assert!(encoded.contains("角度 X"));
    assert!(encoded.contains("头发"));
    assert_eq!(decode_cdi3(&encoded).unwrap().document, decoded.document);
    assert_eq!(
        decoded.document.combined_parameters.unwrap()[0],
        ["ParamA", "ParamB"]
    );

    let missing = decode_cdi3(r#"{"Version":3}"#).unwrap().document;
    assert!(missing.parameters.is_none());
    let empty = decode_cdi3(r#"{"Version":3,"Parameters":[]}"#)
        .unwrap()
        .document;
    assert_eq!(empty.parameters, Some(vec![]));
    assert_ne!(encode_cdi3(&missing).unwrap(), encode_cdi3(&empty).unwrap());
}

#[test]
fn duplicate_json_members_and_wrong_types_report_paths() {
    let duplicate = r#"{"Version":3,"Parameters":[{"Id":"A","Id":"B","GroupId":"","Name":"x"}]}"#;
    let error = decode_cdi3(duplicate).unwrap_err();
    assert!(matches!(error, CdiError::InvalidJson { .. }));
    assert!(error.to_string().contains("duplicate JSON member"));
    assert_eq!(error.path(), Some("$.Parameters[0].Id"));
    let duplicate_extension = r#"{"Version":3,"Future":{"Tag":true,"Tag":false}}"#;
    assert_eq!(
        decode_cdi3(duplicate_extension).unwrap_err().path(),
        Some("$.Future.Tag")
    );
    let wrong_type = r#"{"Version":3,"Parameters":[{"Id":"A","GroupId":"","Name":2}]}"#;
    let error = decode_cdi3(wrong_type).unwrap_err();
    assert!(matches!(error, CdiError::InvalidField { .. }));
    assert!(error.path().unwrap().contains("Parameters[0].Name"));
}

#[test]
fn long_group_chains_and_cycles_have_distinct_results() {
    let mut document = Cdi3 {
        parameter_groups: Some(
            (0..2000)
                .map(|i| CdiParameterGroup {
                    id: format!("Group{i}"),
                    group_id: if i == 0 {
                        String::new()
                    } else {
                        format!("Group{}", i - 1)
                    },
                    name: format!("Group {i}"),
                    extensions: BTreeMap::new(),
                })
                .collect(),
        ),
        ..Cdi3::default()
    };
    assert!(validate_cdi3(&document).is_empty());
    document.parameter_groups.as_mut().unwrap()[0].group_id = "Group1999".into();
    let diagnostics = validate_cdi3(&document);
    assert_eq!(
        diagnostics
            .iter()
            .filter(|d| d.code == DiagnosticCode::GroupCycle)
            .count(),
        1
    );
}

#[test]
fn local_structure_and_model_dependency_are_separate() {
    let text = r#"{
      "Version":3,
      "Parameters":[
        {"Id":"P","GroupId":"Missing","Name":"one"},
        {"Id":"P","GroupId":"G1","Name":"two"}
      ],
      "ParameterGroups":[
        {"Id":"G1","GroupId":"G2","Name":"one"},
        {"Id":"G2","GroupId":"G1","Name":"two"}
      ],
      "CombinedParameters":[["P","ExternalParam","P"]]
    }"#;
    let decoded = decode_cdi3(text).unwrap();
    let diagnostics = &decoded.diagnostics;
    assert!(has(
        diagnostics,
        DiagnosticCode::DuplicateId,
        DiagnosticDomain::FileStructure
    ));
    assert!(has(
        diagnostics,
        DiagnosticCode::MissingGroup,
        DiagnosticDomain::FileStructure
    ));
    assert!(has(
        diagnostics,
        DiagnosticCode::GroupCycle,
        DiagnosticDomain::FileStructure
    ));
    assert!(has(
        diagnostics,
        DiagnosticCode::DuplicateCombinationMember,
        DiagnosticDomain::FileStructure
    ));
    assert!(has(
        diagnostics,
        DiagnosticCode::CombinationMemberNeedsModel,
        DiagnosticDomain::ModelDependency
    ));
    assert!(encode_cdi3(&decoded.document).is_err());

    let unresolved =
        decode_cdi3(r#"{"Version":3,"CombinedParameters":[["ModelA","ModelB"]]}"#).unwrap();
    assert_eq!(unresolved.diagnostics.len(), 2);
    assert!(unresolved
        .diagnostics
        .iter()
        .all(|d| d.domain == DiagnosticDomain::ModelDependency
            && d.severity == DiagnosticSeverity::Warning));
    assert!(encode_cdi3(&unresolved.document).is_ok());
}

#[test]
fn unknown_numeric_values_survive_decode_but_block_first_writer() {
    let source = r#"{"Version":3,"Future":{"Gain":0.5,"Flag":true}}"#;
    let decoded = decode_cdi3(source).unwrap();
    assert_eq!(decoded.document.extensions["Future"]["Gain"], json!(0.5));
    assert!(has(
        &decoded.diagnostics,
        DiagnosticCode::UnsupportedExtensionNumber,
        DiagnosticDomain::FrameworkEncoding
    ));
    let error = encode_cdi3(&decoded.document).unwrap_err();
    assert_eq!(error.path(), Some("$.Future.Gain"));
}

#[test]
fn unsupported_controls_and_programmatic_key_collisions_block_encoding() {
    let accepted =
        decode_cdi3(r#"{"Version":3,"Parts":[{"Id":"PartA","Name":"中\n文"}]}"#).unwrap();
    assert!(encode_cdi3(&accepted.document).is_ok());

    let unsupported =
        decode_cdi3(r#"{"Version":3,"Parts":[{"Id":"PartA","Name":"bad\u0001"}]}"#).unwrap();
    assert!(has(
        &unsupported.diagnostics,
        DiagnosticCode::UnsupportedControlCharacter,
        DiagnosticDomain::FrameworkEncoding
    ));
    assert!(encode_cdi3(&unsupported.document).is_err());

    let mut document = Cdi3 {
        extensions: BTreeMap::new(),
        ..Cdi3::default()
    };
    document
        .extensions
        .insert("Version".into(), json!("shadow"));
    let diagnostics = validate_cdi3(&document);
    assert!(has(
        &diagnostics,
        DiagnosticCode::ExtensionKeyCollision,
        DiagnosticDomain::FileStructure
    ));
    assert!(encode_cdi3(&document).is_err());
}

#[test]
fn version_three_is_required_for_writing() {
    let decoded = decode_cdi3(r#"{"Version":2}"#).unwrap();
    assert!(has(
        &decoded.diagnostics,
        DiagnosticCode::UnsupportedVersion,
        DiagnosticDomain::FileStructure
    ));
    assert!(encode_cdi3(&decoded.document).is_err());
}
