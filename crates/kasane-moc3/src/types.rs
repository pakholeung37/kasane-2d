use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextureSlot {
    pub asset_id: String,
    pub source: String,
    pub package_path: String,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Moc3Artifact {
    pub bytes: Vec<u8>,
    pub model3_json: String,
    pub textures: Vec<TextureSlot>,
}
