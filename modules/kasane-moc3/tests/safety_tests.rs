use kasane_moc3::{
    import_from_bare_moc3, inspect_moc3_safety,
    schema::{section, VersionLayout, SCHEMA},
};
use std::{collections::HashMap, path::PathBuf};

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures")
            .join(name)
            .join("model.moc3"),
    )
    .unwrap()
}
fn set(bytes: &mut [u8], field: usize, value: i32) {
    let offset =
        u32::from_le_bytes(bytes[64 + field * 4..68 + field * 4].try_into().unwrap()) as usize;
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

#[test]
fn native_validation_can_really_be_disabled() {
    if std::env::var_os("KASANE_MOC3_DISABLE_CORE_VALIDATION").is_some() {
        assert!(
            !std::hint::black_box(kasane_moc3::HAS_CORE_VALIDATION),
            "Core validation must not be feature-unified back on"
        );
    }
}

#[test]
fn version_capabilities_reject_absent_fields() {
    let v4 = VersionLayout::new(4).unwrap();
    assert!(v4.require(section::BS_GLUE_SRC_TARGET_IDX).is_err());
    assert!(v4.require(section::ART_MESH_SRC_KEY_COLOR_OFF).is_ok());
    assert_eq!(VersionLayout::new(6).unwrap().offset_count(), 480);
    assert_eq!(VersionLayout::new(5).unwrap().section_count(), 152);
}

#[test]
fn truncated_layout_and_cross_table_references_are_rejected() {
    let original = fixture("external_v50_bs_glue");
    for length in [0, 63, 64, 703, original.len() / 2] {
        assert!(inspect_moc3_safety(&original[..length]).is_err());
    }
    for field in [
        section::BS_GLUE_SRC_TARGET_IDX,
        section::BS_GLUE_SRC_BS_BINDING_OFF,
        section::BS_GLUE_SRC_BS_BINDING_LEN,
        section::BLEND_BINDING_SRC_KEY_TABLE_IDX,
        section::BLEND_BINDING_SRC_KEY_BS_OFF,
        section::BLEND_BINDING_SRC_KEY_BS_LEN,
        section::BLEND_BINDING_SRC_BS_CONSTRAINT_IDX_LEN,
    ] {
        for value in [-2, i32::MAX] {
            let mut bytes = original.clone();
            set(&mut bytes, field, value);
            assert!(
                import_from_bare_moc3(&bytes, &HashMap::new()).is_err(),
                "{}={value}",
                SCHEMA[field].name
            );
        }
    }
}

#[test]
fn mutated_section_values_never_panic() {
    for fixture_name in ["external_v42", "external_cyclic", "external_v50_bs_glue"] {
        let original = fixture(fixture_name);
        let inspected = inspect_moc3_safety(&original).unwrap();
        assert!(import_from_bare_moc3(&original, &HashMap::new()).is_ok());
        for (field, schema) in SCHEMA
            .iter()
            .enumerate()
            .take(VersionLayout::new(original[4]).unwrap().section_count())
            .skip(2)
        {
            if schema.width != 4 {
                continue;
            }
            let start = inspected.section_offsets[field] as usize;
            let end = inspected
                .section_offsets
                .get(field + 1)
                .copied()
                .unwrap_or(original.len() as u32) as usize;
            if end <= start {
                continue;
            }
            for value in [-2, i32::MAX] {
                let mut bytes = original.clone();
                set(&mut bytes, field, value);
                let result =
                    std::panic::catch_unwind(|| import_from_bare_moc3(&bytes, &HashMap::new()));
                assert!(
                    result.is_ok(),
                    "{} {}={value}",
                    fixture_name,
                    SCHEMA[field].name
                );
            }
        }
    }
}

#[test]
fn v6_references_and_stale_reports_are_checked_without_native_code() {
    use kasane_core::{Offscreen, OffscreenKeyform, Part};
    let original = fixture("external_v50_bs_glue");
    let mut doc = import_from_bare_moc3(&original, &HashMap::new())
        .unwrap()
        .document;
    let part = "12340000-1111-4111-8111-111111111111";
    let os = "12340001-1111-4111-8111-111111111111";
    assert!(doc
        .create_part(Part {
            id: part.into(),
            runtime_id: "SafetyPart".into(),
            ..Default::default()
        })
        .status
        .is_ok());
    assert!(doc
        .create_offscreen(Offscreen {
            id: os.into(),
            runtime_id: "SafetyOS".into(),
            part_id: part.into(),
            keyforms: vec![OffscreenKeyform {
                opacity: 0.5,
                ..Default::default()
            }],
            ..Default::default()
        })
        .status
        .is_ok());
    let bytes = kasane_moc3::encode_moc3(&doc).unwrap().bytes;
    let inspection = inspect_moc3_safety(&bytes).unwrap();
    assert_eq!(inspection.version_number, 6);
    assert!(import_from_bare_moc3(&bytes, &HashMap::new()).is_ok());
    for field in [
        section::OFFSCREEN_SRC_OWNER_IDX,
        section::OFFSCREEN_SRC_MASK_LEN,
        section::PART_SRC_OFFSCREEN_IDX,
        section::PART_KEY_SRC_KEY_IDX,
        section::OFFSCREEN_KEY_SRC_KEY_MUL_COLOR_OFF,
        section::OFFSCREEN_KEY_SRC_KEY_SCR_COLOR_OFF,
    ] {
        let mut invalid = bytes.clone();
        set(&mut invalid, field, i32::MAX);
        assert!(
            kasane_moc3::decode_moc3(&invalid, &inspection, &[]).is_err(),
            "{}",
            SCHEMA[field].name
        );
    }
    assert!(inspect_moc3_safety(&bytes[..1983]).is_err());
    for version in [0, 7, 255] {
        let mut invalid = bytes.clone();
        invalid[4] = version;
        assert!(inspect_moc3_safety(&invalid).is_err());
    }
    assert!(kasane_moc3::layout::Layout::with_version(4)
        .field("bs_glue_src.target_idx")
        .is_err());
}

#[test]
fn unsupported_counts_and_cartesian_expansion_fail_before_allocation() {
    fn offset(bytes: &[u8], field: usize) -> usize {
        u32::from_le_bytes(bytes[64 + field * 4..68 + field * 4].try_into().unwrap()) as usize
    }
    let original = fixture("external_v50_bs_glue");
    for count in [35, 36, 37] {
        let mut bytes = original.clone();
        let start = offset(&bytes, section::COUNT_INFO) + count * 4;
        bytes[start..start + 4].copy_from_slice(&i32::MAX.to_le_bytes());
        assert!(import_from_bare_moc3(&bytes, &HashMap::new()).is_err());
    }
    let mut old = fixture("external_v42");
    old[4] = 3;
    let counts = offset(&old, section::COUNT_INFO);
    for index in 23..32 {
        old[counts + index * 4..counts + index * 4 + 4].copy_from_slice(&0i32.to_le_bytes());
    }
    old[counts + 25 * 4..counts + 26 * 4].copy_from_slice(&i32::MAX.to_le_bytes());
    assert!(import_from_bare_moc3(&old, &HashMap::new()).is_err());

    let mut bytes = fixture("external_v42");
    let inspection = inspect_moc3_safety(&bytes).unwrap();
    // Sixteen references to a legal eight-key table describe 8^16 forms in a
    // tiny file. Every individual table/window is in bounds.
    let new_table = (bytes.len() + 7) & !7;
    bytes.resize(new_table + 16 * 4, 0);
    let field = 64 + section::KEY_TABLE_IDX_SRC_IDX * 4;
    bytes[field..field + 4].copy_from_slice(&(new_table as u32).to_le_bytes());
    let counts = offset(&bytes, section::COUNT_INFO);
    bytes[counts + 11 * 4..counts + 12 * 4].copy_from_slice(&16i32.to_le_bytes());
    set(&mut bytes, section::BINDING_SRC_KEY_TABLE_IDX_OFF, 0);
    set(&mut bytes, section::BINDING_SRC_KEY_TABLE_IDX_LEN, 16);
    set(&mut bytes, section::KEY_TABLE_SRC_KEYS_OFF, 0);
    set(&mut bytes, section::KEY_TABLE_SRC_KEYS_LEN, 8);
    let error = kasane_moc3::decode_moc3(&bytes, &inspection, &[]).unwrap_err();
    assert_eq!(error.code, "INDEX_OUT_OF_BOUNDS");
    assert!(error.message.contains("binding grid"));
}
