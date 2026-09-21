extends SceneTree

const DOC = "11111111-1111-4111-8111-111111111111"
const ASSET = "22222222-2222-4222-8222-222222222222"
const MESH = "33333333-3333-4333-8333-333333333333"
const PARAM = "44444444-4444-4444-8444-444444444444"
const BINDING = "55555555-5555-4555-8555-555555555555"
var failures = []
var checks = 0
var files = ""

func check(condition, message):
    checks += 1
    if not condition:
        failures.append(message)
        push_error(message)

func _initialize():
    call_deferred("run")

func shifted(points, offset):
    var output = PackedVector2Array()
    for p in points:
        output.append(p + Vector2(offset, 0))
    return output

func run():
    files = OS.get_cmdline_user_args()[0]
    var doc = ClassDB.instantiate("KasaneDocumentBridge")
    check(doc is RefCounted and not doc is Node, "Document must exist without a scene node")
    check(not doc.has_method("open_project") and not doc.has_method("rebuild_preview"), "Document must not own I/O or preview")
    check(doc.initialize(DOC, Vector2(100, 100), Vector2(50, 50), 100).ok, "initialize")
    check(doc.add_image_asset(ASSET, "texture", (files + "/boundary-texture.png"), 8, 8).ok, "Source metadata must not load texture")
    var positions = PackedVector2Array([Vector2(10, 10), Vector2(40, 12), Vector2(30, 40)])
    var mesh = {"id": MESH, "runtime_id": "ArtMesh", "name": "mesh", "texture_asset_id": ASSET,
        "vertex_ids": PackedInt64Array([71, 4, 91]), "base_positions": positions,
        "uvs": PackedVector2Array([Vector2(0, 0), Vector2(1, 0), Vector2(0.4, 1)]),
        "triangles": PackedInt64Array([71, 4, 91])}
    check(doc.create_mesh(mesh).ok, "Mesh edits without resources or scene")
    var handle = doc.get_mesh(MESH)
    check(handle.is_valid(), "Data handle owns no scene node")
    var param = {"id": PARAM, "runtime_id": "ParamX", "name": "x", "minimum": -1, "maximum": 1, "default_value": 0}
    check(doc.create_parameter(param).ok, "create parameter")
    var binding = {"id": BINDING, "mesh_id": MESH, "axes": [{"parameter_id": PARAM, "keys": [-1, 0, 1]}],
        "keyforms": [{"keys": [-1], "positions": shifted(positions, -10)},
                     {"keys": [0], "positions": positions}, {"keys": [1], "positions": shifted(positions, 20)}]}
    check(doc.write_binding(binding).ok, "create keyforms")
    var io = ClassDB.instantiate("KasaneProjectIO")
    var path = (files + "/boundary-project.json")
    var fixture_image = Image.create(8, 8, false, Image.FORMAT_RGBA8)
    fixture_image.fill(Color.RED)
    fixture_image.save_png((files + "/boundary-texture.png"))
    check(io.save_project(doc, path).ok, "Save directory project with verified PNG")
    check(not doc.get_document_summary().modified, "saved state")
    var revision = doc.get_document_summary().revision
    var frame = doc.set_preview_values({PARAM: 0.5})
    check(frame.ok and is_equal_approx(frame.drawables[0].positions[0].x, -0.3), "Shared evaluator midpoint")
    check(doc.get_document_summary().revision == revision and not doc.get_document_summary().modified, "Preview must not persist or dirty document")
    var clamped = doc.set_preview_values({PARAM: 10})
    check(clamped.ok and clamped.parameters[0].clamped and clamped.parameters[0].value == 1, "Preview reports clamped actual value")
    check(not doc.set_preview_values({PARAM: NAN}).ok, "Reject non-finite preview")
    check(doc.get_frame().parameters[0].value == 1, "Rejected preview preserves previous state")
    doc.set_preview_values({PARAM: 0.5})

    var textures = ClassDB.instantiate("KasaneTextureStore")
    var preview_a = ClassDB.instantiate("KasaneDocumentPreview")
    var preview_b = ClassDB.instantiate("KasaneDocumentPreview")
    root.add_child(preview_a)
    root.add_child(preview_b)
    for preview in [preview_a, preview_b]:
        preview.set_texture_store(textures)
        preview.set_document(doc)
        check(preview.get_last_result().ok, "Packaged PNG loads directly without import cache")
    var geometry_creations_before: int = preview_a.get_render_stats().creations
    var geometry_uploads_before: int = preview_a.get_render_stats().uploads
    var evaluations_before: int = doc.get_parameter_samples().evaluation_count
    var preview_revision_before: int = doc.get_parameter_samples().preview_revision
    var callback_counts := []
    var on_preview = func(): callback_counts.append(doc.get_parameter_samples().evaluation_count)
    doc.preview_changed.connect(on_preview)
    var parameter_result: Dictionary = doc.set_preview_parameter(PARAM, 0.75)
    check(parameter_result.ok and not parameter_result.has("drawables"), "Single parameter update returns lightweight samples")
    check(parameter_result.evaluation_count == evaluations_before + 1, "Two previews share one candidate evaluation")
    check(preview_a.get_render_stats().creations == geometry_creations_before, "Parameter changes reuse static render geometry")
    check(preview_a.get_render_stats().uploads > geometry_uploads_before, "Parameter changes still upload dynamic positions")
    check(callback_counts == [evaluations_before + 1], "Signal readers observe the committed cached frame")
    doc.get_frame()
    doc.evaluate_mesh(MESH)
    preview_a.refresh_geometry()
    preview_b.refresh_geometry()
    check(doc.get_parameter_samples().evaluation_count == evaluations_before + 1, "Frame, mesh and camera refresh reuse evaluation")
    doc.set_preview_parameter(PARAM, 0.75)
    check(callback_counts.size() == 1 and doc.get_parameter_samples().preview_revision == preview_revision_before + 1, "Identical request does not evaluate or notify")
    check(not doc.set_preview_parameter(PARAM, NAN).ok, "Single parameter update rejects NaN")
    check(doc.get_parameter_samples().parameters[0].requested == 0.75, "Rejected request preserves committed parameter")
    doc.preview_changed.disconnect(on_preview)
    var reset_result: Dictionary = doc.reset_preview_values()
    check(reset_result.ok and not reset_result.has("drawables") and reset_result.parameters[0].value == 0.0, "Reset uses lightweight samples and defaults")
    doc.set_preview_values({PARAM: 0.5})
    var rename_evaluations: int = doc.get_parameter_samples().evaluation_count
    var rename_submission: int = preview_a.get_last_result().submission_id
    check(doc.rename_mesh(MESH, "renamed while texture missing").ok, "Preview failure cannot fail a committed source edit")
    check(doc.get_parameter_samples().evaluation_count == rename_evaluations, "Name edit does not evaluate")
    check(preview_a.get_last_result().submission_id == rename_submission, "Name edit does not submit rendering")
    check(doc.get_frame().revision == doc.get_document_summary().revision, "Cached frame reports current document revision separately")
    check(doc.begin_action("Name only").ok and doc.rename_mesh(MESH, "name action").ok and doc.end_action().ok, "Name-only action")
    check(doc.undo().ok and doc.redo().ok, "Name-only undo and redo")
    check(doc.get_parameter_samples().evaluation_count == rename_evaluations and doc.get_parameter_samples().parameters[0].requested == 0.5, "Name history preserves preview values and evaluated frame")
    check(preview_a.get_last_result().submission_id == rename_submission, "Name history does not submit rendering")


    var image = Image.create(8, 8, false, Image.FORMAT_RGBA8)
    image.fill(Color(1, 0, 0, 1))
    check(textures.set_texture(ASSET, ImageTexture.create_from_image(image)).ok, "Supply external resource")
    for preview in [preview_a, preview_b]:
        check(preview.get_last_result().ok, "Two independent previews consume same document")
        check(is_equal_approx(preview.get_mesh_view(MESH).get_positions_snapshot()[0].x, 20), "Preview converts runtime coordinates at presentation boundary")
    check(doc.set_mesh_keyform(BINDING, PackedFloat32Array([0]), shifted(positions, 8)).ok, "Edit specified keyform")
    check(is_equal_approx(preview_b.get_mesh_view(MESH).get_positions_snapshot()[0].x, 24), "Keyform edit updates preview without file export")
    preview_a.free()
    check(handle.is_valid() and doc.rename_mesh(MESH, "still editable").ok, "Destroying preview does not destroy Document or handles")
    check(preview_b.get_last_result().ok, "Remaining preview keeps working")
    check(io.save_project(doc, path).ok, "Persist parameter and keyform data")
    var clone = ClassDB.instantiate("KasaneDocumentBridge")
    check(io.open_project(clone, path).ok, "Open packaged source without a preview")
    check(clone.get_document_summary().bindings.size() == 1, "Binding survives source round trip")
    check(clone.get_frame().parameters[0].value == 0, "Preview values are not persisted")
    check(is_equal_approx(clone.get_frame().drawables[0].positions[0].x, -0.32), "Edited keyform survives source round trip")
    check(not clone.get_document_summary().modified, "Opened source is clean")
    var old_handle = clone.get_mesh(MESH)
    check(io.open_project(clone, path).ok and not old_handle.is_valid(), "Replacing source invalidates old handles")
    var valid_handle = clone.get_mesh(MESH)
    var invalid = JSON.parse_string(FileAccess.get_file_as_string(path))
    invalid.document.bindings[0].keyforms.pop_back()
    var file = FileAccess.open((files + "/invalid.json"), FileAccess.WRITE)
    file.store_string(JSON.stringify(invalid))
    file.close()
    revision = clone.get_document_summary().revision
    check(not io.open_project(clone, (files + "/invalid.json")).ok, "Reject incomplete keyforms on open")
    check(clone.get_document_summary().revision == revision and valid_handle.is_valid(), "Failed open preserves live source and handles")
    var deletion = clone.erase_object(PARAM)
    check(not deletion.ok and BINDING in deletion.referrers, "Reference-aware deletion")
    check(clone.erase_object(BINDING).ok and clone.erase_object(PARAM).ok, "Explicit unbind allows deletion")
    check(clone.get_frame().parameters.is_empty(), "Evaluation rebuilt after deletion")
    check(doc.begin_transaction().ok, "Begin transaction")
    check(not io.open_project(doc, path).ok, "Open must not replace an active transaction")
    check(doc.cancel_transaction().ok, "Cancel transaction")
    # Formal scene source roundtrip, including all appearance and pose channels.
    var part_id = "66666666-6666-4666-8666-666666666666"
    var warp_id = "77777777-7777-4777-8777-777777777777"
    var rotation_id = "88888888-8888-4888-8888-888888888888"
    var scene_binding_id = "99999999-9999-4999-8999-999999999999"
    check(clone.write_part({"id":part_id,"runtime_id":"Part","name":"part","parent_id":"","enabled":true,"draw_order":3}).ok, "create formal Part")
    var appearance = {"opacity":0.8,"multiply":[0.8,0.9,1.0],"screen":[0.1,0.2,0.05]}
    var pose = {"origin":[0.4,0.6],"angle":17,"scale":0.9,"reflect_x":true,"reflect_y":false}
    var warp = {"id":warp_id,"runtime_id":"Warp","name":"warp","part_id":part_id,"parent_id":"","kind":0,"base_angle":0,"rotation":pose,"rows":1,"columns":1,"quad":false,"enabled":true,"points":[[10,90],[90,85],[15,10],[95,15]],"appearance":appearance}
    check(clone.write_transform(warp).ok, "create formal Warp")
    var rotation = warp.duplicate(true)
    rotation.id = rotation_id
    rotation.runtime_id = "Rotation"
    rotation.parent_id = warp_id
    rotation.kind = 1
    rotation.points = []
    check(clone.write_transform(rotation).ok, "create nested Rotation")
    var typed_warp = warp.duplicate(true)
    typed_warp.erase("rotation")
    typed_warp.erase("base_angle")
    check(clone.write_transform(typed_warp, true).ok, "Warp requires no rotation-only fields")
    var typed_rotation = rotation.duplicate(true)
    for key in ["rows", "columns", "quad", "points"]:
        typed_rotation.erase(key)
    check(clone.write_transform(typed_rotation, true).ok, "Rotation requires no Warp-only fields")
    var invalid_grid = typed_warp.duplicate(true)
    invalid_grid.rows = 1.5
    check(not clone.write_transform(invalid_grid, true).ok, "Fractional Warp dimensions are rejected")

    var properties = {"part_id":part_id,"deformer_id":rotation_id,"appearance":appearance,"draw_order":9,"blend_mode":2,"enabled":true,"double_sided":true,"inverted_mask":true,"masks":[]}
    check(clone.set_mesh_properties(MESH, properties).ok, "mesh drawing properties and formal parent")
    check(clone.create_parameter(param).ok, "formal binding parameter")
    var form_a = {"keys":[-1],"rotation":pose,"appearance":appearance}
    var form_b = form_a.duplicate(true)
    form_b.keys = [1]
    form_b.rotation.angle = -23
    check(clone.write_scene_binding({"id":scene_binding_id,"target_id":rotation_id,"axes":[{"parameter_id":PARAM,"keys":[-1,1]}],"keyforms":[form_a,form_b]}).ok, "formal scene keyforms")
    var mesh_snapshot = clone.get_mesh_snapshot(MESH)
    check(mesh_snapshot.properties.part_id == part_id and mesh_snapshot.properties.deformer_id == rotation_id and mesh_snapshot.properties.blend_mode == 2 and is_equal_approx(mesh_snapshot.properties.appearance.opacity, 0.8), "mesh snapshot exposes drawing properties")
    check(clone.replace_mesh(mesh_snapshot).ok and clone.get_mesh_snapshot(MESH).properties == mesh_snapshot.properties, "geometry replacement preserves formal properties")
    check(clone.get_document_summary().transforms.size() == 2 and clone.get_document_summary().scene_bindings.size() == 1, "formal source traversal")
    check(io.save_project(clone, (files + "/formal.json")).ok, "save formal source")
    var formal = ClassDB.instantiate("KasaneDocumentBridge")
    check(io.open_project(formal, (files + "/formal.json")).ok, "reopen formal source")
    for value in [-1, 0, 1]:
        var before = clone.set_preview_values({PARAM:value})
        var after = formal.set_preview_values({PARAM:value})
        check(before.ok and after.ok and before.drawables == after.drawables, "formal pose/color roundtrip sample " + str(value))
    var cycle = warp.duplicate(true)
    cycle.parent_id = rotation_id
    revision = clone.get_document_summary().revision
    check(not clone.write_transform(cycle, true).ok, "reject formal transform cycle")
    check(clone.get_document_summary().revision == revision, "rejected cycle is atomic")
    check(not clone.erase_object(warp_id).ok, "formal reference-aware deletion")
    # Engine-independent delta history, observed through the actual Godot boundary.
    var history_before: Dictionary = doc.get_mesh_snapshot(MESH)
    var history_revision: int = doc.get_document_summary().revision
    var history_steps_before: int = doc.get_history_state().undo_steps
    var history_notifications := []
    var history_observer = func(_change): history_notifications.append(doc.get_history_state())
    doc.changed.connect(history_observer)
    check(doc.begin_action("name and vertex").ok, "Begin native history group")
    check(doc.rename_mesh(MESH, "delta group").ok, "Record grouped name")
    var vertex_id: int = history_before.vertex_ids[0]
    var original_position: Vector2 = history_before.base_positions[0]
    check(doc.set_vertex_positions(MESH, PackedInt64Array([vertex_id]), PackedVector2Array([original_position + Vector2(1, 2)])).ok, "Record grouped vertex")
    check(doc.set_vertex_positions(MESH, PackedInt64Array([vertex_id]), PackedVector2Array([original_position + Vector2(3, 4)])).ok, "Merge repeated vertex write")
    check(doc.end_action().ok and doc.get_history_state().undo_steps == history_steps_before + 1, "Group creates one history entry")
    check(doc.get_history_state().estimated_bytes < 2048, "History contains field deltas only")
    var history_edited: Dictionary = doc.get_mesh_snapshot(MESH)
    check(doc.undo().ok, "Undo grouped fields")
    var history_undone: Dictionary = doc.get_mesh_snapshot(MESH)
    check(history_undone.name == history_before.name and history_undone.base_positions == history_before.base_positions, "Undo restores both initial values")
    check(doc.get_document_summary().revision > history_revision, "Undo advances revision")
    check(preview_b.get_last_result().ok, "Undo refreshes remaining preview")
    check(doc.redo().ok and doc.get_mesh_snapshot(MESH).base_positions == history_edited.base_positions, "Redo restores final positions")
    check(doc.begin_action("cancel").ok and doc.rename_mesh(MESH, "cancelled").ok, "Begin cancelable group")
    check(doc.cancel_action().ok and doc.get_mesh_snapshot(MESH).name == "delta group", "Cancel restores group without snapshots")
    check(history_notifications.size() >= 6, "History notifications allow reentrant state reads")
    doc.changed.disconnect(history_observer)
    check(doc.begin_action("unsupported").ok, "Begin unsupported edit group")
    var barrier: Dictionary = doc.replace_mesh(doc.get_mesh_snapshot(MESH))
    check(barrier.ok and barrier.get("history_warning") == "HISTORY_UNSUPPORTED_EDIT", "Unsupported edit explicitly clears history")
    check(doc.get_history_state().undo_steps == 0 and not doc.cancel_action().ok, "History barrier abandons pending group")
    preview_b.free()
    print(JSON.stringify({"status": "passed" if failures.is_empty() else "failed", "checks": checks, "failures": failures, "gpu": "not_run"}))
    quit(0 if failures.is_empty() else 1)
