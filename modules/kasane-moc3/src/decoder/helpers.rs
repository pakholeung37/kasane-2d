use kasane_core::types::Status;
use sha2::{Digest, Sha256};

pub fn texture_slot_uuid(slot: usize) -> String {
    stable_id("kasane", "asset", slot, &format!("texture_{slot}"))
}

pub fn stable_id(doc_prefix: &str, kind: &str, index: usize, runtime_id: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(doc_prefix.as_bytes());
    hasher.update(b":");
    hasher.update(kind.as_bytes());
    hasher.update(b":");
    hasher.update(index.to_string().as_bytes());
    hasher.update(b":");
    hasher.update(runtime_id.as_bytes());
    let hash = hasher.finalize();

    format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        hash[0], hash[1], hash[2], hash[3],
        hash[4], hash[5],
        (hash[6] & 0x0f) | 0x40, hash[7],
        (hash[8] & 0x3f) | 0x80, hash[9],
        hash[10], hash[11], hash[12], hash[13], hash[14], hash[15]
    )
}

pub fn checked_reference<'a, T>(
    table: &'a [T],
    index: usize,
    field: &str,
) -> Result<&'a T, Status> {
    table.get(index).ok_or_else(|| {
        Status::error(
            "INDEX_OUT_OF_BOUNDS",
            format!(
                "{field}: index {index} exceeds table length {}",
                table.len()
            ),
        )
    })
}

pub fn checked_window(start: usize, len: usize, count: usize, field: &str) -> Result<(), Status> {
    if start > count || len > count - start {
        return Err(Status::error(
            "INDEX_OUT_OF_BOUNDS",
            format!("{field}: window {start}+{len} exceeds table length {count}"),
        ));
    }
    Ok(())
}

pub fn read_string(bytes: &[u8], offset: usize, max_len: usize) -> String {
    if offset >= bytes.len() {
        return String::new();
    }
    let end = (offset + max_len).min(bytes.len());
    let slice = &bytes[offset..end];
    let len = slice.iter().position(|&c| c == 0).unwrap_or(slice.len());
    String::from_utf8_lossy(&slice[..len]).trim().to_string()
}

pub fn read_i32(bytes: &[u8], offset: usize) -> Result<i32, Status> {
    if offset + 4 > bytes.len() {
        return Err(Status::error(
            "BUFFER_TRUNCATED",
            format!("Unexpected EOF reading i32 at offset {offset}"),
        ));
    }
    Ok(i32::from_le_bytes(
        bytes[offset..offset + 4].try_into().unwrap(),
    ))
}

pub fn read_u16(bytes: &[u8], offset: usize) -> Result<u16, Status> {
    if offset + 2 > bytes.len() {
        return Err(Status::error(
            "BUFFER_TRUNCATED",
            format!("Unexpected EOF reading u16 at offset {offset}"),
        ));
    }
    Ok(u16::from_le_bytes(
        bytes[offset..offset + 2].try_into().unwrap(),
    ))
}

pub fn read_f32(bytes: &[u8], offset: usize) -> Result<f32, Status> {
    if offset + 4 > bytes.len() {
        return Err(Status::error(
            "BUFFER_TRUNCATED",
            format!("Unexpected EOF reading f32 at offset {offset}"),
        ));
    }
    Ok(f32::from_le_bytes(
        bytes[offset..offset + 4].try_into().unwrap(),
    ))
}

pub fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, Status> {
    if offset + 4 > bytes.len() {
        return Err(Status::error(
            "BUFFER_TRUNCATED",
            format!("Unexpected EOF reading u32 at offset {offset}"),
        ));
    }
    Ok(u32::from_le_bytes(
        bytes[offset..offset + 4].try_into().unwrap(),
    ))
}

// Iterative three-state traversal: reject cycles before Document mutation and
// avoid using the call stack for externally supplied parent chains.
pub fn parent_order(parents: &[i32], kind: &str) -> Result<Vec<usize>, Status> {
    let mut state = vec![0u8; parents.len()];
    let mut order = Vec::with_capacity(parents.len());
    let mut chain = Vec::new();
    for start in 0..parents.len() {
        let mut current = start as i32;
        while current != -1 {
            if current < 0 || current as usize >= parents.len() {
                return Err(Status::error(
                    "INVALID_REFERENCE",
                    format!("{kind}[{start}]: invalid parent {current}"),
                ));
            }
            let index = current as usize;
            match state[index] {
                1 => {
                    return Err(Status::error(
                        "RELATIONSHIP_CYCLE",
                        format!("{kind}[{index}]: parent relationship contains a cycle"),
                    ))
                }
                2 => break,
                _ => {}
            }
            state[index] = 1;
            chain.push(index);
            current = parents[index];
        }
        while let Some(index) = chain.pop() {
            state[index] = 2;
            order.push(index);
        }
    }
    Ok(order)
}
