use kasane_core::types::{Status, TransformKind};

use crate::layout::checked;
use crate::schema::section;

use super::context::Moc3EncoderContext;
use super::helpers::{index_of, write_colors, write_id, write_positions};

impl<'a> Moc3EncoderContext<'a> {
    pub(super) fn encode_transforms(&mut self) -> Result<(), Status> {
        for id in &self.transforms {
            let t = self.doc.get_transform(id).unwrap();
            let b = self.doc.binding_for_scene(id);
            let warp = t.kind() == TransformKind::Warp;
            let prefix = if warp { "warp" } else { "rotation" };
            let count = if let Some(b) = b {
                checked(b.track.len(), id)?
            } else {
                1
            };
            let stored = if let Some(b) = b {
                count.max(1i32 << b.axes.len())
            } else {
                1
            };

            write_id(
                &mut self.l,
                section::DEFORMER_SRC_ID,
                "deformer_src.id",
                &t.runtime_id,
            )?;
            self.l.integer(
                "deformer_src.binding_idx",
                if let Some(b) = b {
                    self.binding_indices[b.id.as_str()]
                } else {
                    0
                },
            )?;
            self.l.integer("deformer_src.visible", 1)?;
            self.l
                .integer("deformer_src.enable", if t.enabled { 1 } else { 0 })?;
            self.l.integer(
                "deformer_src.parent_part_idx",
                index_of(&self.part_indices, t.part()),
            )?;
            self.l.integer(
                "deformer_src.parent_deformer_idx",
                index_of(&self.transform_indices, t.parent()),
            )?;
            self.l
                .integer("deformer_src.type", if warp { 0 } else { 1 })?;

            let local_slot = if warp { 2 } else { 3 };
            let local_idx = self.l.counts[local_slot];
            self.l.counts[local_slot] += 1;
            self.l.integer("deformer_src.local_idx", local_idx as i32)?;
            if warp {
                self.warp_local_indices
                    .insert(id.clone(), local_idx as usize);
            } else {
                self.rotation_local_indices
                    .insert(id.clone(), local_idx as usize);
            }

            self.l.integer(
                &format!("{prefix}_src.binding_idx"),
                if let Some(b) = b {
                    self.binding_indices[b.id.as_str()]
                } else {
                    0
                },
            )?;
            self.l.integer(
                &format!("{prefix}_src.keyform_off"),
                self.l.counts[if warp { 7 } else { 8 }] as i32,
            )?;
            self.l.integer(&format!("{prefix}_src.key_len"), count)?;
            let color_off = checked(self.l.field("keyform_mul_color_src.r")?.len() / 4, id)?;
            self.l
                .integer(&format!("{prefix}_src.key_color_off"), color_off)?;

            if warp {
                self.l.integer(
                    "warp_src.vertex_count",
                    checked(t.warp().unwrap().points.len(), id)?,
                )?;
                self.l
                    .integer("warp_src.row", t.warp().unwrap().rows as i32)?;
                self.l
                    .integer("warp_src.col", t.warp().unwrap().columns as i32)?;
                self.l.integer(
                    "warp_src.quad_transform",
                    if t.warp().unwrap().quad { 1 } else { 0 },
                )?;
            } else {
                self.l
                    .scalar("rotation_src.base_angle", t.rotation().unwrap().base_angle)?;
            }

            for k in 0..stored {
                let f = if let Some(b) = b {
                    b.track.sample(k.min(count - 1) as usize)
                } else {
                    kasane_core::SceneSample {
                        keys: &[],
                        positions: t.warp().map_or(&[], |w| w.points.as_slice()),
                        rotation: t.rotation().map(|r| r.pose).unwrap_or_default(),
                        appearance: t.appearance,
                        draw_order: 0.0,
                    }
                };

                self.l
                    .scalar(&format!("{prefix}_key_src.opacity"), f.appearance.opacity)?;
                write_colors(&mut self.l, &format!("{prefix}_key_src"), &f.appearance)?;

                if warp {
                    write_positions(
                        &mut self.l,
                        self.doc,
                        "warp_key_src",
                        t.parent(),
                        f.positions,
                    )?;
                } else {
                    let origin = kasane_core::evaluation::to_parent_origin(
                        self.doc,
                        t.parent(),
                        f.rotation.origin,
                    )?;
                    self.l.scalar("rotation_key_src.origin_x", origin.x)?;
                    self.l.scalar("rotation_key_src.origin_y", origin.y)?;
                    self.l.scalar("rotation_key_src.angle", f.rotation.angle)?;
                    self.l.scalar("rotation_key_src.scale", f.rotation.scale)?;
                    self.l.integer(
                        "rotation_key_src.reflect_x",
                        if f.rotation.reflect_x { 1 } else { 0 },
                    )?;
                    self.l.integer(
                        "rotation_key_src.reflect_y",
                        if f.rotation.reflect_y { 1 } else { 0 },
                    )?;
                }
            }
            self.l.counts[if warp { 7 } else { 8 }] += stored as u32;
        }

        Ok(())
    }
}
