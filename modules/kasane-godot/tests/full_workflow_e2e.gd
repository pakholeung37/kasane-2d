extends SceneTree

const DOC_ID = "aaaaaaaa-1111-4111-8111-111111111111"
const ASSET_ID = "bbbbbbbb-2222-4222-8222-222222222222"
const PART_ID = "cccccccc-3333-4333-8333-333333333333"
const ROT_ID = "dddddddd-4444-4444-8444-444444444444"
const WARP_ID = "eeeeeeee-5555-4555-8555-555555555555"
const MESH_ID = "ffffffff-6666-4666-8666-666666666666"
const PARAM_ID = "00000000-7777-4777-8777-777777777777"
const BINDING_ID = "11111111-8888-4888-8888-888888888888"

var failures = []
var checks = 0
var files = ""

func check(condition: bool, message: String):
    checks += 1
    if not condition:
        failures.append(message)
        push_error(message)

func _initialize():
    call_deferred("run")

func run():
    files = OS.get_cmdline_user_args()[0] if OS.get_cmdline_user_args().size() > 0 else "res://"
    var io = ClassDB.instantiate("KasaneProjectIO")
    var textures = ClassDB.instantiate("KasaneTextureStore")

    # Step 1: Create verified PNG asset
    var asset_path = files + "/e2e-texture.png"
    var img = Image.create(32, 32, false, Image.FORMAT_RGBA8)
    img.fill(Color(0.2, 0.6, 0.8, 1.0))
    img.save_png(asset_path)
    check(textures.set_texture(ASSET_ID, ImageTexture.create_from_image(img)).ok, "Register texture")

    # Step 2: New Project & Initialize Document
    var doc = ClassDB.instantiate("KasaneDocumentBridge")
    check(doc.initialize(DOC_ID, Vector2(1280, 720), Vector2(640, 360), 100).ok, "Init doc")
    check(doc.add_image_asset(ASSET_ID, "texture_main", asset_path, 32, 32).ok, "Add image asset")

    # Step 3: Create Parts & Deformers
    check(doc.write_part({
        "id": PART_ID,
        "runtime_id": "BodyPart",
        "name": "Body Part",
        "parent_id": "",
        "enabled": true,
        "draw_order": 0.0,
    }).ok, "Create Part")

    var app = {"opacity": 1.0, "multiply": [1.0, 1.0, 1.0], "screen": [0.0, 0.0, 0.0]}
    var identity_pose = {"origin": [640.0, 360.0], "angle": 0.0, "scale": 1.0, "reflect_x": false, "reflect_y": false}

    check(doc.write_transform({
        "id": ROT_ID,
        "runtime_id": "HeadRot",
        "name": "Head Rotation",
        "part_id": PART_ID,
        "parent_id": "",
        "kind": 1,
        "base_angle": 0.0,
        "rows": 0,
        "columns": 0,
        "quad": false,
        "enabled": true,
        "points": [],
        "rotation": identity_pose,
        "appearance": app,
    }).ok, "Create Rotation Deformer")

    check(doc.write_transform({
        "id": WARP_ID,
        "runtime_id": "FaceWarp",
        "name": "Face Warp",
        "part_id": PART_ID,
        "parent_id": ROT_ID,
        "kind": 0,
        "base_angle": 0.0,
        "rows": 1,
        "columns": 1,
        "quad": true,
        "enabled": true,
        "points": [
            [-50.0, 50.0], [50.0, 50.0],
            [-50.0, -50.0], [50.0, -50.0],
        ],
        "rotation": identity_pose,
        "appearance": app,
    }).ok, "Create Warp Deformer")

    # Step 4: Create Mesh & Parameter & Keyforms
    var base_pos = PackedVector2Array([
        Vector2(-30, 30), Vector2(-30, -30),
        Vector2(30, -30), Vector2(30, 30),
    ])
    check(doc.create_mesh({
        "id": MESH_ID,
        "runtime_id": "ArtMeshE2E",
        "name": "E2E Mesh",
        "texture_asset_id": ASSET_ID,
        "part_id": PART_ID,
        "deformer_id": WARP_ID,
        "vertex_ids": PackedInt64Array([1, 2, 3, 4]),
        "base_positions": base_pos,
        "uvs": PackedVector2Array([
            Vector2(0, 0), Vector2(0, 1),
            Vector2(1, 1), Vector2(1, 0),
        ]),
        "triangles": PackedInt64Array([1, 2, 3, 1, 3, 4]),
        "draw_order": 1.0,
    }).ok, "Create Mesh")

    check(doc.create_parameter({
        "id": PARAM_ID,
        "runtime_id": "ParamAngle",
        "name": "Angle Parameter",
        "minimum": -1.0,
        "maximum": 1.0,
        "default_value": 0.0,
        "decimal_places": 4,
    }).ok, "Create Parameter")

    var shifted_pos = PackedVector2Array([
        Vector2(-20, 35), Vector2(-20, -25),
        Vector2(40, -25), Vector2(40, 35),
    ])
    check(doc.write_binding({
        "id": BINDING_ID,
        "mesh_id": MESH_ID,
        "axes": [{"parameter_id": PARAM_ID, "keys": [-1.0, 0.0, 1.0]}],
        "keyforms": [
            {"keys": [-1.0], "positions": base_pos},
            {"keys": [0.0], "positions": base_pos},
            {"keys": [1.0], "positions": shifted_pos},
        ],
    }).ok, "Create Keyforms Binding")

    # Step 5: Live Interactive Preview
    var preview = ClassDB.instantiate("KasaneDocumentPreview")
    root.add_child(preview)
    preview.set_texture_store(textures)
    preview.set_document(doc)
    check(preview.get_last_result().ok, "Preview attached to live document")

    var preview_frame = doc.set_preview_values({PARAM_ID: 1.0})
    check(preview_frame.ok, "Set preview parameter to 1.0")

    var mesh_view = preview.get_mesh_view(MESH_ID)
    check(mesh_view != null, "Mesh view exists")
    var snapshot_pos = mesh_view.get_positions_snapshot()
    check(snapshot_pos.size() == 4, "Mesh view has 4 vertices")

    # Step 6: Save Project to Disk
    var proj_dir = files + "/e2e_project"
    var proj_file = proj_dir + "/project.kasane.json"
    check(io.save_project(doc, proj_file).ok, "Save project to disk")
    check(not doc.get_document_summary().modified, "Document not modified after save")

    preview.free()

    # Step 7: Fresh Reopen from Disk in new Document instance
    var reopen_doc = ClassDB.instantiate("KasaneDocumentBridge")
    check(io.open_project(reopen_doc, proj_file).ok, "Reopen project in clean document")
    var summary = reopen_doc.get_document_summary()
    check(summary.parts.size() == 1, "Reopened parts count matches")
    check(summary.transforms.size() == 2, "Reopened transforms count matches")
    check(summary.meshes.size() == 1, "Reopened meshes count matches")
    check(summary.parameters.size() == 1, "Reopened parameters count matches")
    check(summary.bindings.size() == 1, "Reopened bindings count matches")

    # Verify actual evaluated values survive persistence and feed an independent Core probe.
    var runtime_samples = []
    for value in [-1.0, 0.0, 1.0]:
        var original = doc.set_preview_values({PARAM_ID: value})
        var reopened = reopen_doc.set_preview_values({PARAM_ID: value})
        check(original.ok and reopened.ok, "Evaluate original and reopened document")
        check(original.drawables[0].positions == reopened.drawables[0].positions, "Reopened evaluated positions match original")
        var positions = []
        for point in original.drawables[0].positions:
            positions.append([point.x, point.y])
        runtime_samples.append({"parameter": value, "positions": positions})
    check(runtime_samples[0].positions != runtime_samples[2].positions, "Parameter changes evaluated geometry")

    # Step 8: Export MOC3 / Model3 Package
    var export_dir = files + "/e2e_export"
    var export_res = io.export_package(reopen_doc, export_dir)
    check(export_res.ok, "Export project package")

    var moc3_path = export_dir + "/model.moc3"
    var model3_path = export_dir + "/model.model3.json"
    check(FileAccess.file_exists(moc3_path), "Exported model.moc3 exists on disk")
    check(FileAccess.file_exists(model3_path), "Exported model.model3.json exists on disk")

    # Step 9: Verify exported MOC3 binary header and model3 JSON structure
    var moc3_file = FileAccess.open(moc3_path, FileAccess.READ)
    check(moc3_file != null, "Open exported moc3 file")
    if moc3_file:
        var header = moc3_file.get_buffer(8)
        check(header.slice(0, 4).get_string_from_ascii() == "MOC3", "Exported moc3 magic header matches")
        check(header[4] == 5, "Exported moc3 version 5.0 matches")
        moc3_file.close()

    var model3_file = FileAccess.open(model3_path, FileAccess.READ)
    check(model3_file != null, "Open exported model3.json file")
    if model3_file:
        var model3_dict = JSON.parse_string(model3_file.get_as_text())
        check(model3_dict != null and model3_dict.has("FileReferences"), "model3.json contains FileReferences")
        check(model3_dict.FileReferences.Moc == "model.moc3", "model3.json references model.moc3")
        model3_file.close()

    var report = {
        "status": "passed" if failures.is_empty() else "failed",
        "checks": checks,
        "failures": failures,
        "runtime_samples": runtime_samples,
    }

    var report_file = FileAccess.open(files + "/workflow-report.json", FileAccess.WRITE)
    if report_file:
        report_file.store_string(JSON.stringify(report, "  "))
        report_file.close()

    print(JSON.stringify(report))
    quit(0 if failures.is_empty() else 1)
