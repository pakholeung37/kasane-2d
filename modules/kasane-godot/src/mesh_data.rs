use godot::obj::InstanceId;
use godot::prelude::*;

use crate::conversions::{error_dict, is_main_thread, Dictionary};
use crate::document_bridge::KasaneDocumentBridge;

#[derive(GodotClass)]
#[class(init, base=RefCounted)]
pub struct KasaneMeshData {
    base: Base<RefCounted>,
    owner_id: u64,
    generation: u64,
    id: GString,
}

#[godot_api]
impl KasaneMeshData {
    pub fn attach(&mut self, owner: u64, generation: u64, id: GString) {
        self.owner_id = owner;
        self.generation = generation;
        self.id = id;
    }

    fn owner(&self) -> Option<Gd<KasaneDocumentBridge>> {
        if !is_main_thread() {
            return None;
        }
        let instance_id = InstanceId::try_from_i64(self.owner_id as i64)?;
        let bridge = Gd::<KasaneDocumentBridge>::try_from_instance_id(instance_id).ok()?;
        if bridge.bind().generation() == self.generation {
            Some(bridge)
        } else {
            None
        }
    }

    #[func]
    pub fn is_valid(&self) -> bool {
        let Some(bridge) = self.owner() else {
            return false;
        };
        let snap = bridge.bind().get_mesh_snapshot(self.id.clone());
        snap.get("ok")
            .and_then(|v| v.try_to::<bool>().ok())
            .unwrap_or(false)
    }

    #[func]
    pub fn get_id(&self) -> GString {
        self.id.clone()
    }

    #[func]
    pub fn snapshot(&self) -> Dictionary {
        let Some(bridge) = self.owner() else {
            return error_dict(
                "STALE_HANDLE",
                "The owning document was closed or replaced.",
            );
        };
        let res = bridge.bind().get_mesh_snapshot(self.id.clone());
        res
    }

    #[func]
    pub fn get_name(&self) -> GString {
        self.snapshot()
            .get("name")
            .and_then(|v| v.try_to::<GString>().ok())
            .unwrap_or_default()
    }

    #[func]
    pub fn set_name(&mut self, value: GString) {
        let Some(mut bridge) = self.owner() else {
            godot_error!("The owning document was closed or replaced.");
            return;
        };
        let edit = bridge.bind_mut().rename_mesh(self.id.clone(), value);
        let ok = edit
            .get("ok")
            .and_then(|v| v.try_to::<bool>().ok())
            .unwrap_or(false);
        if !ok {
            let msg = edit
                .get("message")
                .and_then(|v| v.try_to::<GString>().ok())
                .unwrap_or_default();
            godot_error!("{}", msg);
        }
    }

    #[func]
    pub fn get_positions(&self) -> PackedVector2Array {
        self.snapshot()
            .get("base_positions")
            .and_then(|v| v.try_to::<PackedVector2Array>().ok())
            .unwrap_or_default()
    }

    #[func]
    pub fn set_positions(&mut self, value: PackedVector2Array) {
        let edit = self.set_vertex_positions(self.get_vertex_ids(), value);
        let ok = edit
            .get("ok")
            .and_then(|v| v.try_to::<bool>().ok())
            .unwrap_or(false);
        if !ok {
            let msg = edit
                .get("message")
                .and_then(|v| v.try_to::<GString>().ok())
                .unwrap_or_default();
            godot_error!("{}", msg);
        }
    }

    #[func]
    pub fn get_vertex_ids(&self) -> PackedInt64Array {
        self.snapshot()
            .get("vertex_ids")
            .and_then(|v| v.try_to::<PackedInt64Array>().ok())
            .unwrap_or_default()
    }

    #[func]
    pub fn set_vertex_positions(
        &mut self,
        ids: PackedInt64Array,
        positions: PackedVector2Array,
    ) -> Dictionary {
        let Some(mut bridge) = self.owner() else {
            return error_dict(
                "STALE_HANDLE",
                "The owning document was closed or replaced.",
            );
        };
        let res = bridge
            .bind_mut()
            .set_vertex_positions(self.id.clone(), ids, positions);
        res
    }

    #[func]
    pub fn replace_geometry(
        &mut self,
        ids: PackedInt64Array,
        positions: PackedVector2Array,
        uvs: PackedVector2Array,
        triangles: PackedInt64Array,
    ) -> Dictionary {
        let Some(mut bridge) = self.owner() else {
            return error_dict(
                "STALE_HANDLE",
                "The owning document was closed or replaced.",
            );
        };
        let mut data = self.snapshot();
        let ok = data
            .get("ok")
            .and_then(|v| v.try_to::<bool>().ok())
            .unwrap_or(false);
        if !ok {
            return data;
        }
        data.set("vertex_ids", &ids);
        data.set("base_positions", &positions);
        data.set("uvs", &uvs);
        data.set("triangles", &triangles);
        let res = bridge.bind_mut().replace_mesh(data);
        res
    }
}
