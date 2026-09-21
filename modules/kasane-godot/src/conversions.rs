use godot::builtin::{VarArray, VarDictionary};
use godot::prelude::*;
use kasane_core::types::{
    Appearance, BindingAxis, BlendMode, Mesh, MeshBinding, MeshKeyform, Parameter, Part,
    RotationPose, SceneBinding, SceneKeyform, Status, Transform, TransformKind, Vec2,
};
use kasane_project::store::ProjectResult;

pub type Dictionary = VarDictionary;
pub type Array = VarArray;

pub fn is_main_thread() -> bool {
    let os = godot::classes::Os::singleton();
    os.get_thread_caller_id() == os.get_main_thread_id()
}

pub fn status_to_dict(s: &Status) -> Dictionary {
    let mut d = Dictionary::new();
    d.set("ok", s.is_ok());
    d.set("code", s.code.as_str());
    d.set("message", s.message.as_str());
    d
}

pub fn error_dict(code: &str, message: &str) -> Dictionary {
    status_to_dict(&Status::error(code, message))
}

pub fn vectors_to_packed(vectors: &[Vec2]) -> PackedVector2Array {
    let mut out = PackedVector2Array::new();
    out.resize(vectors.len());
    for (i, v) in vectors.iter().enumerate() {
        out[i] = Vector2::new(v.x, v.y);
    }
    out
}

pub fn packed_to_vectors(packed: &PackedVector2Array) -> Vec<Vec2> {
    let mut out = Vec::with_capacity(packed.len());
    for i in 0..packed.len() {
        let v = packed[i];
        out.push(Vec2::new(v.x, v.y));
    }
    out
}

pub fn ids_to_packed(ids: &[u32]) -> PackedInt64Array {
    let mut out = PackedInt64Array::new();
    out.resize(ids.len());
    for (i, &id) in ids.iter().enumerate() {
        out[i] = id as i64;
    }
    out
}

pub fn packed_to_ids(packed: &PackedInt64Array) -> Result<Vec<u32>, Status> {
    let mut out = Vec::with_capacity(packed.len());
    for i in 0..packed.len() {
        let v = packed[i];
        if v < 0 || v > u32::MAX as i64 {
            return Err(Status::error(
                "INVALID_VERTEX_ID",
                "Vertex IDs must fit uint32.",
            ));
        }
        out.push(v as u32);
    }
    Ok(out)
}

fn fail() -> Status {
    Status::error(
        "INVALID_FIELD",
        "Required field is absent or has the wrong type",
    )
}

fn get_str(d: &Dictionary, key: &str) -> Result<String, Status> {
    match d.get(key) {
        Some(v) => v
            .try_to::<GString>()
            .map(|s| s.to_string())
            .map_err(|_| fail()),
        None => Err(fail()),
    }
}

fn get_f32(d: &Dictionary, key: &str) -> Result<f32, Status> {
    match d.get(key) {
        Some(v) => {
            if let Ok(f) = v.try_to::<f64>() {
                Ok(f as f32)
            } else if let Ok(i) = v.try_to::<i64>() {
                Ok(i as f32)
            } else {
                Err(fail())
            }
        }
        None => Err(fail()),
    }
}

fn get_bool(d: &Dictionary, key: &str) -> Result<bool, Status> {
    match d.get(key) {
        Some(v) => v.try_to::<bool>().map_err(|_| fail()),
        None => Err(fail()),
    }
}

fn get_dict(d: &Dictionary, key: &str) -> Result<Dictionary, Status> {
    match d.get(key) {
        Some(v) => v.try_to::<Dictionary>().map_err(|_| fail()),
        None => Err(fail()),
    }
}

fn get_array(d: &Dictionary, key: &str) -> Result<Array, Status> {
    match d.get(key) {
        Some(v) => v.try_to::<Array>().map_err(|_| fail()),
        None => Err(fail()),
    }
}

fn extract_floats(v: &Variant) -> Result<Vec<f32>, Status> {
    if let Ok(packed) = v.try_to::<PackedFloat32Array>() {
        let mut out = Vec::with_capacity(packed.len());
        for i in 0..packed.len() {
            out.push(packed[i]);
        }
        return Ok(out);
    }
    if let Ok(arr) = v.try_to::<Array>() {
        let mut out = Vec::with_capacity(arr.len());
        for i in 0..arr.len() {
            let elem = arr.at(i);
            if let Ok(f) = elem.try_to::<f64>() {
                out.push(f as f32);
            } else if let Ok(i) = elem.try_to::<i64>() {
                out.push(i as f32);
            } else {
                return Err(fail());
            }
        }
        return Ok(out);
    }
    Err(fail())
}

pub fn appearance_from_dict(d: &Dictionary) -> Result<Appearance, Status> {
    let opacity = get_f32(d, "opacity")?;
    let mut app = Appearance {
        opacity,
        ..Default::default()
    };
    for key in &["multiply", "screen"] {
        let val = match d.get(*key) {
            Some(v) => v,
            None => return Err(fail()),
        };
        let floats = extract_floats(&val)?;
        if floats.len() != 3 {
            return Err(fail());
        }
        if *key == "multiply" {
            app.multiply = [floats[0], floats[1], floats[2]];
        } else {
            app.screen = [floats[0], floats[1], floats[2]];
        }
    }
    Ok(app)
}

pub fn dict_from_appearance(a: &Appearance) -> Dictionary {
    let mut d = Dictionary::new();
    d.set("opacity", a.opacity);
    let mut mul = Array::new();
    mul.push(a.multiply[0]);
    mul.push(a.multiply[1]);
    mul.push(a.multiply[2]);
    d.set("multiply", &mul);
    let mut scr = Array::new();
    scr.push(a.screen[0]);
    scr.push(a.screen[1]);
    scr.push(a.screen[2]);
    d.set("screen", &scr);
    d
}

pub fn pose_from_dict(d: &Dictionary) -> Result<RotationPose, Status> {
    let angle = get_f32(d, "angle")?;
    let scale = get_f32(d, "scale")?;
    let reflect_x = get_bool(d, "reflect_x")?;
    let reflect_y = get_bool(d, "reflect_y")?;
    let origin_val = match d.get("origin") {
        Some(v) => v,
        None => return Err(fail()),
    };
    let xy: Vec<f64> = if let Ok(array) = origin_val.try_to::<Array>() {
        array.iter_shared().map(|v| match v.get_type() {
            VariantType::FLOAT => Ok(v.to::<f64>()),
            VariantType::INT => Ok(v.to::<i64>() as f64),
            _ => Err(fail()),
        }).collect::<Result<_, _>>()?
    } else if let Ok(point) = origin_val.try_to::<Vector2>() {
        vec![point.x as f64, point.y as f64]
    } else { extract_floats(&origin_val)?.into_iter().map(f64::from).collect() };
    if xy.len() != 2 { return Err(fail()); }
    Ok(RotationPose {
        origin: kasane_core::types::PreciseVec2::new(xy[0], xy[1]),
        angle,
        scale,
        reflect_x,
        reflect_y,
    })
}

pub fn dict_from_pose(p: &RotationPose) -> Dictionary {
    let mut d = Dictionary::new();
    let mut origin = Array::new();
    origin.push(p.origin.x);
    origin.push(p.origin.y);
    d.set("origin", &origin);
    d.set("angle", p.angle);
    d.set("scale", p.scale);
    d.set("reflect_x", p.reflect_x);
    d.set("reflect_y", p.reflect_y);
    d
}

pub fn parameter_from_dict(d: &Dictionary) -> Result<Parameter, Status> {
    let id = get_str(d, "id")?;
    let runtime_id = get_str(d, "runtime_id")?;
    let name = get_str(d, "name")?;
    let minimum = get_f32(d, "minimum")?;
    let maximum = get_f32(d, "maximum")?;
    let default_value = get_f32(d, "default_value")?;
    let decimal_places = if d.contains_key("decimal_places") {
        let p = get_f32(d, "decimal_places")?;
        if !p.is_finite() || p.floor() != p || p < 0.0 || p > 9.0 {
            return Err(fail());
        }
        p as i32
    } else {
        6
    };
    let kind = if !d.contains_key("kind") {
        kasane_core::types::ParameterKind::Normal
    } else {
        match get_str(d, "kind")?.as_str() {
            "blend_shape" => kasane_core::types::ParameterKind::BlendShape,
            "normal" => kasane_core::types::ParameterKind::Normal,
            _ => return Err(Status::error("INVALID_PARAMETER_KIND", "Expected normal or blend_shape")),
        }
    };
    let repeat = if d.contains_key("repeat") {
        get_bool(d, "repeat")?
    } else {
        false
    };
    Ok(Parameter {
        id,
        runtime_id,
        name,
        minimum,
        maximum,
        default_value,
        decimal_places,
        kind,
        repeat,
    })
}

pub fn dict_from_parameter(p: &Parameter) -> Dictionary {
    let mut d = Dictionary::new();
    d.set("id", p.id.as_str());
    d.set("runtime_id", p.runtime_id.as_str());
    d.set("name", p.name.as_str());
    d.set("minimum", p.minimum);
    d.set("maximum", p.maximum);
    d.set("default_value", p.default_value);
    d.set("decimal_places", p.decimal_places);
    d.set(
        "kind",
        match p.kind {
            kasane_core::types::ParameterKind::Normal => "normal",
            kasane_core::types::ParameterKind::BlendShape => "blend_shape",
        },
    );
    d.set("repeat", p.repeat);
    d
}

pub fn binding_from_dict(d: &Dictionary) -> Result<MeshBinding, Status> {
    let id = get_str(d, "id")?;
    let mesh_id = get_str(d, "mesh_id")?;
    let raw_axes = get_array(d, "axes")?;
    let raw_forms = get_array(d, "keyforms")?;

    let mut axes = Vec::with_capacity(raw_axes.len());
    for i in 0..raw_axes.len() {
        let a = raw_axes.at(i).try_to::<Dictionary>().map_err(|_| fail())?;
        let parameter_id = get_str(&a, "parameter_id")?;
        let keys_val = match a.get("keys") {
            Some(v) => v,
            None => return Err(fail()),
        };
        let keys = extract_floats(&keys_val)?;
        axes.push(BindingAxis { parameter_id, keys });
    }

    let mut keyforms = Vec::with_capacity(raw_forms.len());
    for i in 0..raw_forms.len() {
        let f = raw_forms.at(i).try_to::<Dictionary>().map_err(|_| fail())?;
        let keys_val = match f.get("keys") {
            Some(v) => v,
            None => return Err(fail()),
        };
        let keys = extract_floats(&keys_val)?;
        let pos_val = match f.get("positions") {
            Some(v) => v,
            None => return Err(fail()),
        };
        let positions = if pos_val.get_type() == VariantType::PACKED_VECTOR2_ARRAY {
            packed_to_vectors(&pos_val.to::<PackedVector2Array>())
        } else if let Ok(arr) = pos_val.try_to::<Array>() {
            let mut pts = Vec::with_capacity(arr.len());
            for j in 0..arr.len() {
                let p_val = arr.at(j);
                let xy = extract_floats(&p_val)?;
                if xy.len() != 2 {
                    return Err(fail());
                }
                pts.push(Vec2::new(xy[0], xy[1]));
            }
            pts
        } else {
            return Err(fail());
        };

        let appearance = if f.contains_key("appearance") {
            let app_d = get_dict(&f, "appearance")?;
            appearance_from_dict(&app_d)?
        } else {
            Appearance::default()
        };

        let draw_order = if f.contains_key("draw_order") {
            Some(get_f32(&f, "draw_order")?)
        } else {
            None
        };

        keyforms.push(MeshKeyform {
            keys,
            positions,
            appearance,
            draw_order,
        });
    }

    Ok(MeshBinding {
        id,
        mesh_id,
        axes,
        keyforms,
    })
}

pub fn dict_from_binding(b: &MeshBinding) -> Dictionary {
    let mut d = Dictionary::new();
    d.set("id", b.id.as_str());
    d.set("mesh_id", b.mesh_id.as_str());

    let mut axes = Array::new();
    for a in &b.axes {
        let mut axis = Dictionary::new();
        axis.set("parameter_id", a.parameter_id.as_str());
        let mut keys = Array::new();
        for &k in &a.keys {
            keys.push(k);
        }
        axis.set("keys", &keys);
        axes.push(&axis);
    }
    d.set("axes", &axes);

    let mut forms = Array::new();
    for f in &b.keyforms {
        let mut form = Dictionary::new();
        let mut keys = Array::new();
        for &k in &f.keys {
            keys.push(k);
        }
        form.set("keys", &keys);

        let mut points = Array::new();
        for p in &f.positions {
            let mut pt = Array::new();
            pt.push(p.x);
            pt.push(p.y);
            points.push(&pt);
        }
        form.set("positions", &points);
        let app_dict = dict_from_appearance(&f.appearance);
        form.set("appearance", &app_dict);
        if let Some(order) = f.draw_order {
            form.set("draw_order", order);
        }
        forms.push(&form);
    }
    d.set("keyforms", &forms);
    d
}

pub fn mesh_properties_from_dict(d: &Dictionary, m: &mut Mesh) -> Result<(), Status> {
    m.part_id = get_str(d, "part_id")?;
    m.deformer_id = get_str(d, "deformer_id")?;
    let blend_val = get_f32(d, "blend_mode")?;
    if !blend_val.is_finite()
        || blend_val < 0.0
        || blend_val > 2.0
        || blend_val.floor() != blend_val
    {
        return Err(fail());
    }
    m.blend_mode = match blend_val as i32 {
        0 => BlendMode::Normal,
        1 => BlendMode::Additive,
        2 => BlendMode::Multiplicative,
        _ => return Err(fail()),
    };
    m.enabled = get_bool(d, "enabled")?;
    m.double_sided = get_bool(d, "double_sided")?;
    m.inverted_mask = get_bool(d, "inverted_mask")?;

    let app_dict = get_dict(d, "appearance")?;
    m.appearance = appearance_from_dict(&app_dict)?;

    if d.contains_key("draw_order") {
        m.draw_order = Some(get_f32(d, "draw_order")?);
    }

    if d.contains_key("masks") {
        let raw_masks = get_array(d, "masks")?;
        m.masks.clear();
        for i in 0..raw_masks.len() {
            let mask_id = raw_masks
                .at(i)
                .try_to::<GString>()
                .map(|s| s.to_string())
                .map_err(|_| fail())?;
            m.masks.push(mask_id);
        }
    }
    Ok(())
}

pub fn dict_from_mesh_properties(m: &Mesh) -> Dictionary {
    let mut d = Dictionary::new();
    d.set("part_id", m.part_id.as_str());
    d.set("deformer_id", m.deformer_id.as_str());
    let app_dict = dict_from_appearance(&m.appearance);
    d.set("appearance", &app_dict);
    if let Some(order) = m.draw_order {
        d.set("draw_order", order);
    }
    let blend_int = match m.blend_mode {
        BlendMode::Normal => 0,
        BlendMode::Additive => 1,
        BlendMode::Multiplicative => 2,
    };
    d.set("blend_mode", blend_int);
    d.set("enabled", m.enabled);
    d.set("double_sided", m.double_sided);
    d.set("inverted_mask", m.inverted_mask);
    let mut masks = Array::new();
    for id in &m.masks {
        masks.push(&GString::from(id.as_str()));
    }
    d.set("masks", &masks);
    d
}

pub fn part_from_dict(d: &Dictionary) -> Result<Part, Status> {
    let id = get_str(d, "id")?;
    let runtime_id = get_str(d, "runtime_id")?;
    let name = get_str(d, "name")?;
    let parent_id = get_str(d, "parent_id")?;
    let enabled = get_bool(d, "enabled")?;
    let draw_order = get_f32(d, "draw_order")?;
    Ok(Part {
        id,
        runtime_id,
        name,
        parent_id,
        enabled,
        draw_order,
    })
}

pub fn dict_from_part(p: &Part) -> Dictionary {
    let mut d = Dictionary::new();
    d.set("id", p.id.as_str());
    d.set("runtime_id", p.runtime_id.as_str());
    d.set("name", p.name.as_str());
    d.set("parent_id", p.parent_id.as_str());
    d.set("enabled", p.enabled);
    d.set("draw_order", p.draw_order);
    d
}

pub fn transform_from_dict(d: &Dictionary) -> Result<Transform, Status> {
    let id = get_str(d, "id")?;
    let runtime_id = get_str(d, "runtime_id")?;
    let name = get_str(d, "name")?;
    let part_id = get_str(d, "part_id")?;
    let parent_id = get_str(d, "parent_id")?;
    let kind_val = get_f32(d, "kind")?;
    if !kind_val.is_finite() || (kind_val != 0.0 && kind_val != 1.0) {
        return Err(fail());
    }
    let kind = if kind_val == 0.0 {
        TransformKind::Warp
    } else {
        TransformKind::Rotation
    };
    let base_angle = get_f32(d, "base_angle")?;
    let rows_val = get_f32(d, "rows")?;
    let cols_val = get_f32(d, "columns")?;
    if !rows_val.is_finite() || !cols_val.is_finite() || rows_val < 0.0 || cols_val < 0.0 {
        return Err(fail());
    }
    let rows = rows_val as usize;
    let columns = cols_val as usize;
    let quad = get_bool(d, "quad")?;
    let enabled = get_bool(d, "enabled")?;
    let rot_d = get_dict(d, "rotation")?;
    let rotation = pose_from_dict(&rot_d)?;
    let app_d = get_dict(d, "appearance")?;
    let appearance = appearance_from_dict(&app_d)?;

    let raw_pts = get_array(d, "points")?;
    let mut points = Vec::with_capacity(raw_pts.len());
    for i in 0..raw_pts.len() {
        let pair_val = raw_pts.at(i);
        let xy = extract_floats(&pair_val)?;
        if xy.len() != 2 {
            return Err(fail());
        }
        points.push(Vec2::new(xy[0], xy[1]));
    }

    Ok(Transform {
        id,
        runtime_id,
        name,
        part_id,
        parent_id,
        kind,
        base_angle,
        rotation,
        rows: rows as u32,
        columns: columns as u32,
        quad,
        enabled,
        points,
        appearance,
    })
}

pub fn dict_from_transform(t: &Transform) -> Dictionary {
    let mut d = Dictionary::new();
    d.set("id", t.id.as_str());
    d.set("runtime_id", t.runtime_id.as_str());
    d.set("name", t.name.as_str());
    d.set("part_id", t.part_id.as_str());
    d.set("parent_id", t.parent_id.as_str());
    let kind_int = match t.kind {
        TransformKind::Warp => 0,
        TransformKind::Rotation => 1,
    };
    d.set("kind", kind_int);
    d.set("base_angle", t.base_angle);
    let rot_d = dict_from_pose(&t.rotation);
    d.set("rotation", &rot_d);
    d.set("rows", t.rows as i64);
    d.set("columns", t.columns as i64);
    d.set("quad", t.quad);
    d.set("enabled", t.enabled);
    let mut pts = Array::new();
    for p in &t.points {
        let mut pt = Array::new();
        pt.push(p.x);
        pt.push(p.y);
        pts.push(&pt);
    }
    d.set("points", &pts);
    let app_d = dict_from_appearance(&t.appearance);
    d.set("appearance", &app_d);
    d
}

pub fn scene_binding_from_dict(d: &Dictionary) -> Result<SceneBinding, Status> {
    let target_id = get_str(d, "target_id")?;
    let mut copy = d.clone();
    copy.set("mesh_id", target_id.as_str());
    let mesh_b = binding_from_dict(&copy)?;
    let raw_forms = get_array(d, "keyforms")?;

    let mut keyforms = Vec::with_capacity(mesh_b.keyforms.len());
    for i in 0..mesh_b.keyforms.len() {
        let m = &mesh_b.keyforms[i];
        let f_dict = raw_forms.at(i).try_to::<Dictionary>().map_err(|_| fail())?;
        let rot_dict = get_dict(&f_dict, "rotation")?;
        let rotation = pose_from_dict(&rot_dict)?;
        keyforms.push(SceneKeyform {
            keys: m.keys.clone(),
            positions: m.positions.clone(),
            appearance: m.appearance,
            draw_order: m.draw_order.unwrap_or(0.0),
            rotation,
        });
    }

    Ok(SceneBinding {
        id: mesh_b.id,
        target_id,
        axes: mesh_b.axes,
        keyforms,
    })
}

pub fn dict_from_scene_binding(b: &SceneBinding) -> Dictionary {
    let mesh = MeshBinding {
        id: b.id.clone(),
        mesh_id: b.target_id.clone(),
        axes: b.axes.clone(),
        keyforms: b
            .keyforms
            .iter()
            .map(|f| MeshKeyform {
                keys: f.keys.clone(),
                positions: f.positions.clone(),
                appearance: f.appearance,
                draw_order: Some(f.draw_order),
            })
            .collect(),
    };
    let mut d = dict_from_binding(&mesh);
    d.remove("mesh_id");
    d.set("target_id", b.target_id.as_str());
    let raw_forms = d.get("keyforms").unwrap().to::<Array>();
    let mut new_forms = Array::new();
    for i in 0..raw_forms.len() {
        let mut f = raw_forms.at(i).to::<Dictionary>();
        let rot_d = dict_from_pose(&b.keyforms[i].rotation);
        f.set("rotation", &rot_d);
        new_forms.push(&f);
    }
    d.set("keyforms", &new_forms);
    d
}

pub fn project_result_dict(res: &ProjectResult) -> Dictionary {
    let mut out = status_to_dict(&res.status);
    let mut diagnostics = Array::new();
    for diag in &res.diagnostics {
        let mut d = Dictionary::new();
        d.set("asset_id", diag.asset_id.as_str());
        d.set("code", diag.code.as_str());
        d.set("message", diag.message.as_str());
        diagnostics.push(&d);
    }
    out.set("diagnostics", &diagnostics);
    out.set("resources_complete", res.resources_complete());
    out.set("published", res.published);
    out.set("durable", res.durable);
    let mut warnings = Array::new();
    for w in &res.warnings {
        warnings.push(&GString::from(w.as_str()));
    }
    out.set("warnings", &warnings);
    out
}

// Structured Dictionaries for the M3B data model. No JSON text crosses the API;
// integer IDs stay integers and Vector2 inputs retain the usual Godot convention.
pub fn structured_from_dict<T: serde::de::DeserializeOwned>(d: &Dictionary) -> Result<T, Status> {
    fn value(v: &Variant) -> Result<serde_json::Value, Status> {
        use serde_json::{Map, Number, Value};
        Ok(match v.get_type() {
            VariantType::NIL => Value::Null,
            VariantType::BOOL => Value::Bool(v.to::<bool>()),
            VariantType::INT => Value::Number(v.to::<i64>().into()),
            VariantType::FLOAT => Value::Number(Number::from_f64(v.to::<f64>()).ok_or_else(fail)?),
            VariantType::STRING => Value::String(v.to::<GString>().to_string()),
            VariantType::STRING_NAME => Value::String(v.to::<StringName>().to_string()),
            VariantType::VECTOR2 => {
                let p = v.to::<Vector2>();
                if !p.is_finite() {
                    return Err(fail());
                }
                serde_json::json!({"x": p.x, "y": p.y})
            }
            VariantType::DICTIONARY => {
                let d = v.to::<Dictionary>();
                let mut object = Map::new();
                for (k, v) in d.iter_shared() {
                    let key = match k.get_type() {
                        VariantType::STRING => k.to::<GString>().to_string(),
                        VariantType::STRING_NAME => k.to::<StringName>().to_string(),
                        _ => {
                            return Err(Status::error(
                                "INVALID_FIELD",
                                "Dictionary keys must be strings",
                            ))
                        }
                    };
                    let converted = value(&v)
                        .map_err(|s| Status::error(s.code, format!("{key}: {}", s.message)))?;
                    object.insert(key, converted);
                }
                Value::Object(object)
            }
            VariantType::ARRAY => Value::Array(
                v.to::<Array>()
                    .iter_shared()
                    .map(|v| value(&v))
                    .collect::<Result<_, _>>()?,
            ),
            VariantType::PACKED_FLOAT32_ARRAY => Value::Array(
                v.to::<PackedFloat32Array>()
                    .as_slice()
                    .iter()
                    .map(|&v| value(&(v as f64).to_variant()))
                    .collect::<Result<_, _>>()?,
            ),
            VariantType::PACKED_INT64_ARRAY => Value::Array(
                v.to::<PackedInt64Array>()
                    .as_slice()
                    .iter()
                    .map(|&v| Value::Number(v.into()))
                    .collect(),
            ),
            VariantType::PACKED_VECTOR2_ARRAY => Value::Array(
                v.to::<PackedVector2Array>()
                    .as_slice()
                    .iter()
                    .map(|v| value(&v.to_variant()))
                    .collect::<Result<_, _>>()?,
            ),
            VariantType::PACKED_STRING_ARRAY => Value::Array(
                v.to::<PackedStringArray>()
                    .as_slice()
                    .iter()
                    .map(|v| Value::String(v.to_string()))
                    .collect(),
            ),
            _ => {
                return Err(Status::error(
                    "INVALID_FIELD",
                    format!("Unsupported field type: {:?}", v.get_type()),
                ))
            }
        })
    }
    serde_json::from_value(value(&d.to_variant())?)
        .map_err(|e| Status::error("INVALID_FIELD", e.to_string()))
}

pub fn structured_to_dict<T: serde::Serialize>(data: &T) -> Dictionary {
    fn variant(v: serde_json::Value) -> Variant {
        use serde_json::Value;
        match v {
            Value::Null => Variant::nil(),
            Value::Bool(v) => v.to_variant(),
            Value::Number(v) => {
                if let Some(i) = v.as_i64() {
                    i.to_variant()
                } else {
                    v.as_f64().unwrap().to_variant()
                }
            }
            Value::String(v) => GString::from(&v).to_variant(),
            Value::Array(v) => {
                let mut a = Array::new();
                for item in v {
                    a.push(&variant(item));
                }
                a.to_variant()
            }
            Value::Object(v) => {
                let mut d = Dictionary::new();
                for (key, val) in v {
                    d.set(key.as_str(), &variant(val));
                }
                d.to_variant()
            }
        }
    }
    variant(serde_json::to_value(data).expect("validated document is serializable"))
        .to::<Dictionary>()
}
