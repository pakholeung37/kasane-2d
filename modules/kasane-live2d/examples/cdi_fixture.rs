//! Deterministic, self-made CDI3 used by the official Framework CPU oracle.
use std::collections::BTreeMap;

use kasane_live2d::cdi3::{encode_cdi3, Cdi3, CdiParameter, CdiParameterGroup, CdiPart};
use serde_json::json;

fn main() {
    let mut document = Cdi3 {
        parameters: Some(vec![
            CdiParameter {
                id: "ParamAngleX".into(),
                group_id: "Face".into(),
                name: "角度\n\"X\"\\位置".into(),
                extensions: BTreeMap::from([("Hint".into(), json!("追加\t説明"))]),
            },
            CdiParameter {
                id: "ParamAngleY".into(),
                group_id: "Face".into(),
                name: "角度\n\"X\"\\位置".into(),
                extensions: BTreeMap::new(),
            },
        ]),
        parameter_groups: Some(vec![CdiParameterGroup {
            id: "Face".into(),
            group_id: String::new(),
            name: "顔".into(),
            extensions: BTreeMap::new(),
        }]),
        parts: Some(vec![CdiPart {
            id: "PartHair".into(),
            name: "头发".into(),
            extensions: BTreeMap::new(),
        }]),
        combined_parameters: Some(vec![vec!["ParamAngleX".into(), "ParamAngleY".into()]]),
        ..Cdi3::default()
    };
    document
        .extensions
        .insert("Future".into(), json!({"Label":"扩展", "Enabled":true}));
    println!(
        "{}",
        encode_cdi3(&document).expect("self-made CDI3 must be encodable")
    );
}
