//! Binary variants of the independently generated v50 fixture.
pub fn set_i32(bytes: &mut [u8], section: usize, index: usize, value: i32) {
    let offset = u32::from_le_bytes(
        bytes[64 + section * 4..68 + section * 4]
            .try_into()
            .unwrap(),
    ) as usize;
    bytes[offset + index * 4..offset + index * 4 + 4].copy_from_slice(&value.to_le_bytes());
}

pub fn variant(source: &[u8], name: &str) -> Vec<u8> {
    let mut bytes = source.to_vec();
    let offsets = kasane_moc3::inspect_moc3(source).unwrap().section_offsets;
    match name {
        "binding_zero" => {
            for section in [4, 12, 19, 25, 34] {
                for index in 0..if section == 12 { 2 } else { 1 } {
                    let p = offsets[section] as usize + index * 4;
                    let old = i32::from_le_bytes(bytes[p..p + 4].try_into().unwrap());
                    set_i32(&mut bytes, section, index, 1 - old);
                }
            }
            for section in [73, 74] {
                let p = offsets[section] as usize;
                for i in 0..4 {
                    bytes.swap(p + i, p + 4 + i);
                }
            }
        }
        "color_window" => {
            set_i32(&mut bytes, 105, 0, 3);
            set_i32(&mut bytes, 106, 0, 3);
            // Reverse the six-entry pools as well, keeping the independent
            // per-keyform offsets untouched.
            for section in 108..=113 {
                let p = offsets[section] as usize;
                for k in 0..3 {
                    for i in 0..4 {
                        bytes.swap(p + k * 4 + i, p + (5 - k) * 4 + i);
                    }
                }
            }
        }
        "canvas_y" => bytes[offsets[1] as usize + 20] = 0,
        _ => panic!("Unknown fixture variant: {name}"),
    }
    bytes
}
