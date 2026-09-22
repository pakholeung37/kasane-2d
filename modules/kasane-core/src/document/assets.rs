use super::*;
use crate::types::{ChangeKind, EditResult, ImageAsset, Status};

impl Document {
    pub fn asset_order(&self) -> &[String] {
        &self.asset_order
    }

    pub fn asset_count(&self) -> usize {
        self.assets.len()
    }

    pub fn get_asset(&self, id: &str) -> Option<&ImageAsset> {
        self.assets.get(id)
    }

    pub fn add_asset(&mut self, asset: ImageAsset) -> EditResult {
        if self.mutation_blocked() {
            return self.failed(Status::error(
                "TRANSACTION_ACTIVE",
                "Commit or cancel the active transaction first.",
            ));
        }
        if !self.initialized() {
            return self.failed(Status::error(
                "NOT_INITIALIZED",
                "Initialize Document first.",
            ));
        }
        if !valid_uuid(&asset.id) {
            return self.failed(Status::error(
                "INVALID_ID",
                "Asset ID must be a canonical UUID.",
            ));
        }
        if self.contains_id(&asset.id) {
            return self.failed(Status::error("DUPLICATE_ID", "Object ID already exists."));
        }
        if asset.width == 0 || asset.height == 0 || asset.source.is_empty() {
            return self.failed(Status::error(
                "INVALID_ASSET",
                "Asset requires source and positive pixel dimensions.",
            ));
        }
        let key = asset.id.clone();
        self.assets.insert(key.clone(), asset);
        self.asset_order.push(key.clone());
        self.changed(ChangeKind::Structure, Vec::new(), vec![key])
    }

    pub fn replace_asset(&mut self, asset: ImageAsset) -> EditResult {
        if self.mutation_blocked() {
            return self.failed(Status::error("TRANSACTION_ACTIVE", &asset.id));
        }
        if !self.assets.contains_key(&asset.id) {
            return self.failed(Status::error("MISSING_ASSET", &asset.id));
        }
        if asset.width == 0 || asset.height == 0 || asset.source.is_empty() {
            return self.failed(Status::error("INVALID_ASSET", &asset.id));
        }
        let key = asset.id.clone();
        self.assets.insert(key.clone(), asset);
        let meshes = self.mesh_order.clone();
        self.changed(ChangeKind::Resources, meshes, vec![key])
    }
}
