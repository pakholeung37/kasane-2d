extends SceneTree

const DOC_ID = "11111111-1111-4111-8111-111111111111"
const ASSET_ID = "22222222-2222-4222-8222-222222222222"
const MESH_ID = "33333333-3333-4333-8333-333333333333"
const PARAM_ID = "44444444-4444-4444-8444-444444444444"
const BINDING_ID = "55555555-5555-4555-8555-555555555555"
const PART_ID = "66666666-6666-4666-8666-666666666666"
const DEFORMER_ID = "77777777-7777-4777-8777-777777777777"

var failures = []
var checks = 0
var files = ""

# Signal callback tracking
var changed_signal_count = 0
var signal_reentrant_summary = {}
var signal_reentrant_frame = {}
var signal_reentrant_error = ""

func check(condition: bool, message: String):
    checks += 1
    if not condition:
        failures.append(message)
        push_error(message)

func _initialize():
    call_deferred("run")

func _on_doc_changed(change: Dictionary, doc: RefCounted):
    changed_signal_count += 1
    # Re-entrantly access DocumentBridge during signal emission
    var summary = doc.get_document_summary()
    if not summary.has("revision"):
        signal_reentrant_error = "Failed to get summary during changed signal"
    signal_reentrant_summary = summary

    # Also test read access to frame
    var frame = doc.get_frame()
    if not frame.ok:
        signal_reentrant_error = "Failed to get frame during changed signal"
    signal_reentrant_frame = frame

func run():
    files = OS.get_cmdline_user_args()[0] if OS.get_cmdline_user_args().size() > 0 else "res://"
    var io = ClassDB.instantiate("KasaneProjectIO")
    var textures = ClassDB.instantiate("KasaneTextureStore")

    # Prepare minimal PNG texture on disk
    var img_path = files + "/lifecycle-texture.png"
    var test_img = Image.create(16, 16, false, Image.FORMAT_RGBA8)
    test_img.fill(Color.BLUE)
    test_img.save_png(img_path)
    check(textures.set_texture(ASSET_ID, ImageTexture.create_from_image(test_img)).ok, "Register texture")

    # ========================================================
    # Test 1: Setup Document and Verify Handle Invalidation
    # ========================================================
    var doc = ClassDB.instantiate("KasaneDocumentBridge")
    check(doc.initialize(DOC_ID, Vector2(640, 480), Vector2(271, 193), 100).ok, "Init doc")
    check(doc.add_image_asset(ASSET_ID, "texture", img_path, 16, 16).ok, "Add asset")

    check(doc.write_part({"id": PART_ID, "runtime_id": "RootPart", "name": "root", "parent_id": "", "enabled": true, "draw_order": 0}).ok, "Create part")
    var app = {"opacity": 1.0, "multiply": [1.0, 1.0, 1.0], "screen": [0.0, 0.0, 0.0]}
    check(doc.write_transform({
        "id": DEFORMER_ID,
        "runtime_id": "Rot",
        "name": "rot",
        "part_id": PART_ID,
        "parent_id": "",
        "kind": 1,
        "base_angle": 0.0,
        "rows": 0,
        "columns": 0,
        "quad": false,
        "enabled": true,
        "points": [],
        "rotation": {"origin": [271.0, 193.0], "angle": 0.0, "scale": 1.0, "reflect_x": false, "reflect_y": false},
        "appearance": app,
    }).ok, "Create deformer")

    var positions = PackedVector2Array([Vector2(10, 10), Vector2(40, 12), Vector2(30, 40)])
    var mesh = {
        "id": MESH_ID,
        "runtime_id": "ArtMesh",
        "name": "mesh",
        "texture_asset_id": ASSET_ID,
        "part_id": PART_ID,
        "deformer_id": DEFORMER_ID,
        "vertex_ids": PackedInt64Array([1, 2, 3]),
        "base_positions": positions,
        "uvs": PackedVector2Array([Vector2(0, 0), Vector2(1, 0), Vector2(0.5, 1)]),
        "triangles": PackedInt64Array([1, 2, 3]),
    }
    check(doc.create_mesh(mesh).ok, "Create mesh")

    var mesh_handle = doc.get_mesh(MESH_ID)
    var deformer_handle = doc.get_deformer(DEFORMER_ID)
    check(mesh_handle.is_valid(), "Initial mesh handle is valid")
    check(deformer_handle.is_valid(), "Initial deformer handle is valid")
    check(mesh_handle.snapshot().ok, "Mesh handle snapshot succeeds")
    check(deformer_handle.snapshot().ok, "Deformer handle snapshot succeeds")

    # Save project to disk
    var proj_path = files + "/lifecycle-project.json"
    check(io.save_project(doc, proj_path).ok, "Save project for handle test")

    # Reopening project into doc MUST advance generation and invalidate old handles
    check(io.open_project(doc, proj_path).ok, "Reopen project advances generation")
    check(not mesh_handle.is_valid(), "Old mesh handle is invalid after project reopen")
    check(not deformer_handle.is_valid(), "Old deformer handle is invalid after project reopen")

    var stale_mesh_snap = mesh_handle.snapshot()
    check(not stale_mesh_snap.ok and stale_mesh_snap.code == "STALE_HANDLE", "Stale mesh handle returns STALE_HANDLE code")
    var stale_def_snap = deformer_handle.snapshot()
    check(not stale_def_snap.ok and stale_def_snap.code == "STALE_HANDLE", "Stale deformer handle returns STALE_HANDLE code")

    # ========================================================
    # Test 2: Preview Node Destruction and Multi-Preview Isolation
    # ========================================================
    var preview_1 = ClassDB.instantiate("KasaneDocumentPreview")
    var preview_2 = ClassDB.instantiate("KasaneDocumentPreview")
    var preview_3 = ClassDB.instantiate("KasaneDocumentPreview")
    root.add_child(preview_1)
    root.add_child(preview_2)
    root.add_child(preview_3)

    for p in [preview_1, preview_2, preview_3]:
        p.set_texture_store(textures)
        p.set_document(doc)
        check(p.get_last_result().ok, "Multiple previews connect cleanly")

    # Verify mesh view in preview
    var mv_1 = preview_1.get_mesh_view(MESH_ID)
    check(mv_1 != null, "Mesh view exists in preview 1")

    # Destroy preview_1 with free()
    preview_1.free()
    check(preview_2.get_last_result().ok, "Preview 2 unaffected after preview 1 freed")
    check(preview_3.get_last_result().ok, "Preview 3 unaffected after preview 1 freed")

    # Edit document while remaining previews are active
    check(doc.rename_mesh(MESH_ID, "mesh_renamed").ok, "Rename mesh with active previews")
    check(preview_2.get_last_result().ok, "Preview 2 refreshes cleanly on doc edit")
    check(preview_3.get_last_result().ok, "Preview 3 refreshes cleanly on doc edit")

    # Destroy preview_2 with queue_free() and await frames
    preview_2.queue_free()
    await process_frame
    await process_frame

    check(preview_3.get_last_result().ok, "Preview 3 maintains state after preview 2 queue_freed")
    preview_3.free()

    # Document remains fully functional
    var fresh_handle = doc.get_mesh(MESH_ID)
    check(fresh_handle.is_valid(), "Document still produces valid handles after all previews freed")

    # ========================================================
    # Test 3: Signal Re-entrancy Safety
    # ========================================================
    changed_signal_count = 0
    signal_reentrant_error = ""
    var callable = Callable(self, "_on_doc_changed").bind(doc)
    doc.changed.connect(callable)

    # Trigger doc mutation -> causes doc.changed emission -> invokes _on_doc_changed -> queries doc
    check(doc.rename_mesh(MESH_ID, "mesh_signal_test").ok, "Mutate mesh to trigger signal")
    check(changed_signal_count > 0, "doc.changed signal fired")
    check(signal_reentrant_error.is_empty(), "Re-entrant document query inside signal caused no error")
    check(signal_reentrant_summary.has("revision"), "Re-entrant summary has revision")

    doc.changed.disconnect(callable)

    # ========================================================
    # Test 4: Continuous Load/Unload Stress Cycle (50 iterations)
    # ========================================================
    var stress_doc = ClassDB.instantiate("KasaneDocumentBridge")
    var stress_preview = ClassDB.instantiate("KasaneDocumentPreview")
    root.add_child(stress_preview)
    stress_preview.set_texture_store(textures)

    var stress_start_objects = Performance.get_monitor(Performance.OBJECT_COUNT)

    for i in 50:
        var open_res = io.open_project(stress_doc, proj_path)
        if not open_res.ok:
            failures.append("Stress iteration %d open failed: %s" % [i, str(open_res)])
            break

        stress_preview.set_document(stress_doc)
        var preview_res = stress_preview.get_last_result()
        if not preview_res.ok:
            failures.append("Stress iteration %d preview failed: %s" % [i, str(preview_res)])
            break

        # Mutate document
        stress_doc.rename_mesh(MESH_ID, "mesh_iter_%d" % i)

        # Disconnect document
        stress_preview.set_document(null)

    stress_preview.free()
    stress_doc = null

    var stress_end_objects = Performance.get_monitor(Performance.OBJECT_COUNT)
    check(abs(stress_end_objects - stress_start_objects) < 25, "Object count remains bounded after 50 stress iterations")

    # Report results
    var report = {
        "status": "passed" if failures.is_empty() else "failed",
        "checks": checks,
        "failures": failures,
        "stress_iterations": 50,
        "object_delta": stress_end_objects - stress_start_objects,
    }

    var report_file = FileAccess.open(files + "/lifecycle-report.json", FileAccess.WRITE)
    if report_file:
        report_file.store_string(JSON.stringify(report, "  "))
        report_file.close()

    print(JSON.stringify(report))
    quit(0 if failures.is_empty() else 1)
