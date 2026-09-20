use godot::obj::InstanceId;
use godot::prelude::*;

use crate::conversions::{error_dict, is_main_thread, Dictionary};
use crate::document_bridge::KasaneDocumentBridge;

#[derive(GodotClass)]
#[class(init, base=RefCounted)]
pub struct KasaneDeformerData {
    base: Base<RefCounted>,
    owner_id: u64,
    generation: u64,
    epoch: u64,
    id: GString,
}

#[godot_api]
impl KasaneDeformerData {
    pub fn attach(&mut self, owner: u64, generation: u64, epoch: u64, id: GString) {
        self.owner_id = owner;
        self.generation = generation;
        self.epoch = epoch;
        self.id = id;
    }

    fn owner(&self) -> Option<Gd<KasaneDocumentBridge>> {
        if !is_main_thread() {
            return None;
        }
        let instance_id = InstanceId::try_from_i64(self.owner_id as i64)?;
        let bridge = Gd::<KasaneDocumentBridge>::try_from_instance_id(instance_id).ok()?;
        if bridge.bind().generation() == self.generation
            && bridge.bind().object_epoch(&self.id.to_string()) == self.epoch
        {
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
        let snap = bridge.bind().get_deformer_snapshot(self.id.clone());
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
            return error_dict("STALE_HANDLE", "Document was replaced or closed.");
        };
        let res = bridge.bind().get_deformer_snapshot(self.id.clone());
        res
    }

    #[func]
    pub fn get_angle_degrees(&self) -> f64 {
        self.snapshot()
            .get("angle_degrees")
            .and_then(|v| v.try_to::<f64>().ok())
            .unwrap_or(0.0)
    }

    #[func]
    pub fn set_angle_degrees(&mut self, angle: f64) {
        let r = self.update_rotation(self.get_center(), angle);
        let ok = r
            .get("ok")
            .and_then(|v| v.try_to::<bool>().ok())
            .unwrap_or(false);
        if !ok {
            let msg = r
                .get("message")
                .and_then(|v| v.try_to::<GString>().ok())
                .unwrap_or_default();
            godot_error!("{}", msg);
        }
    }

    #[func]
    pub fn get_center(&self) -> Vector2 {
        self.snapshot()
            .get("center")
            .and_then(|v| v.try_to::<Vector2>().ok())
            .unwrap_or(Vector2::ZERO)
    }

    #[func]
    pub fn set_center(&mut self, center: Vector2) {
        let r = self.update_rotation(center, self.get_angle_degrees());
        let ok = r
            .get("ok")
            .and_then(|v| v.try_to::<bool>().ok())
            .unwrap_or(false);
        if !ok {
            let msg = r
                .get("message")
                .and_then(|v| v.try_to::<GString>().ok())
                .unwrap_or_default();
            godot_error!("{}", msg);
        }
    }

    #[func]
    pub fn get_control_points(&self) -> PackedVector2Array {
        self.snapshot()
            .get("control_points")
            .and_then(|v| v.try_to::<PackedVector2Array>().ok())
            .unwrap_or_default()
    }

    #[func]
    pub fn set_control_points(&mut self, points: PackedVector2Array) {
        let r = self.update_control_points(points);
        let ok = r
            .get("ok")
            .and_then(|v| v.try_to::<bool>().ok())
            .unwrap_or(false);
        if !ok {
            let msg = r
                .get("message")
                .and_then(|v| v.try_to::<GString>().ok())
                .unwrap_or_default();
            godot_error!("{}", msg);
        }
    }

    #[func]
    pub fn update_rotation(&mut self, center: Vector2, angle: f64) -> Dictionary {
        let Some(mut bridge) = self.owner() else {
            return error_dict("STALE_HANDLE", "Document was replaced or closed.");
        };
        let res = bridge
            .bind_mut()
            .set_rotation(self.id.clone(), center, angle);
        res
    }

    #[func]
    pub fn update_control_points(&mut self, points: PackedVector2Array) -> Dictionary {
        let Some(mut bridge) = self.owner() else {
            return error_dict("STALE_HANDLE", "Document was replaced or closed.");
        };
        let res = bridge.bind_mut().set_warp_points(self.id.clone(), points);
        res
    }

    #[func]
    pub fn bind_to(&mut self, parent: GString) -> Dictionary {
        let Some(mut bridge) = self.owner() else {
            return error_dict("STALE_HANDLE", "Document was replaced or closed.");
        };
        let res = bridge.bind_mut().set_deform_parent(self.id.clone(), parent);
        res
    }
}
