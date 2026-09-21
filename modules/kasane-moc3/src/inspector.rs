use std::os::raw::{c_int, c_uint, c_void};
use kasane_core::types::Status;

#[cfg(has_purism_core)]
extern "C" {
    fn csmHasMocConsistency(address: *mut c_void, size: c_uint) -> c_int;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Moc3Version {
    Version33,
    Version50,
    Other(u8),
}

impl Moc3Version {
    pub fn from_u8(v: u8) -> Self {
        match v {
            2 => Moc3Version::Version33,
            5 => Moc3Version::Version50,
            other => Moc3Version::Other(other),
        }
    }

    pub fn to_u8(self) -> u8 {
        match self {
            Moc3Version::Version33 => 2,
            Moc3Version::Version50 => 5,
            Moc3Version::Other(v) => v,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct CanvasInfo {
    pub pixels_per_unit: f32,
    pub origin_x: f32,
    pub origin_y: f32,
    pub width: f32,
    pub height: f32,
    pub flag: u8,
}

#[derive(Debug, Clone, Default)]
pub struct ModelCounts {
    pub parts: i32,
    pub deformers: i32,
    pub warps: i32,
    pub rotations: i32,
    pub art_meshes: i32,
    pub parameters: i32,
    pub part_keyforms: i32,
    pub warp_keyforms: i32,
    pub rotation_keyforms: i32,
    pub art_mesh_keyforms: i32,
    pub keyform_pos: i32,
    pub key_table_idx: i32,
    pub bindings: i32,
    pub key_tables: i32,
    pub keys: i32,
    pub uvs: i32,
    pub idx: i32,
    pub masks: i32,
    pub draw_groups: i32,
    pub draw_items: i32,
    pub glues: i32,
    pub glue_info: i32,
    pub glue_keyforms: i32,
    pub keyform_mul_colors: i32,
    pub keyform_scr_colors: i32,
    pub blend_key_tables: i32,
    pub blend_bindings: i32,
    pub bs_warps: i32,
    pub bs_art_meshes: i32,
    pub bs_constraint_idx: i32,
    pub bs_constraints: i32,
    pub bs_constraint_vals: i32,
    pub bs_parts: i32,
    pub bs_rotations: i32,
    pub bs_glues: i32,
    pub offscreens: i32,
    pub offscreen_keyforms: i32,
    pub bs_offscreens: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnsupportedFeature {
    pub category: String,
    pub count: usize,
    pub detail: String,
}

#[derive(Debug, Clone)]
pub struct Moc3InspectionReport {
    pub version: Moc3Version,
    pub version_number: u8,
    pub endian_flag: u8,
    pub counts: ModelCounts,
    pub canvas: CanvasInfo,
    pub section_offsets: Vec<u32>,
    pub unsupported_features: Vec<UnsupportedFeature>,
}

fn read_i32(buf: &[u8], offset: usize) -> Result<i32, Status> {
    if offset + 4 > buf.len() {
        return Err(Status::error(
            "TRUNCATED_BUFFER",
            format!("Buffer truncated reading i32 at {offset}"),
        ));
    }
    Ok(i32::from_le_bytes(buf[offset..offset + 4].try_into().unwrap()))
}

fn read_u32(buf: &[u8], offset: usize) -> Result<u32, Status> {
    if offset + 4 > buf.len() {
        return Err(Status::error(
            "TRUNCATED_BUFFER",
            format!("Buffer truncated reading u32 at {offset}"),
        ));
    }
    Ok(u32::from_le_bytes(buf[offset..offset + 4].try_into().unwrap()))
}

fn read_f32(buf: &[u8], offset: usize) -> Result<f32, Status> {
    if offset + 4 > buf.len() {
        return Err(Status::error(
            "TRUNCATED_BUFFER",
            format!("Buffer truncated reading f32 at {offset}"),
        ));
    }
    Ok(f32::from_le_bytes(buf[offset..offset + 4].try_into().unwrap()))
}

fn inspect_moc3_internal(bytes: &[u8]) -> Result<Moc3InspectionReport, Status> {
    if bytes.len() < 64 {
        return Err(Status::error(
            "BUFFER_TOO_SMALL",
            format!("MOC3 file size {} is smaller than 64-byte header", bytes.len()),
        ));
    }

    if &bytes[0..4] != b"MOC3" {
        return Err(Status::error(
            "INVALID_MAGIC",
            "MOC3 file magic header mismatch (expected b'MOC3')",
        ));
    }

    let version_raw = bytes[4];
    let endian_flag = bytes[5];

    if endian_flag != 0 {
        return Err(Status::error(
            "UNSUPPORTED_ENDIAN",
            format!("Big-endian MOC3 files are not supported (endian_flag={endian_flag})"),
        ));
    }

    match version_raw {
        2 | 5 => {}
        other => {
            return Err(Status::error(
                "UNSUPPORTED_VERSION",
                format!(
                    "MOC3 version {other} is not supported (only csmMocVersion_33 [2] and csmMocVersion_50 [5] are accepted)"
                ),
            ));
        }
    }

    let version = Moc3Version::from_u8(version_raw);

    let offset_count = 160;
    let header_size = 64 + offset_count * 4;
    if bytes.len() < header_size {
        return Err(Status::error(
            "BUFFER_TOO_SMALL",
            format!(
                "MOC3 buffer size {} too small for header and 160 offsets ({header_size})",
                bytes.len()
            ),
        ));
    }

    let mut section_offsets = Vec::with_capacity(offset_count);
    for i in 0..offset_count {
        let off = read_u32(bytes, 64 + i * 4)?;
        section_offsets.push(off);
    }

    let counts_off = section_offsets[0] as usize;
    let count_ints = if version_raw >= 5 { 64 } else { 32 };
    let count_bytes = count_ints * 4;
    if counts_off + count_bytes > bytes.len() {
        return Err(Status::error(
            "SECTION_OOB",
            format!("count_info section at {counts_off} exceeds buffer size {}", bytes.len()),
        ));
    }

    let mut counts_raw = vec![0i32; count_ints];
    for i in 0..count_ints {
        counts_raw[i] = read_i32(bytes, counts_off + i * 4)?;
        if counts_raw[i] < 0 {
            return Err(Status::error(
                "FILE_CORRUPT",
                format!("count_info[{i}] has negative count {}", counts_raw[i]),
            ));
        }
    }

    let mut counts = ModelCounts {
        parts: counts_raw[0],
        deformers: counts_raw[1],
        warps: counts_raw[2],
        rotations: counts_raw[3],
        art_meshes: counts_raw[4],
        parameters: counts_raw[5],
        part_keyforms: counts_raw[6],
        warp_keyforms: counts_raw[7],
        rotation_keyforms: counts_raw[8],
        art_mesh_keyforms: counts_raw[9],
        keyform_pos: counts_raw[10],
        key_table_idx: counts_raw[11],
        bindings: counts_raw[12],
        key_tables: counts_raw[13],
        keys: counts_raw[14],
        uvs: counts_raw[15],
        idx: counts_raw[16],
        masks: counts_raw[17],
        draw_groups: counts_raw[18],
        draw_items: counts_raw[19],
        glues: counts_raw[20],
        glue_info: counts_raw[21],
        glue_keyforms: counts_raw[22],
        keyform_mul_colors: counts_raw[23],
        keyform_scr_colors: counts_raw[24],
        blend_key_tables: counts_raw[25],
        blend_bindings: counts_raw[26],
        bs_warps: counts_raw[27],
        bs_art_meshes: counts_raw[28],
        bs_constraint_idx: counts_raw[29],
        bs_constraints: counts_raw[30],
        bs_constraint_vals: counts_raw[31],
        ..Default::default()
    };

    if version_raw >= 5 {
        counts.bs_parts = counts_raw[32];
        counts.bs_rotations = counts_raw[33];
        counts.bs_glues = counts_raw[34];
        counts.offscreens = counts_raw[35];
        counts.offscreen_keyforms = counts_raw[36];
        counts.bs_offscreens = counts_raw[37];
    }

    // Verify deformer arithmetic
    if i64::from(counts.warps) + i64::from(counts.rotations) != i64::from(counts.deformers) {
        return Err(Status::error(
            "FILE_CORRUPT",
            format!(
                "Deformer count mismatch: warps ({}) + rotations ({}) != deformers ({})",
                counts.warps, counts.rotations, counts.deformers
            ),
        ));
    }

    // Read canvas info (section 1) before feature checking
    let canvas_off = section_offsets[1] as usize;
    if canvas_off + 24 > bytes.len() {
        return Err(Status::error(
            "SECTION_OOB",
            format!("canvas_info section at {canvas_off} exceeds buffer size {}", bytes.len()),
        ));
    }

    let ppu = read_f32(bytes, canvas_off)?;
    let ox = read_f32(bytes, canvas_off + 4)?;
    let oy = read_f32(bytes, canvas_off + 8)?;
    let width = read_f32(bytes, canvas_off + 12)?;
    let height = read_f32(bytes, canvas_off + 16)?;
    let flag = bytes[canvas_off + 20];

    if !ppu.is_finite() || ppu <= 0.0 || !ox.is_finite() || !oy.is_finite() || !width.is_finite() || !height.is_finite() {
        return Err(Status::error(
            "NON_FINITE",
            "canvas_info contains non-finite or non-positive float values",
        ));
    }

    // Collect all unsupported features in a single pass without failing on the first one
    let mut unsupported_features = Vec::new();

    // 1. BlendShape Glue (Mao has 0; out of scope for M3B)
    if counts.bs_glues > 0 {
        unsupported_features.push(UnsupportedFeature {
            category: "blend_shape_glue".into(),
            count: counts.bs_glues as usize,
            detail: format!(
                "Model contains {} BlendShape Glues (BlendShape Glue is out of scope for this milestone)",
                counts.bs_glues
            ),
        });
    }

    // 2. Offscreen (offscreens, offscreen_keyforms, bs_offscreens)
    let offscreen_total = i64::from(counts.offscreens) + i64::from(counts.offscreen_keyforms) + i64::from(counts.bs_offscreens);
    if offscreen_total > 0 {
        unsupported_features.push(UnsupportedFeature {
            category: "offscreen".into(),
            count: offscreen_total as usize,
            detail: format!(
                "Model contains Offscreen features (offscreens={}, offscreen_keyforms={}, bs_offscreens={}; Offscreen is out of scope for this milestone)",
                counts.offscreens, counts.offscreen_keyforms, counts.bs_offscreens
            ),
        });
    }

    // 3. Cyclic parameter check (param_src.repeat)
    // Section 54 is param_src.repeat
    if counts.parameters > 0 && section_offsets.len() > 54 {
        let repeat_off = section_offsets[54] as usize;
        let id_off = section_offsets[50] as usize;
        if repeat_off + counts.parameters as usize * 4 <= bytes.len() {
            let mut cyclic_params = Vec::new();
            for p in 0..counts.parameters as usize {
                let rep = read_i32(bytes, repeat_off + p * 4)?;
                if rep != 0 {
                    let mut param_id = format!("Param{p}");
                    if id_off + (p + 1) * 64 <= bytes.len() {
                        let id_slice = &bytes[id_off + p * 64..id_off + (p + 1) * 64];
                        let len = id_slice.iter().position(|&c| c == 0).unwrap_or(64);
                        if let Ok(s) = std::str::from_utf8(&id_slice[..len]) {
                            param_id = s.to_string();
                        }
                    }
                    cyclic_params.push(param_id);
                }
            }
            if !cyclic_params.is_empty() {
                unsupported_features.push(UnsupportedFeature {
                    category: "cyclic_parameter".into(),
                    count: cyclic_params.len(),
                    detail: format!(
                        "Parameter(s) with repeat enabled: {} (cyclic parameters are out of scope for this milestone)",
                        cyclic_params.join(", ")
                    ),
                });
            }
        }
    }

    #[cfg(has_purism_core)]
    {
        let mut buffer_copy = bytes.to_vec();
        let r = unsafe {
            csmHasMocConsistency(
                buffer_copy.as_mut_ptr() as *mut c_void,
                buffer_copy.len() as c_uint,
            )
        };
        if r != 1 {
            return Err(Status::error(
                "FILE_CORRUPT",
                "PurismCore consistency check failed (moc3 is corrupt or inconsistent)",
            ));
        }
    }

    // Feature checks that traverse references run only after consistency validation.
    if version_raw >= 5 {
        for p in 0..counts.parameters as usize {
            let kind = read_i32(bytes, section_offsets[114] as usize + p * 4)?;
            if kind != 0 && kind != 1 {
                unsupported_features.push(UnsupportedFeature {
                    category: "parameter_type".into(), count: 1,
                    detail: format!("Parameter {p}: unknown type {kind}"),
                });
            }
        }
    }

    Ok(Moc3InspectionReport {
        version,
        version_number: version_raw,
        endian_flag,
        counts,
        canvas: CanvasInfo {
            pixels_per_unit: ppu,
            origin_x: ox,
            origin_y: oy,
            width,
            height,
            flag,
        },
        section_offsets,
        unsupported_features,
    })
}

pub fn inspect_moc3_safety(bytes: &[u8]) -> Result<Moc3InspectionReport, Status> {
    inspect_moc3_internal(bytes)
}

pub fn inspect_moc3(bytes: &[u8]) -> Result<Moc3InspectionReport, Status> {
    let report = inspect_moc3_safety(bytes)?;
    if !report.unsupported_features.is_empty() {
        let details: Vec<String> = report
            .unsupported_features
            .iter()
            .map(|u| format!("[{}] {}", u.category, u.detail))
            .collect();
        return Err(Status::error(
            "UNSUPPORTED_FEATURE",
            format!("Model contains unsupported features: {}", details.join("; ")),
        ));
    }
    Ok(report)
}
