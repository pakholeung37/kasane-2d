# Python API reference

This reference describes the public `kasane` package in this repository. Use
`import kasane`; `kasane._native` is an implementation module. The typed Python
records and wrappers are exposed by [`python/kasane/__init__.py`](python/kasane/__init__.py);
geometry builders and editing recipes live in separate Python modules.
For installation and runnable recipes, start with the [SDK README](README.md).

## Conventions

- `Point` is a `tuple[float, float]`, such as `(40, 60)`. It is a type alias,
  not a constructor. Canvas and editable mesh positions use source pixel
  coordinates. Evaluated drawable positions use runtime coordinates: for a
  root mesh, `((x - origin_x) / pixels_per_unit,
  (origin_y - y) / pixels_per_unit)`. `CanvasSnapshot.source_to_runtime()`
  and `runtime_to_source()` perform these conversions for root drawables.
- Object IDs and document IDs are canonical lowercase UUID strings; generate
  them with `str(uuid.uuid4())`. IDs identify objects across reads and edits.
  Display names are not IDs and may be duplicated.
- Native project, import, export, and PNG paths must be absolute `Path` values.
  `Edit.add_png_asset_from_base()` is the exception for a relative PNG path:
  it pairs that path with an explicit absolute base. `Session.save()` and
  `Observer.observe_run()` check absolute paths in Python; other calls are
  checked by the native layer.
- Most reads return `NamedTuple` snapshots. Their list fields are copies; edit
  methods must be called to publish changes. Use `record._replace(...)` to
  copy a record with changed fields. A snapshot's `version` identifies the
  state read, not a live view.
- `Version` is `(session_id, generation, revision)`. Pass a saved value as
  `expected_version` to reject stale edits or IO. Replacing a project advances
  the generation. `Session.version` is a property, so use `session.version`,
  not `session.version()`.

## Entry points and results

| API | Purpose |
| --- | --- |
| `Session(document_id, width, height, origin, pixels_per_unit)` | Create an empty authoring session. `origin` is a `Point`. |
| `Session.with_history_limits(document_id, width, height, origin, pixels_per_unit, max_steps, max_bytes)` | Create a session with explicit undo capacity. |
| `open_project(absolute_path)` | Open a saved project and return a `Session`. This is a package function, not a `Session` method. |
| `capabilities()` | Return booleans including `gpu_observation`, `purism_core_validation`, and `official_core_validation`. |
| `Session.save(absolute_path, expected_version=None, *, on_exists="error")` | Save to a project directory or `.kasane.json` manifest path; return `SaveResult`. `on_exists="new"` chooses a numbered sibling if another project occupies the destination. |
| `SaveResult` | `manifest: Path` is the **actual** manifest path; `durable: bool`, `warnings: list[str]`, `history_warnings: list[str]` describe publication. |
| `Session.import_model3(absolute_path, expected_version=None)` | Replace the current project from a model3 JSON file; return `ImportResult`. |
| `Session.import_bare_moc3(absolute_path, texture_map, expected_version=None)` | Replace the current project from a MOC3 file. `texture_map` maps integer texture slots to absolute PNG paths. |
| `Session.import_psd(absolute_path, destination, expected_version=None)` | Publish a layered 8-bit RGB PSD as a new project directory and replace the current session after publication. Both paths are absolute; `destination` must not exist. Unique ASCII identifier layer names become mesh runtime IDs; duplicate or unsuitable names receive stable generated IDs. |
| `ImportResult` | `version`, `moc_version`, `diagnostics: list[ResourceIssue]`, `warnings`. Imported content has no saved `project_path` until saved. |
| `PsdImportResult` | `version`, `manifest`, `width`, `height`, `raster_layers`, `groups`, `durable`, `warnings`. The new project is already saved. |
| `Session.export_package(absolute_path, expected_version=None)` | Publish a standalone MOC3/model3/texture directory with its supported animation, CDI, Sound, and UserData assets; return `ExportResult(published, durable, warnings)`. |
| `Session.new_project(document_id, width, height, origin, pixels_per_unit, expected_version=None)` | Replace the current in-memory project and return its new `Version`. |
| `Session.remesh_rectangle_grid(mesh_id, columns, rows, expected_version=None)` | Atomically subdivide a PSD/create_rectangle quad and migrate its ordinary mesh keyforms, mesh BlendShape deltas, and glue references. Bound meshes require equal columns and rows. |

`save()` never silently replaces an unrelated project. `on_exists="new"` does
not override a `PROJECT_CONFLICT` on a session's own saved project. Check
`durable` and `warnings` even when a save or export returns successfully.

## Reading a session

| API | Returns / behavior |
| --- | --- |
| Properties: `version`, `document_id`, `canvas`, `uv_v_origin`, `draw_order_groups`, `evaluation_revision`, `modified`, `project_path` | Current values; `project_path` is `Path | None`. `canvas.origin` returns the original `(x, y)` pair. `uv_v_origin` is `"top"` or `"bottom"` for mesh UVs relative to PNG rows. `draw_order_groups` is `None` when no explicit groups exist. |
| `asset_ids()`, `mesh_ids()`, `parameter_ids()`, `binding_ids()` | IDs of PNG assets, meshes, parameters, and mesh bindings. |
| `part_ids()`, `transform_ids()`, `scene_binding_ids()`, `offscreen_ids()`, `glue_ids()` | IDs of scene and composition objects. |
| `blend_key_table_ids()`, `blend_constraint_ids()`, `blend_binding_ids()` | IDs of BlendShape objects. |
| `asset(id)`, `mesh(id)`, `mesh_record(id)`, `mesh_properties(id)`, `parameter(id)`, `geometry(id)` | Snapshots or `None` if absent. `mesh()` is a short name/position view; `mesh_record()` includes full geometry, drawing, and relationships. |
| `binding(id)`, `binding_for_mesh(mesh_id)` | A `MeshBindingSnapshot` or `None`. |
| `part(id)`, `transform(id)`, `scene_binding(id)`, `binding_for_scene(target_id)` | Scene snapshots or `None`. |
| `offscreen(id)`, `glue(id)`, `blend_key_table(id)`, `blend_constraint(id)`, `blend_binding(id)` | Composition and BlendShape snapshots or `None`. |
| `parameter_id(name_or_id)` | Resolve a parameter ID or unique display name; duplicate names raise `ValueError`. |
| `find_meshes_by_name(name)`, `require_unique_mesh(name)` | Return all matches or require exactly one match. |
| `references_to(id)` | IDs of current objects referring to an object. |
| `handle(kind, id)`, `resolve_handle(handle)`, `mesh_by_handle(handle)` | Obtain and check an `ObjectHandle`; typed mesh lookup is available. Handles become stale when an object disappears, even if undo/redo later restores it. |
| `history_state()`, `history_lengths()`, `estimated_content_bytes()` | Undo capacity/usage, `(undo_steps, redo_steps)`, and a content estimate. |
| `drain_events()` | Consume published edit and undo/redo `EditEvent` records. |
| `validate_structure()`, `diagnose_resources()`, `diagnose_geometry(min_triangle_area=0, canvas_bounds=None)` | Lists of `StructureIssue`, `ResourceIssue`, or `GeometryIssue`. Geometry issues are authoring hints; resource diagnosis checks files separately from structure validation. |

`evaluate(values).drawables` contains `DrawableSample` records. Each record has
`id` (the authoring mesh ID) and `positions` (runtime coordinates); it has no
`mesh_id` or `name` field. To select a drawable, first resolve the mesh with
`require_unique_mesh(name)` or compare `mesh_record(id).runtime_id`, then match
`drawable.id == mesh.id`. On model3 import, authoring IDs may be rebuilt, so
use public runtime IDs or names to find objects after a fresh import.

`GeometrySnapshot` includes `vertex_ids`, `positions`, `uvs`, `triangles`,
`space`, `parent_id`, and `version`. Triangles refer to vertex IDs. Root mesh
positions are in canvas pixels; positions under a deformer are local to their
parent. `mesh_record()` is for full replacement and topology work;
`mesh_properties()` is for drawing-only changes.

`rectangle_grid_geometry` and `Session.remesh_rectangle_grid` delegate geometry
generation and dependency migration to the Rust SDK.

`rectangle_grid_geometry(source, columns, rows)` returns a deterministic
regular grid from a four-corner, axis-aligned PSD/create_rectangle mesh with
canonical UVs and the standard diagonal. It retains corner vertex IDs and
interpolates the two original triangles. Use it when preparing a new binding
inside an existing edit; use `Session.remesh_rectangle_grid()` when the mesh
already has binding, BlendShape, or glue dependencies. The session method is
one undoable edit and rejects bound rectangular grids with unequal dimensions
because the old triangle diagonal would cross new cells. It cannot remesh an
already subdivided mesh.

When locating a mesh's texels in a PNG atlas, use `session.uv_v_origin`.
For `"top"`, a UV `v` maps near PNG row `v * height`; for `"bottom"`, it maps
near `(1 - v) * height`. `u` maps near column `u * width`. UV bounds only locate
the neighborhood: filtering and packed raster edges can differ from a resized
PSD layer. Check the actual atlas pixels before editing an asset.

## Atomic edits

```python
before = session.version
with session.edit("move mesh", expected_version=before) as edit:
    geometry = session.geometry(mesh_id)
    edit.update_positions(
        mesh_id, geometry.vertex_ids,
        [(x + 5, y) for x, y in geometry.positions],
    )
```

`Session.edit(label, expected_version=None)` returns an `Edit`. A successful
`with` exit commits all commands as one revision. An exception cancels the
batch. `edit.commit()` and `edit.cancel()` are available for manual control.
`edit.parameter(id)` and `edit.mesh(id)` read preceding changes within the
candidate edit; `session.parameter(id)` and `session.mesh(id)` still see the
published version until commit. Any failed command aborts the edit even if
its exception is caught; a later `commit()` raises `EDIT_ABORTED`. Save,
import/export, undo/redo, project reset, and nested edits are rejected with
`EDIT_ACTIVE` while an edit is open.

| Edit method | Purpose |
| --- | --- |
| `add_png_asset(id, name, absolute_path)` | Add a PNG after reading its size and hash. |
| `add_png_asset_from_base(id, name, absolute_base, relative_path)` | Resolve a PNG against an explicit absolute base. |
| `replace_png_asset(id, name, absolute_path)` | Replace asset content, allowing a new size and hash. Keep the source PNG at that path until save and package export finish; those operations read it again. |
| `relocate_png_asset(id, absolute_path)` | Move an asset reference only when the replacement PNG matches the old size and hash. |
| `create_rectangle(mesh_id, name, asset_id, minimum, maximum)` | Create a four-vertex, two-triangle root mesh. |
| `create_mesh(MeshRecordSpec)`, `replace_mesh(MeshRecordSnapshot)` | Create or replace a full mesh record. |
| `replace_topology(source, mesh, vertex_mapping, binding=None, blend_bindings=(), glues=())` | Replace mesh topology and affected bindings/glues atomically, using a `GeometrySnapshot` from the edit's starting version. |
| `rename_mesh(mesh_id, name)`, `update_positions(mesh_id, vertex_ids, positions)` | Change a mesh name or source positions. |
| `update_mesh_properties(mesh_id, MeshProperties)` | Replace drawing fields while preserving geometry. |
| `create_parameter(id, name, minimum, maximum, default_value, repeat=False, kind="normal", runtime_id=None)` | Add a normal or `blend_shape` parameter. Supply `runtime_id` for the identifier used in an exported MOC3; omitted uses the internal UUID. |
| `replace_parameter(id, name, minimum, maximum, default_value, repeat=False, kind=None, runtime_id=None)` | Change a parameter; `kind=None` and `runtime_id=None` keep their current values. |
| `create_mesh_binding(id, mesh_id, axes, forms)`, `replace_mesh_binding(...)` | Set a complete mesh parameter grid. |
| `set_mesh_keyform(binding_id, MeshKeyform)` | Update an existing key combination. |
| `create_part(...)`, `replace_part(...)`, `set_organization_parent(part_id, parent_id)` | Manage the Part tree. |
| `create_rotation_transform(...)`, `create_warp_transform(...)`, `replace_transform(TransformSnapshot)` | Manage typed deformers. |
| `update_rotation(transform_id, RotationData)`, `update_warp_points(transform_id, points)` | Update existing deformer data. |
| `set_transform_parent(transform_id, parent_id)`, `set_transform_part(transform_id, part_id)` | Set a transform's deformer parent or owning Part. |
| `set_deform_parent(mesh_id, transform_id)`, `set_mesh_part(mesh_id, part_id)` | Set a mesh's deformer parent or owning Part. |
| `create_scene_binding(id, kind, target_id, axes, forms)`, `replace_scene_binding(snapshot)` or `replace_scene_binding(id, kind, target_id, axes, forms)`, `set_scene_keyform(binding_id, form)` | Manage Part, Rotation, and Warp parameter grids. To change one existing form, prefer `set_scene_keyform`. |
| `create_offscreen(OffscreenSpec)`, `replace_offscreen(OffscreenSnapshot)` | Manage an offscreen composition layer. |
| `replace_part_binding_with_offscreen(binding, offscreen)` | Change a Part binding and related offscreen keyform mapping together. |
| `create_glue(GlueSpec)`, `replace_glue(GlueSnapshot)` | Manage paired mesh vertices and optional parameter binding. |
| `create_blend_key_table(BlendKeyTableSpec)`, `replace_blend_key_table(BlendKeyTableSnapshot)` | Manage BlendShape keys. |
| `create_blend_constraint(BlendConstraintSpec)`, `replace_blend_constraint(BlendConstraintSnapshot)` | Manage BlendShape weights. |
| `create_blend_binding(BlendBindingSpec)`, `replace_blend_binding(BlendBindingSnapshot)` | Bind BlendShape deltas to a target. |
| `replace_canvas(width, height, origin, pixels_per_unit)`, `replace_draw_order_groups(groups)` | Change document-wide settings. |
| `erase_object(object_id)` | Delete an unreferenced object; referenced objects raise `OBJECT_REFERENCED` with `referrers`. |
| `set_parameter_display_name(id, name)`, `set_part_display_name(id, name)` | Edit CDI names while retaining runtime IDs. |
| `create_parameter_group(id, runtime_id, name, parent_id=None)`, `replace_parameter_group(...)`, `set_parameter_group(parameter_id, group_id)` | Edit CDI hierarchy and membership. Group `id` is a project UUID. |
| `set_combined_parameters(set_id, parameter_ids)`, `import_cdi3(text)` | Edit ordered combined sets or import CDI inside an atomic edit. `import_cdi3` returns `CdiDiagnostic` records. |

`Session.display_info()` returns detached CDI metadata, and
`Session.export_cdi3()` encodes the current names and runtime IDs. An
unresolved imported target can be saved for repair; strict CDI export reports
unresolved parameter references with `SdkFailure.field_path`. Imported labels
for absent Parts are preserved by runtime ID. `export_package()` writes `model.cdi3.json`
and references it from `model.model3.json`.

`Edit.create_expression(id, name, entries, fade_in=None, fade_out=None)` creates
an expression from `(parameter_uuid, value, blend)` tuples. Blend is `add`,
`multiply`, `overwrite`, or `default` (the exp3 omitted Add default).
`Edit.replace_expression(...)` updates known fields. `Edit.import_expression3(id, name, text)` imports or replaces an exp3 asset and
returns `ExpressionDiagnostic` records for unresolved runtime IDs. An existing
expression can be removed with `erase_object(id)`. `Session.expression_ids()`,
`Session.expression(id)`, and `Session.export_expression3(id)` expose ordered
assets and strict exp3 output. Unknown JSON fields are retained in the project;
the exporter blocks content edits or a changed parameter runtime namespace
while they exist. Reimporting establishes a new baseline for unknown fields.
Package export writes registered expressions to `expressions/*.exp3.json` and
references them from `model.model3.json`. Export filenames use safe readable
asset names; case-insensitive collisions receive a short ID suffix.
`Session.expression_preview()` captures a detached Expression stage.
`ExpressionPreview.schedule_expression(id, time)` records activations;
`advance(dt)` and `seek(time)` return `ExpressionSnapshot` with UUID keyed
parameter values and active expression IDs. `frame()` evaluates drawable
geometry for the current values. `seek` replays from time zero in 1/60 second
steps and does not change the authoring session.

Motion clips use stable UUIDs. `Edit.create_motion(id, name, duration, fps,
looping=False, restricted_beziers=True, fade_in=None, fade_out=None)` creates
an empty clip. `create_motion_track()`, `replace_motion_track()`,
`set_motion_segment()`, `insert_motion_segment()`, `move_motion_key()`,
`set_motion_event()`, `remove_motion_track()`, `remove_motion_event()`, and
`set_motion_timing()` edit its timeline. These methods take detached mapping
records for tracks, segments, and events; `Session.motion(id)` returns their
current shape. `Edit.replace_motion()` replaces that detached record.
`Edit.import_motion3(id, name, text)` returns unresolved target diagnostics.
`Session.motion_ids()`, `motion_groups()`, and `export_motion3(id)` read the
assets and registration. `Edit.set_motion_groups(groups)` sets ordered model3
registrations, including per-entry fade and Sound references.
Imported Motion names default to the source file stem, so package export can
retain names such as `mtn_01.motion3.json`. Imported virtual tracks retain
their runtime IDs on export and appear in preview coverage diagnostics.

`Edit.create_pose(id, groups, fade_in=None)` takes ordered groups of
`(part_uuid, linked_part_uuids)` pairs. `replace_pose()` accepts a detached
`Session.pose()` mapping; `import_pose3(id, text)` reports unresolved Parts.
`Session.export_pose3()` returns `None` when no Pose exists.

`Edit.create_physics(id, physics3, parameter_bindings)` stores a typed
physics3 record and a mapping from runtime parameter IDs to project UUIDs.
`replace_physics()` accepts `Session.physics()`; `import_physics3(id, text)`
reports unresolved parameter IDs. `Session.export_physics3()` returns `None`
when there is no Physics asset. `Session.physics_preview()` captures a
detached stateful simulator with `set_parameter()`, `advance(dt)`,
`stabilize()`, `reset()`, `parameters()`, and `diagnostics()`.

`Session.motion_preview()` combines Motion → Expression → Physics → Pose on
one detached document snapshot. Schedule clips and expressions with
`schedule_motion(id, time)` and `schedule_expression(id, time)`; use
`schedule_parameter_input(parameter_uuid, time, value)` for reproducible
editor input. `advance(dt)` and `seek(time)` return a `MotionSnapshot` with
parameter values, virtual Part control channels, resulting Part opacities,
model opacity, active assets, fired events, and coverage diagnostics.
Loop events are collected across every crossed cycle, including large deltas.
An estimated event batch above one million is rejected with `EVENT_LIMIT`
before changing preview state.
`stabilize_physics()` settles the particle state. `frame()` evaluates drawable
geometry from real parameter values. Create a new preview after editing the
document. Dynamic EyeBlink/LipSync model mappings are reported as coverage
gaps; ordinary Parameter curves work.
`seek_with_progress(time, callback)` reports `(completed_steps, total_steps)`
for remaining replay work after cache restoration (exact hits report `(0, 0)`);
return `False` to cancel with `SEEK_CANCELLED` while retaining the preceding
preview state. Exceptions from the callback also leave the state unchanged.



### Motion timeline records and indices

The following records use project UUIDs, seconds and domain field names, not
motion3 wire-format arrays. Parameter and Part references must exist in the
same document. Use `Session.motion(id)` as the starting point for full-clip
replacement; it returns `None` for an unknown UUID.

```python
point = {"time": 1.0, "value": 0.5}
segment = {"kind": "linear", "end": point}
track = {
    "id": track_uuid,
    "target": {"kind": "parameter", "parameter_id": parameter_uuid},
    "initial": {"time": 0.0, "value": 0.0},
    "segments": [segment],
    "fade_in": None, "fade_out": None, "extensions": {},
}
event = {"id": event_uuid, "time": 0.5, "value": "cue", "extensions": {}}
groups = [{"name": "Idle", "entries": [{
    "clip_id": motion_uuid, "fade_in": None, "fade_out": None,
    "sound": None, "extensions": {},
}]}]
```

Segment `kind` is `linear`, `stepped`, `inverse_stepped` or `bezier`.
Every segment has an `end` point; Bezier additionally has `control1` and
`control2` points of the same `{time, value}` shape. Points must be finite,
endpoints must increase in time, and the resulting curve must satisfy clip
bounds and Bezier validation. Times are not automatically sorted or rescaled.
Track targets are `{"kind": "parameter", "parameter_id": UUID}`,
`{"kind": "part_opacity", "part_id": UUID}`, or
`{"kind": "model", "runtime_id": str}`. Imports may retain
`{"kind": "unresolved", "category": str, "runtime_id": str}` for repair;
motion3 export preserves their runtime IDs. Preview skips their curves and
reports them in snapshot coverage.

| `Edit` operation | Index and mutation rules |
| --- | --- |
| `create_motion_track(motion_id, track)` | Append a track; duplicate track UUIDs fail. |
| `replace_motion_track(motion_id, track)` | Replace the track matching `track["id"]` in place; it must exist. |
| `set_motion_segment(motion_id, track_id, index, segment)` | Replace zero-based `segments[index]`; valid range is `0 <= index < len(segments)`. |
| `insert_motion_segment(motion_id, track_id, index, segment)` | Insert before index; `index == len(segments)` appends. |
| `move_motion_key(motion_id, track_id, index, time, value)` | Key 0 is `initial`; keys 1 through segment count are endpoints of `segments[index - 1]`. Bezier controls do not move. |
| `set_motion_event(motion_id, event)` | Replace the event matching its UUID in place, or append a new event. Event time must be within `[0, duration]`. |
| `remove_motion_track(motion_id, track_id)` | Remove the track and all its points; absence is an error. |
| `remove_motion_event(motion_id, event_id)` | Remove the event; absence is an error. |
| `set_motion_timing(motion_id, duration, fps, looping, fade_in=None, fade_out=None)` | Replace timing metadata. Duration/FPS must be positive and finite; fades are `None` or finite nonnegative seconds. `None` clears a fade override. Existing curves/events must remain valid. |

All timeline helpers validate the complete result. Typical `SdkFailure.code`
values are `MISSING_MOTION`, `MISSING_MOTION_TRACK`, `MISSING_MOTION_SEGMENT`,
`MISSING_MOTION_EVENT`, `DUPLICATE_ID`, `INVALID_MOTION_POINT`,
`INVALID_MOTION_BEZIER`, `INVALID_MOTION_EVENT_TIME`, `INVALID_MOTION_DURATION`,
`INVALID_MOTION_FPS`, and `INVALID_MOTION_FADE`. SDK errors abort the current
edit, so earlier mutations in that transaction are not committed. Timeline
helpers reject unknown imported extensions with `OPAQUE_EDIT_REQUIRES_IMPORT`;
reimport to establish a new baseline. Indices must fit a native unsigned integer;
Python conversion can raise `OverflowError` or `TypeError` before SDK validation.

### Preview state, time and errors

All three preview types capture the document when created. `document_revision`
is that captured revision, not the current session revision. Recreate a preview
after authoring edits. Python snapshots/mappings/lists are detached copies;
Rust snapshot accessors borrow state. IDs in parameter/Part maps are UUIDs.

| API | State semantics |
| --- | --- |
| Expression/Motion `snapshot()` | Read current values without advancing time; Motion events are from the last update. |
| Expression/Motion `set_base_parameter(parameter_id, value)` | Clamp a finite value to the parameter range, store as baseline and reset playback. Retain schedules; Motion clears checkpoints/statistics. |
| Expression/Motion `schedule_expression(expression_id, time)` | Schedule at absolute seconds, no earlier than current time. Ties preserve call order; activation begins on the first due update. |
| Motion `schedule_motion(motion_id, time)` | Same time rules; uses clip/curve fades. For registration overrides use `schedule_motion_entry`. |
| Motion `schedule_parameter_input(parameter_id, time, value)` | Clamp a finite value and schedule an override before Motion on the first due update; retain it across reset/seek. |
| Expression/Motion `reset()` | Restore time zero and initial state from the baseline while retaining schedules. Time-zero activations need `advance(0)` or `seek(0)` to evaluate. Motion also clears cache/statistics. |
| Expression/Motion `advance(dt)` | Finite nonnegative seconds; zero evaluates due activations. Returns a snapshot. |
| Expression/Motion `seek(time)` | Absolute seconds, 60 Hz replay with a partial final step. Seeking zero evaluates one zero-length step. Expression replays from baseline; Motion may restore a canonical checkpoint. |
| Physics `parameters()` / `diagnostics()` | Current values / coverage messages from the last advance, without evaluation. |
| Physics `set_parameter(parameter_id, value)` | Clamp a finite value without resetting particles; this is a temporary input, not a persistent baseline. |
| Physics `advance(dt)` | Finite nonnegative seconds; uses the asset FPS if present, otherwise the supplied delta. Refreshes diagnostics and returns parameter values. |
| Physics `stabilize()` / Motion `stabilize_physics()` | Initialize physics particles/outputs using current values without advancing time or running other stages. Does not seed seek checkpoints or refresh diagnostics. |
| Physics `reset()` | Restore document-default values and initial particles/caches; discard temporary inputs and clear diagnostics. |

Scheduling errors include `INVALID_TIME` (nonfinite/negative time),
`PAST_ACTIVATION`, `MISSING_EXPRESSION`/`MISSING_MOTION`, and
`UNRESOLVED_PARAMETER` for expressions. Motion skips unresolved imported tracks
and reports coverage instead of rejecting scheduling. Baseline edits reject missing
UUIDs or nonfinite values with `EVALUATION_FAILED`. Physics/input setters use
`INVALID_TIME` for nonfinite values and `EVALUATION_FAILED` for missing UUIDs.
Invalid advance deltas or Expression/Motion clock overflow use `INVALID_TIME`.
Seek rejects `time * 60 > 1_000_000` with `SEEK_LIMIT` even if the cache is warm.
These rejected inputs do not alter preview state. Motion event expansion above
one million is rejected with `EVENT_LIMIT` before mutation.

`ExpressionSnapshot` contains `time` (seconds), `parameters` (UUID → value) and
`active_expressions` (queue-order UUIDs, including fading-out entries).
`MotionSnapshot` additionally contains `part_opacity_channels` (virtual Part
controls), `part_opacities` (Pose results), `model_opacity`, `active_motions`,
`fired_events` (`(motion_uuid, event_uuid, text)` tuples) and `coverage` messages.
Repeated activations may repeat an asset UUID in the active lists. Seek returns
events from its final replay step, not all traversed steps; events do not trigger
audio or other side effects. `frame()` includes ancestor Part opacity for Motion;
model opacity remains a separate snapshot channel. Geometry failures raise
`EVALUATION_FAILED`. Standalone Physics has no geometry/frame or seek API.

### Motion registration and seek caching

```python
preview = model.motion_preview()
preview.schedule_motion_entry("Idle", 0, 0.0)  # zero-based model3 group entry
preview.set_seek_cache_budget(16 * 1024 * 1024)  # default; 0 disables caching
preview.seek(60.0)
preview.seek(61.0)
print(preview.seek_cache_stats())  # 60 replay steps if the 60s checkpoint fits
preview.clear_seek_cache()
```

Registration fades override clip fades per activation; explicit track fades retain
precedence. Missing groups/indices raise `MISSING_MOTION_ENTRY`. Sound metadata is
not played. Canonical 60 Hz replay saves complete Motion/Expression/Physics/Pose
checkpoints every second. The budget conservatively estimates retained state,
excluding shared documents and temporary seek state. Oversized checkpoints are
skipped; older checkpoints are evicted first. Schedule edits, base edits and reset
clear the cache. Arbitrary advance/stabilization never seed it.
`SeekCacheStats` exposes `budget_bytes`, `estimated_bytes`, `checkpoints`,
`last_restored_time`, and `last_replayed_steps`. Progress callbacks count remaining
replay work; exact hits call `(0, 0)`. Returning false or raising an exception leaves
both playback and cache unchanged. Standalone Expression preview still replays
from its initial state.


| API | Contract |
| --- | --- |
| `schedule_motion_entry(group: str, index: int, time: float) -> None` | Exact group name, nonnegative zero-based index, absolute time in seconds. Time must be finite, nonnegative and no earlier than the current preview time. Success clears checkpoints/statistics without resetting playback. |
| `set_seek_cache_budget(size_bytes: int) -> None` | Nonnegative byte budget, default 16 MiB; zero disables caching. Every call clears checkpoints/statistics, including calls with the same budget. Preserves playback and schedules. |
| `clear_seek_cache() -> None` | Discards checkpoints and zeros statistics while retaining the configured budget, current playback, baseline values and schedules. |
| `seek_cache_stats() -> SeekCacheStats` | Returns an immutable snapshot without changing playback or the cache. |

Registration scheduling raises `SdkFailure` with `MISSING_MOTION_ENTRY` for an
unknown group/index, `INVALID_TIME` for nonfinite/negative time, `PAST_ACTIVATION`
for past time. Unresolved imported tracks are skipped and reported in coverage. Failure
preserves playback, schedules and cache. Registration fades fall back to clip
fades and then one second when absent. Each activation retains its own settings.
Parameter-curve fade overrides retain precedence. Negative or oversized Python
indices/budgets raise `OverflowError` when converted to a native unsigned integer;
non-integer arguments raise `TypeError`, without modifying state.

| `SeekCacheStats` field | Meaning |
| --- | --- |
| `budget_bytes: int` | Configured retained-memory budget in bytes; zero disables caching. |
| `estimated_bytes: int` | Conservative retained allocation estimate in bytes, at most the budget; excludes shared document/curve data and temporary seek state. |
| `checkpoints: int` | Number of retained complete playback checkpoints. |
| `last_restored_time: float` | Last restored checkpoint time in seconds; zero means replay started from the initial state. |
| `last_replayed_steps: int` | Actual steps in the last successful seek, including a partial tail; zero for an exact cache hit, one for an uncached seek to time zero. |

Clearing/invalidation zeros all statistics except `budget_bytes`; failed or
cancelled seeks preserve them. `advance()` and stabilization do not update the
last-seek statistics. Rust SDK reexports the same `MotionPreview` methods and
`SeekCacheStats`; indices/budgets use `usize`, times use `f32`, replay-step counts
use `u32`, and registration errors use `AnimationError` variants.


`Session.model3_settings()` exposes Groups, Layout, HitAreas, UserData, and
unknown model3 fields. Groups use `{"name": "EyeBlink", "parameters":
[{"kind": "resolved", "object_id": parameter_uuid}]}`; HitAreas use
`{"name": "Head", "mesh": {"kind": "resolved", "object_id": mesh_uuid}}`.
Imported unresolved references use `{"kind": "unresolved", "runtime_id": "..."}`
and block strict export. Layout is a mapping of names to finite numbers.
Known references follow runtime-ID renames and protect referenced objects from
deletion. `Edit.set_model3_settings()` changes supported fields;
`replace_model3_settings()` takes a detached mapping. Managed Sound and
UserData files are stored as project bytes through
`Edit.set_package_attachments({relative_path: bytes})` and read through
`Session.package_attachments()`. Imported missing model3 references appear
in `Session.missing_attachments()`, survive save/open, and block strict
`export_package()` with `MISSING_PACKAGE_ATTACHMENT`. Repair the source
asset or explicitly omit each absent reference using
`Edit.discard_missing_attachment(path)`. Unknown imported JSON fields are
preserved; changing their source namespace or content blocks strict export
until reimport establishes a new baseline.

`Axis(parameter_id, keys)` and `MeshKeyform(keys, positions, appearance,
draw_order)` describe a mesh binding. Its forms must cover the Cartesian
product of all axis keys in one committed edit. `SceneWarpKeyform`,
`SceneRotationKeyform`, and `ScenePartKeyform` correspond to `kind="warp"`,
`"rotation"`, and `"part"` scene bindings. The full record shapes and defaults
are typed in [`python/kasane/__init__.py`](python/kasane/__init__.py).

| Record family | Key fields and use |
| --- | --- |
| `MeshGeometryData`, `MeshDrawingData`, `MeshRecordSpec`, `MeshRecordSnapshot` | Full mesh geometry, drawing state, Part/deformer relationships, and runtime identity. `MeshRecordSnapshot` has `part_id` and `deformer_id` (not `parent_id`). Use the `Spec` to create and a read snapshot to replace. |
| `Appearance`, `MeshProperties`, `MeshPropertiesSnapshot` | Opacity, multiply/screen colors, texture, draw order, blend mode, masks, and visibility flags. |
| `RotationPose`, `RotationData`, `WarpData`, `TransformSnapshot` | Rotation and warp deformer input and snapshots. |
| `OffscreenKeyform`, `OffscreenSpec`, `OffscreenSnapshot` | Offscreen composition settings, masks, and Part keyform mapping. |
| `GlueVertexPair`, `GlueBinding`, `GlueSpec`, `GlueSnapshot` | Mesh vertex pairs, weights, intensity, and optional parameter control. |
| `BlendKeyTableSpec`, `BlendConstraintSpec`, `BlendBindingSpec` | BlendShape setup; corresponding `Snapshot` records are returned on reads. |
| `BlendMeshDelta`, `BlendWarpDelta`, `BlendRotationDelta`, `BlendPartDelta`, `BlendGlueDelta`, `BlendOffscreenDelta` | Target-specific BlendShape keyform deltas. |
| `DrawOrderGroup` | An owner, ordered item IDs, and minimum/maximum order. |

## Evaluation and preview

| API | Result |
| --- | --- |
| `evaluate(values)` | `Evaluation(parameters, drawables)`: sampled parameters and drawable runtime positions. It does not change preview values. |
| `evaluate_snapshot(values)` | `EvaluationSnapshot` with version, source revision, canvas, full drawable render attributes, offscreens, and render plan. |
| `preview_values`, `preview_revision`, `preview_evaluation_count` | Read-only properties of the cached preview state. |
| `set_preview_values(values)`, `set_preview_parameter(name_or_id, value)`, `reset_preview_values()` | Update preview input; return whether it changed. |
| `preview_frame()`, `preview_snapshot()` | Evaluate the current preview input as a short or full snapshot. |

`values` is a mapping from parameter ID or unique display name to a number.
`ParameterSample` reports the requested value, sampled `value`, and whether
it was clamped. Passing the same parameter by ID and name in one mapping
raises `ValueError`. The full snapshot copies geometry arrays, so prefer the
short `evaluate()` when only runtime positions are needed.

## Optional GPU observation

`Observer(width, height, fit_long_side)` requires a wheel built with the
`observe` feature and a working GPU. It can be used as a context manager.

| API | Result |
| --- | --- |
| `observer.observe(session, values=None)` | `ObservedFrame` with `rgba`, `png`, size, version, input hash, sampled parameters, canvas/view, drawable bounds, texture revisions, and adapter information. |
| `frame.save_png(absolute_path)` | Write the PNG bytes to disk. |
| `observer.set_fit_long_side(value)` | Change the view's fitted long side. |
| `observer.observe_run(session, samples, output, focus=())` | Render one or more parameter maps into a unique child of absolute `output`; optionally crop visible drawable IDs. Return `ObservationRun`. |
| `observer.capture_scene(session, values=None)` | Freeze one evaluated frame and all decoded texture bytes in `CapturedScene`; later edits and asset changes do not change it. |
| `observer.capture_scenes(session, samples)` | Freeze 1–64 parameter samples against one detached document snapshot; resolve names there and decode the union of textures once. All returned scenes share a capture ID. |
| `observer.capture_animation_scene(session, preview, apply_model_opacity=False)` | Freeze the preview's actual evaluated animation frame and current snapshot without advancing it; reject a stale preview. |
| `observer.render_scene(scene, *, roi, resolution, padding_canvas=0)` | Rerender the frozen scene at a source-canvas ROI. Return `RenderedSceneView` with an `ObservedFrame`, requested/padded/visible ROI, and `render_digest`. |
| `scene.save_scene(absolute_directory)` | Save a new data-only scene bundle with PNG texture bytes and `scene.json`; refuses an existing directory. |
| `observer.open_scene(absolute_directory)` | Validate hashes/format and open the bundle without a live session or original asset files. |
| `observer.inspect(session, values=None, *, request=RawInspectionRequest(...))` | Return an O1 raw `InspectionPacket` with a frozen scene and one transparent context view. |
| `observer.inspect_animation(session, preview, *, request, apply_model_opacity=False)` | Return the actual animation frame in the same raw packet form, with current operation identity. |
| `observer.render(packet, *, request)` | Append a raw ROI view to a new packet value with the same capture ID, without reading a session or source asset. |
| `packet.save(absolute_directory, profile="analysis")` | Save a new packet with checked artifact hashes; `report` stores PNG/metadata, `analysis` also stores evaluated geometry and raw RGBA, `scene` also stores frozen textures for rerendering. |
| `kasane.open_inspection_packet(absolute_directory)` / `observer.open(...)` | Validate and open a saved packet. Report/analysis profiles can be read from a CPU-only wheel; scene profile requires the observe wheel. |

These capture, raw packet and ROI methods complete O1's frozen-scene gate.
`CapturedScene.capture_id` is unique to an acquisition and survives a v2 scene
bundle round trip. `CapturedScene.scene_digest` hashes canonical frozen scene
content, including metadata and texture descriptors/content hashes, while
excluding the acquisition ID, live animation operation identity, and
session/evaluation revision counters.
`RenderedSceneView.render_digest` adds the explicit ROI, dimensions, padding,
and fixed raw context render policy. The digest identifies inputs and policy,
not GPU pixel equivalence across adapters. The legacy `ObservedFrame.input_sha256`
keeps its original behavior and is separate from these digests. Opening a v1
bundle computes the scene digest and assigns a new capture ID.
They currently render the legacy raw transparent pixel policy. The O1 packet
uses `RawInspectionRequest`; the full `InspectionRequest`, labels, display
backgrounds, query/comparison APIs, batch layout and complete report v2 remain
later-stage work. `packet.capabilities` explicitly marks those channels
unavailable. The `analysis` profile preserves query inputs but does not yet
implement the O2 geometry/probe query. A saved scene contains an evaluated frame,
not a resumable animation preview. `CapturedScene.source` reports
`source_kind`, and for animation, current snapshot, host Model opacity policy
and `history_status="not_recorded"`. Snapshot events cover only the most recent
update, not a full interval journal. `source.operation` records preview ID,
successful operation sequence, kind and actual time; it is not a full playback
recipe or a semantic state digest. `CapturedScene.authoring` returns frozen
mesh/Part/transform/binding and offscreen/glue/blend source records from the
same document revision as the frame; it does not yet report selected keyform
interpolation weights. `CapturedScene.metadata.snapshot_clone_ns` records the
cost of copying the authoring document for the read-only capture.

```python
with kasane.Observer(256, 256, 256) as observer:
    scene = observer.capture_scene(session, {"Shift": 0.25})
    view = observer.render_scene(
        scene, roi=(20, 20, 60, 60), resolution=(1024, 768),
        padding_canvas=4,
    )
    scene.save_scene(Path("/absolute/path/to/new-scene"))
    point_on_canvas = view.image_to_canvas((512.5, 384.5))
    reopened = observer.open_scene(Path("/absolute/path/to/new-scene"))
    assert reopened.capture_id == scene.capture_id
    assert reopened.scene_digest == scene.scene_digest
    assert observer.render_scene(
        reopened, roi=(20, 20, 60, 60), resolution=(1024, 768),
        padding_canvas=4,
    ).frame.rgba == view.frame.rgba

    request = kasane.RawInspectionRequest((20, 20, 60, 60), (1024, 768), 4)
    packet = observer.inspect_scene(scene, request=request)
    receipt = packet.save(Path("/absolute/path/to/new-packet"), profile="scene")
    reopened_packet = observer.open(receipt.directory)
    assert observer.render(reopened_packet, request=request).capture_id == packet.capture_id
```

`ObservationRun.directory` (also `output`) is the actual run directory;
`report`, `frames`, `crops`, and `contact_sheet` are paths under it. A failed
run writes a failure report and attaches `run_directory` to the exception.
GPU failures use `ObservationFailure.code` and, when applicable, `asset_id`.
`ObservedFrame` exposes `width`, `height`, `rgba`, and `png`; it has no `size`
field. Observer image pixels, evaluated runtime coordinates, and editable
source or parent-local mesh coordinates are different spaces. For root meshes,
convert runtime to source pixels with the canvas origin and
`pixels_per_unit`; for meshes under a transform, inspect `geometry.space` and
`geometry.parent_id` before changing keyforms.

## Errors and command-line runner

`SdkFailure` exposes `code`, `operation`, `object_ids`, `field_path`,
`expected_version`, `actual_version`, and `referrers`. Use these structured
fields instead of matching message text. Python argument errors may raise
`TypeError` or `ValueError`; an error inside an `Edit` still aborts that edit.

`python -m kasane run /absolute/script.py --report /absolute/report.json`
runs one script and writes a JSON report containing captured stdout/stderr,
exception details, and live session versions. The runner does not save a
project automatically.

Projects now write format v6 and read v1–v6. The v5 migration binds raw model3
Groups/HitAreas to stable UUIDs using their saved namespace. Older readers must
reject v6. Python callers that passed capitalized wire-style Groups/HitAreas to
`set_model3_settings()` must use the typed records shown above; model3 package
import/export still uses the standard Live2D wire format.
