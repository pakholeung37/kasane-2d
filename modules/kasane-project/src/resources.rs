use kasane_core::types::Status;
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AssetData {
    pub bytes: Vec<u8>,
    pub rgba: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub sha256: String,
}

pub fn content_sha256(bytes: &[u8]) -> String {
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

pub fn is_valid_asset_path(s: &str) -> bool {
    if !s.starts_with("assets/") || s.contains('\\') || s.contains(':') || s.contains('\0') {
        return false;
    }
    let p = std::path::Path::new(s);
    if p.is_absolute() || p.file_name().is_none() {
        return false;
    }
    for comp in p.components() {
        match comp {
            std::path::Component::Normal(_) => {}
            _ => return false,
        }
    }
    true
}

pub fn decode_png(bytes: &[u8]) -> Result<AssetData, Status> {
    let image = kasane_core::image::decode_png(bytes)?;
    Ok(AssetData {
        bytes: bytes.to_vec(),
        rgba: image.rgba,
        width: image.width,
        height: image.height,
        sha256: content_sha256(bytes),
    })
}
