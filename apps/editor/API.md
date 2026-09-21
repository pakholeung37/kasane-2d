# Editor scripting API

Scripts extend `RefCounted` and implement synchronous `run(w) -> Dictionary`. Return `{ "ok": true, ... }` for success, or `{ "ok": false, "code": "...", "message": "..." }` for a business failure. `w` is the application's `workspace.gd`, `w.document` is its single `KasaneDocumentBridge`, and `w.files` is `KasaneProjectIO`. Native classes are discoverable through Godot `ClassDB.class_get_method_list` and `class_get_property_list`. They do not inherit Node2D. Use `w.new_id()` for canonical UUID v4 IDs; keep IDs returned by prior requests rather than rebuilding objects.

The tested application version is Godot 4.7.2. The script logger requires Godot 4.5 or later. It uses the real [Godot Logger callbacks](https://docs.godotengine.org/en/stable/classes/class_logger.html) to capture compile/runtime messages and source lines. The editor is exported with the Godot debug template (and an optimized Rust release library): Godot release templates omit some runtime script checks and cannot meet this host's error contract. Call stacks are also explicitly enabled. Scripts run on the main thread, have normal local application privileges, and must finish synchronously. There is no sandbox, forced cancellation or automatic transaction. A runtime failure retains completed writes; results report starting and ending generation/revision. `await`, background document access, and scheduling delayed edits are outside this synchronous execution contract.

## Values and ownership

All model reads return copies. This includes `Dictionary`, `Array` and `Packed*Array` results. Changing a returned array does not edit the model. Submit changes explicitly with the relevant write method. Only methods listed below mutate model fields; the binding exposes no writable model properties. Runtime IDs and display names are different from stable object IDs. All native mutations return a Dictionary with `ok: bool`, `code: String`, `message: String`; edit results additionally contain `revision: int`, `change_kind: String`, `changed_meshes: PackedStringArray`, `changed_objects: PackedStringArray`, `referrers: PackedStringArray`.

Never continue dependent operations after `ok == false`. Successful native results currently use an empty `code`; the host uses `OK` for successful script execution. Numeric inputs must be finite. Reads/writes require the main thread. Unknown references, relationship cycles, incomplete keyforms, duplicate IDs and illegal deletion return structured failures. Array lengths and topology are validated before committing. Failed native edits do not change revision.

Mesh/deformer handles are checked against owner, document generation, object ID and deletion epoch. Open/import/new invalidates all earlier handles and snapshots. Deleting an object invalidates its handles even if the same ID is later recreated or restored through Undo. `get_id()` retains the original ID for diagnosis. Discard handles after invalidation and resolve a fresh handle by ID.

## Coordinates

Root geometry is original-canvas pixels: X right, Y down. Runtime X = `(source.x - canvas_origin.x) / pixels_per_unit`; runtime Y = `(canvas_origin.y - source.y) / pixels_per_unit`. Source UVs have `(0,0)` at top left; evaluated UVs have `(0,0)` at bottom left. Triangle indices are reversed at evaluation/export to match runtime winding.

**Parent assignment does not preserve the pose or convert arrays.** A Rotation child's positions are local runtime units; a Warp child's positions are normalized grid coordinates, with `[0,1]²` inside the grid and extrapolation outside it. Nested rotation origins and warp control points use their parent's domain. Supply base geometry and all affected keyforms explicitly in that domain. A downward-pointing parent warp grid can contribute a 180-degree inherited rotation; `base_angle` can compensate explicitly. Angles are counterclockwise degrees in runtime coordinates. There is no second editable Node2D transform representing source geometry.

`w.import_png(path, original_size = Vector2.ZERO, crop_offset = Vector2.ZERO)` decodes a PNG and creates an asset with an application-generated ID. An omitted original size means a full-canvas image. The explicit original size must equal the Document canvas; the cropped image must fit at the supplied offset. It returns `asset_id`, `image_size`, `original_size`, `crop_offset`. `w.create_rectangle(asset_id, crop_offset = Vector2.ZERO, name = "")` creates four source-pixel vertices, top-left UVs and two triangles, returning `mesh_id`. It does not triangulate arbitrary outlines or automatically attach deformers. Crop location lives in mesh geometry after creation.

## Workspace operations

| Method | Return / behavior |
|---|---|
| `new_id()` | String UUID v4 |
| `new_project(size: Vector2 = Vector2(1024,1024))` | Result; origin is size/2, ppu=100 |
| `open_project(path: String)` / `save_project(path: String)` | Project result; save to another path is Save As |
| `import_model(path: String)` | model3 import result; attachments not imported are reported |
| `export_model(directory: String)` | MOC3 runtime package result |
| `import_png(...)` / `create_rectangle(...)` | As described above |
| `find_object(id: String)` | `{ok, kind, data}` copied snapshot; kind is assets/parts/meshes/parameters/transforms/bindings/scene_bindings |
| `get_keyform(binding_id: String, keys: Array)` | `{ok, data}`; exact keys in declared axis order, not an interpolated sample |
| `set_keyform(binding_id: String, form: Dictionary)` | Replace that explicit combination while preserving all other forms |
| `complete_binding(description, template, scene=false, replace=false)` | Fill missing Cartesian combinations from a copied template; existing forms remain. Axes have 1–3 nonempty Array key lists, at most 4096 combinations in this helper. Core still validates completeness, values and duplicates atomically. |
| `fit_view(id: String = "")` | Fit all drawables, or the specified mesh, without changing model data |
| `begin_action(label: String)` / `end_action()` / `cancel_action()` | Explicit whole-Document snapshot Action. `w.undo_redo.undo()` / `redo()` replay it. Generation changes clear history. Snapshots include model fields only, not project files, external side effects, camera or selection. Preview parameters reset on restore. A script failure does not implicitly cancel an Action. |

## Document methods

`d` below means `w.document`. Unless stated otherwise, writes return the structured result above. Snapshot Dictionaries are copied and read-only with respect to the live model until written back.

| Methods | Arguments / result |
|---|---|
| `initialize(id, canvas_size, origin=Vector2.ZERO, pixels_per_unit=1.0)` | First initialization only; ID String, size/origin Vector2, ppu float |
| `new_project(id, canvas_size, origin=Vector2.ZERO, pixels_per_unit=1.0)` | Validates before replacing Document/session; clears path and preview, increments generation. Failure preserves old project. |
| `get_document_state()` | Lightweight `{ok,id,initialized,generation,revision,modified,transaction_active,path,canvas_size,canvas_origin,pixels_per_unit}` |
| `get_document_summary()` | State fields plus schema_version, asset_count, assets, meshes, deformers, parts, parameters, transforms, bindings, scene_bindings. Mesh entries are ID/name/counts; use mesh snapshot for geometry. Full keyform copies can be large. |
| `add_image_asset(id,name,source,width,height)` | Strings and positive uint32-size ints; metadata only. Prefer PNG import to decode dimensions. |
| `get_asset_snapshot(id)` | `{ok,id,name,source,width,height,revision}` |
| `create_mesh(description)` / `replace_mesh(description)` | Complete geometry Dictionary; replacement preserves previous runtime_id/properties when omitted. With bindings, topology changes require the atomic method below. |
| `replace_mesh_with_keyforms(description,binding)` | Replace bound topology and its complete MeshBinding as one revision. Preserve mesh/binding IDs; every form's positions must follow the submitted vertex-ID order. Failure preserves all old geometry/forms. |
| `get_mesh_snapshot(id)` | Full mesh description plus `ok`, `deform_parent`, `organization_parent`, `revision` |
| `get_mesh(id)` | KasaneMeshData handle or null |
| `set_vertex_positions(id, vertex_ids: PackedInt64Array, positions: PackedVector2Array)` | Explicit subset or full base-position update; IDs must exist |
| `rename_mesh(id,name)` | Structured name update |
| `set_mesh_properties(id,properties)` | Full properties Dictionary from current snapshot, with desired fields modified |
| `write_part(description,replace=false)` | Create/replace Part |
| `write_transform(description,replace=false)` | Create/replace full Rotation or Warp |
| `create_rotation(id,name,center: Vector2,angle: float)` | Convenience root Rotation; unique runtime ID, scale=1 |
| `create_warp(id,name,origin: Vector2,size: Vector2,columns: int,rows: int)` | Convenience regular root grid, 1–16 cells/axis; unique runtime ID. Full `write_transform` supports core limits (1–1024). |
| `set_rotation(id,center,angle)` / `set_warp_points(id,points: PackedVector2Array)` | Modify existing deformation data; full transform writes expose remaining fields |
| `get_deformer_snapshot(id)` | Concise rotation/warp snapshot; `kind` String, organization/deform parents, center/angle or control_points/rows/columns. `size` is not an inferred bound; use full points. |
| `get_deformer(id)` | KasaneDeformerData handle or null |
| `set_deform_parent(id,parent_id)` | Mesh/deformer; empty String detaches. No coordinate conversion. |
| `set_organization_parent(id,parent_id)` | Mesh/deformer -> Part, Part -> Part; empty String detaches |
| `create_parameter(description)` / `replace_parameter(description)` | Parameter Dictionary; new ranges must contain existing binding keys |
| `references_to(id)` | `{ok,referrers: PackedStringArray}`; deterministic referring IDs |
| `write_binding(description,replace=false)` | Full MeshBinding, exactly one form for every key combination |
| `write_scene_binding(description,replace=false)` | Full Part/Rotation/Warp binding |
| `set_mesh_keyform(binding_id,keys: PackedFloat32Array,positions: PackedVector2Array)` | Geometry-only replacement of an existing combination; preserves its appearance/order |
| `erase_object(id)` | Legal deletion of asset/Part/mesh/deformer/parameter/binding; referenced objects are rejected with referrers. Erase a binding to unbind. |
| `set_preview_values(values: Dictionary)` | Map parameter ID -> numeric requested value; replaces entire temporary map, clamps values, returns evaluated frame with actual values. Empty Dictionary restores defaults. Does not edit keyforms/revision. |
| `get_frame()` / `evaluate_mesh(id)` | Structured evaluation result; final runtime-unit positions, runtime UVs and dense indices. Full frame includes revision, coordinate_units, drawables and `{id,requested,value,clamped}` parameter samples. |
| `capture_state()` / `restore_state(state)` | KasaneDocumentState or null / structured restore; owner and generation checked. Does not restore files/session path. |
| `begin_transaction()` / `stage_vertex_positions(mesh_id,vertex_ids,positions)` / `commit_transaction()` / `cancel_transaction()` | Explicit base-vertex batch only; ordinary direct scripts do not create a transaction. Other edits reject an active transaction. |
| `commit_vertex_updates(updates: Array,expected_revision: int)` | Atomic base-position batch with optimistic revision check; each update is `{mesh_id,vertex_ids:PackedInt64Array,positions:PackedVector2Array}` |

Signals: `changed(change: Dictionary)` after document changes; `preview_changed()` after temporary values change. UI and renderer use these same signals. Reads during changed callbacks are supported; do not reenter the same mutable ProjectIO instance from its callback (use a separate stateless reader).

## Dictionary schemas

- **Mesh**: required `id,name,texture_asset_id: String`, `vertex_ids: PackedInt64Array`, `base_positions,uvs: PackedVector2Array`, `triangles: PackedInt64Array` (flat triples of stable vertex IDs, not array offsets). Optional `runtime_id: String`, `properties: Dictionary`. IDs fit uint32; positions and UV counts match, triangles reference distinct existing vertices. Packed arrays must have the declared element types.
- **Mesh properties**: `part_id,deformer_id: String`; `blend_mode: int` (0 normal, 1 additive, 2 multiplicative); `enabled,double_sided,inverted_mask: bool`; `appearance`; `masks: Array[String]`; optional `draw_order: float` in [-32768,32767]. Use the full snapshot properties, not a partial patch. Mesh name and texture are changed through full mesh replacement.
- **Appearance**: `{opacity: float, multiply: Array[float] of length 3, screen: Array[float] of length 3}`. Values are finite. Opacity need not be artificially clamped to 1, to preserve imported authoring values.
- **Part**: `{id,runtime_id,name,parent_id: String, enabled: bool, draw_order: float}`.
- **Parameter**: `{id,runtime_id,name: String, minimum,maximum,default_value: float, decimal_places: int=6}`. Minimum <= default <= maximum; decimal_places 0–9.
- **RotationPose**: `{origin: [x,y], angle: float, scale: float>=0, reflect_x,reflect_y: bool}`.
- **Transform**: `{id,runtime_id,name,part_id,parent_id: String, kind: int (0 Warp / 1 Rotation), base_angle: float, rows,columns: int, quad,enabled: bool, points: Array[[x,y]], rotation: RotationPose, appearance}`. Warp point count is `(rows+1)*(columns+1)` in row-major order. Rotation uses rows=columns=0 and empty points.
- **BindingAxis**: `{parameter_id: String, keys: Array[float]}`; strictly increasing unique finite values within the parameter range; 1–3 distinct axes per binding.
- **MeshBinding**: `{id,mesh_id: String, axes: Array[BindingAxis], keyforms: Array[MeshKeyform]}`. **MeshKeyform**: `{keys: Array[float], positions: PackedVector2Array or Array[[x,y]], appearance: Appearance (optional, defaults identity), draw_order: float (optional)}`. Positions follow the target mesh's vertex-ID order. There is one binding per target.
- **SceneBinding**: `{id,target_id: String, axes, keyforms}`. **SceneKeyform** adds required `rotation: RotationPose`; `positions` is empty for Part/Rotation and full control points for Warp. `appearance` defaults identity, `draw_order` defaults zero. Key combinations are matched by declared axis order; no nearest-key fallback. Full writes replace the entire binding atomically.

## Handles and file services

Mesh handle reads: `is_valid() -> bool`, `get_id()/get_name() -> String`, `get_positions() -> PackedVector2Array`, `get_vertex_ids() -> PackedInt64Array`, `snapshot() -> Dictionary`. Structured writes: `set_vertex_positions(ids,positions)`, `replace_geometry(ids,positions,uvs,triangles)`. Legacy `set_name` and `set_positions` return void and log failures; use `d.rename_mesh` or structured position writes for Agent operations.

Deformer handle reads: `is_valid`, `get_id`, `snapshot`, `get_angle_degrees() -> float`, `get_center() -> Vector2`, `get_control_points() -> PackedVector2Array`. Structured writes: `update_rotation(center,angle)`, `update_control_points(points)`, `bind_to(parent_id)`. Legacy setters `set_angle_degrees`, `set_center`, `set_control_points` return void; use the structured counterparts.

Every `w.files` method takes `d` as its first argument: `save_project(d,path)`, `open_project(d,path)`, `diagnose_resources(d)`, `relocate_asset(d,asset_id,path)`, `replace_asset(d,asset_id,path)`, `export_package(d,directory)`, `import_model3(d,path)`, `import_moc3(d,path,texture_map)`. Bare MOC3 texture maps are `{slot:int -> absolute_png_path:String}`. Relocation requires matching bytes/metadata; replacement explicitly changes bytes and dimensions. File operations return resource diagnostics and warnings; `resources_complete` is separate from `ok`. Successful import/open advances generation. Failed import/open preserves the active Document. Export produces model.moc3, model.model3.json, textures and report; it does not change the current project path.

## Local request protocol

Start with `-- --agent-dir=/absolute/directory`; default is `user://agent/<pid>`. The UI displays this directory. One live app owns a directory; simultaneous ownership is rejected. `status.json` includes app_id, generation, revision, engine, pid, path and busy. Wait for `running: true`.

Use `python3 tools/editor_agent.py --directory DIR --script FILE.gd [--observe] [--object-id MESH_ID] [--id EXECUTION_ID]`. The client copies the whole script, fsyncs temporary files and atomically publishes metadata. Do not edit an already published script. IDs contain only ASCII letters, digits, underscores and hyphens. Timeout means no result yet; it is not cancellation. Retry the same ID to retrieve the original result.

Raw requests are `requests/<id>.json` with `{id,app_id,generation,script_path,observe?:bool,object_id?:String}`. Publish script first, then rename the complete metadata file from a temporary filename. The application serially consumes only `.json` requests. Old app IDs/generations are rejected and removed with an unexecuted result, never retargeted. A durable `claims/<id>.json` is written before running. Completed IDs return their existing result; a claim with no result after interruption returns `EXECUTION_OUTCOME_UNKNOWN` with `executed:null` and is never replayed. Neither a crash nor a timeout promises that no writes occurred.

Results are atomically published to `results/<id>.json`: `ok`, `code`, `phase` (request/compile/runtime/business/observation), `executed`, start/end generation and revision, original script_path, compiler error code when applicable, errors `{file,line,function,message,type}`, captured logs, and the script's `business` Dictionary. JSON output normalizes packed arrays, Vector2 and Color to numeric arrays. UI shows a bounded summary and the complete result-file path.

`observe:true` waits for M4's exact render submission, rejects state changes, then writes a clean PNG with selection overlays hidden. Result metadata includes document generation/revision, actual/requested preview parameters, camera, viewport and output dimensions, crop rectangle, SHA-256 and renderer ready/submission state. `object_id` crops the selected mesh's evaluated bounds. Headless, missing textures, failed evaluation, offscreen crop, changed state and file-write failures return explicit failure; no old PNG is reused as the current result. Images remain local; this protocol has no upload transport.

### BlendShape, constraints, Glue and complete topology (M3B)

`get_document_summary()` now includes `blend_key_tables`, `blend_constraints`,
`blend_bindings`, and `glues`, in document order. `workspace.find_object(id)` and
Hierarchy/Inspector expose these objects and their target/parameter/constraint links.

Each collection has `get_<type>_snapshot(id)` and
`write_<type>(description: Dictionary, replace: bool = false)`, where `<type>` is
`blend_key_table`, `blend_constraint`, `blend_binding`, or `glue`.
Use `erase_object(id)` and `references_to(id)` for reference-safe deletion.
Writes validate before mutation, emit `changed`, and participate in
`workspace.begin_action/end_action` undo/redo. Snapshots are detached values.

- Key table: `{id, parameter_id, keys: Array[float], base_key_idx: int}`. The parameter must be BlendShape.
- Constraint: `{id, parameter_id, keys: Array[float], weights: Array[float]}`. Either parameter kind is allowed; weights are in `[0,1]`.
- Blend binding: `{id, target_id, target_kind: "mesh"|"warp"|"rotation"|"part", key_table_id, constraint_ids: Array[String], keyforms: {type: same target kind, items: Array[Dictionary]}}`.
  Each delta item follows the snapshot: mesh `positions`, warp `points`, rotation `origin/angle/scale`, part `draw_order`; supported appearance fields are `opacity/multiply/screen`. Optional values are `null` when absent. Do not replace an absent color with zero: those have different interpolation semantics.
- Glue: `{id, runtime_id, name, mesh_a_id, mesh_b_id, pairs: [{vertex_a, vertex_b, weight_a, weight_b}], intensity, binding?: {axes: [{parameter_id, keys}], keyforms: [{intensity}]}}`.
  Axes reference Normal parameters. The first axis varies fastest in the Cartesian keyform grid. Binding intensity overrides the static intensity; all intensities must be finite. Vertex IDs refer to stable mesh IDs, not array indices. Pair order and weights are preserved.

New structured snapshots represent points as `{x,y}`; writes also accept `Vector2`.
Use ordinary Arrays or the corresponding packed arrays. Numeric IDs and indices must
be integers; invalid types, incomplete key grids and dangling references return errors.

`get_mesh_topology_snapshot(mesh_id)` returns `{mesh, binding, blend_bindings, glues,
vertex_mapping}`. Edit this complete snapshot and submit it with
`replace_mesh_topology(description)`. `vertex_mapping` contains `[old_id, new_id]`
(or `[old_id, null]` for removal) for every old vertex. Preserve binding/Glue IDs,
update all ordinary/delta geometry, and replace every affected Glue pair reference.
The method requires the exact dependency set, preserves collection order, and commits
one revision; failure leaves all collections unchanged. Wrap it in one workspace Action
for a single Undo step.

Project writes use format v3 for dedicated Glue intensity bindings; v1/v2 remain readable.
Old non-null `binding_id` MeshBinding references are rejected instead of flattened.

Rotation 原点的作者坐标在 v3 中使用双精度保存，保证 root 的 runtime→像素→runtime 往返可逆。普通 pose Dictionary 的 `origin: [x,y]` 保留 f64；使用 Vector2 输入会采用该输入本身的 f32 精度。运行时求值仍为 f32。

Preview 的 `refresh()` 重新校验磁盘资源；`refresh_geometry()` 复用已验证纹理，仅提交几何/外观/遮罩更新。参数和相机信号使用后者，截图前仍显式执行完整 refresh。
