use std::collections::HashMap;
use std::sync::Arc;

use crate::document::Document;
use crate::types::{BindingAxis, Status, Vec2};

#[derive(Debug, Clone)]
pub(super) struct SelectionAxis {
    pub parameter: usize,
    pub keys: Vec<f32>,
    pub epsilon: f32,
}

#[derive(Debug, Clone)]
pub(super) struct StaticGeometry {
    pub(super) uvs: Arc<[Vec2]>,
    pub(super) indices: Arc<[u32]>,
}

/// Immutable evaluation inputs, owned by the document and invalidated by structural edits.
#[derive(Debug, Clone)]
pub(crate) struct PreparedEvaluation {
    pub(super) parts: Vec<String>,
    pub(super) transforms: Vec<String>,
    pub(super) transform_slots: HashMap<String, usize>,
    pub(super) assets: HashMap<String, usize>,
    pub(super) meshes: HashMap<String, usize>,
    pub(super) groups: Vec<crate::draw_order::DrawOrderGroup>,
    pub(super) order_slots: HashMap<String, usize>,
    pub(super) offscreen_slots: HashMap<String, usize>,
    pub(super) totals: HashMap<String, usize>,
    pub(super) geometry: Vec<StaticGeometry>,
    pub(super) selections: Vec<Vec<SelectionAxis>>,
    pub(super) mesh_selections: Vec<Option<usize>>,
    pub(super) scene_selections: HashMap<String, usize>,
}
impl PreparedEvaluation {
    pub(crate) fn new(doc: &Document) -> Result<Self, Status> {
        let groups = crate::draw_order::resolved_groups(doc);
        let totals = crate::draw_order::descendant_counts_with_offscreens(doc, &groups)
            .into_iter()
            .map(|(id, n)| (id.to_owned(), n))
            .collect();
        let mut geometry = Vec::new();
        for id in doc.mesh_order() {
            let mesh = doc.get_mesh(id).unwrap();
            let uvs = mesh
                .uvs
                .iter()
                .map(|uv| {
                    Vec2::new(
                        uv.x,
                        if doc.canvas().flag & 1 == 0 {
                            uv.y
                        } else {
                            1.0 - uv.y
                        },
                    )
                })
                .collect::<Vec<_>>();
            let mut indices = Vec::new();
            doc.render_indices_into(id, &mut indices)?;
            for tri in indices.as_chunks_mut::<3>().0 {
                tri.swap(1, 2);
                if doc.canvas().flag & 1 == 0 {
                    tri.swap(0, 2);
                }
            }
            geometry.push(StaticGeometry {
                uvs: uvs.into(),
                indices: indices.into(),
            });
        }
        let transforms = doc.sorted_transforms();
        let parameter_slots: HashMap<_, _> = doc
            .parameter_order()
            .iter()
            .enumerate()
            .map(|(i, id)| (id.as_str(), i))
            .collect();
        // Identical axis grids sample the same normalized parameters. Intern
        // them once per document, including their axis order and float bits.
        let mut selections = Vec::new();
        let mut selection_slots = HashMap::new();
        let mut intern = |axes: &[BindingAxis]| {
            let key: Vec<_> = axes
                .iter()
                .map(|axis| {
                    (
                        parameter_slots[axis.parameter_id.as_str()],
                        axis.keys.iter().map(|v| v.to_bits()).collect::<Vec<_>>(),
                    )
                })
                .collect();
            *selection_slots.entry(key).or_insert_with(|| {
                let slot = selections.len();
                selections.push(
                    axes.iter()
                        .map(|axis| SelectionAxis {
                            parameter: parameter_slots[axis.parameter_id.as_str()],
                            keys: axis.keys.clone(),
                            epsilon: 0.1f32.powi(
                                doc.get_parameter(&axis.parameter_id)
                                    .unwrap()
                                    .decimal_places,
                            ),
                        })
                        .collect(),
                );
                slot
            })
        };
        let mesh_selections = doc
            .mesh_order()
            .iter()
            .map(|id| {
                doc.binding_for_mesh(id)
                    .map(|binding| intern(&binding.axes))
            })
            .collect();
        let mut scene_selections = HashMap::new();
        for id in doc.scene_binding_order() {
            let binding = doc.get_scene_binding(id).unwrap();
            scene_selections.insert(binding.target_id().to_owned(), intern(&binding.axes));
        }
        for id in doc.glue_order() {
            if let Some(binding) = &doc.get_glue(id).unwrap().binding {
                scene_selections.insert(id.clone(), intern(&binding.axes));
            }
        }
        Ok(Self {
            parts: doc.sorted_parts(),
            transform_slots: transforms
                .iter()
                .enumerate()
                .map(|(i, id)| (id.clone(), i))
                .collect(),
            transforms,
            assets: doc
                .asset_order()
                .iter()
                .enumerate()
                .map(|(i, id)| (id.clone(), i))
                .collect(),
            meshes: doc
                .mesh_order()
                .iter()
                .enumerate()
                .map(|(i, id)| (id.clone(), i))
                .collect(),
            order_slots: doc
                .mesh_order()
                .iter()
                .chain(doc.part_order())
                .chain(std::iter::once(&String::new()))
                .enumerate()
                .map(|(i, id)| (id.clone(), i))
                .collect(),
            offscreen_slots: doc
                .offscreen_order()
                .iter()
                .enumerate()
                .map(|(i, id)| (id.clone(), i))
                .collect(),
            groups,
            totals,
            geometry,
            selections,
            mesh_selections,
            scene_selections,
        })
    }
}
