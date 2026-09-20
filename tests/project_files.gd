extends SceneTree
# Binding/resource adapter smoke only. Filesystem behavior is tested by native CTest.
var checks = []
var failures = []
func check(value, label):
    checks.append({"name":label,"expected":true,"actual":value,"status":"passed" if value else "failed"})
    if not value: failures.append(label); push_error(label)
func _initialize():
    call_deferred("run")
func run():
    var args = OS.get_cmdline_user_args()
    var source = args[0]
    var output = args[1]
    var io = ClassDB.instantiate("KasaneProjectIO")
    var doc = ClassDB.instantiate("KasaneDocumentBridge")
    var opened = io.open_project(doc, source)
    check(opened.ok and opened.resources_complete,"bind native open result")
    var mesh = "11111111-1111-4111-8111-000000000020"
    var asset = "11111111-1111-4111-8111-000000000002"
    var parameter = "11111111-1111-4111-8111-000000000010"
    var handle = doc.get_mesh(mesh)
    check(handle.is_valid(),"native session mesh handle")
    check(not io.open_project(doc,"res://project.kasane.json").ok,"binding does not translate res URI")
    check(not io.save_project(doc,"user://project.kasane.json").ok,"binding does not translate user URI")
    check(handle.is_valid(),"failed native operation preserves handle")
    check(doc.set_preview_values({parameter:0.5}).ok and not doc.get_document_summary().modified,"preview does not dirty native session")
    var state = doc.capture_state()
    check(doc.rename_mesh(mesh,"binding edit").ok and doc.get_document_summary().modified,"source edit reaches native session")
    check(doc.restore_state(state).ok and not doc.get_document_summary().modified,"source snapshot preserves native save baseline")
    var saved = io.save_project(doc,output+"/copy")
    check(saved.ok and saved.published and saved.durable,"native publication result exposed")
    check(handle.is_valid(),"native save preserves live handles")
    check(io.open_project(doc,output+"/copy").ok and not handle.is_valid(),"native open invalidates old handles")
    check(doc.get_frame().parameters[0].value == 0,"open resets temporary preview")
    var textures = ClassDB.instantiate("KasaneTextureStore")
    check(textures.load_asset(doc,asset).ok,"native PNG bytes become Godot texture")
    var preview = ClassDB.instantiate("KasaneDocumentPreview")
    root.add_child(preview)
    preview.set_texture_store(textures)
    preview.set_document(doc)
    check(preview.refresh().ok,"preview consumes native session")
    var relative = doc.get_asset_snapshot(asset).source
    var texture_path = output+"/copy/"+relative
    var png = FileAccess.get_file_as_bytes(texture_path)
    DirAccess.remove_absolute(texture_path)
    var diagnostics = io.diagnose_resources(doc)
    check(diagnostics.ok and not diagnostics.resources_complete and diagnostics.diagnostics[0].asset_id==asset,"native diagnostics converted to script")
    check(not preview.refresh().ok,"missing native resource clears preview")
    check(not textures.load_asset(doc,asset).ok and textures.get_texture(asset)==null,"failed native load invalidates texture cache")
    var file = FileAccess.open(texture_path,FileAccess.WRITE)
    file.store_buffer(png);file.close()
    check(preview.refresh().ok,"restored native resource reloads preview")
    check(io.export_package(doc,output+"/runtime").ok,"export binding reaches native publisher")
    preview.free()
    file = FileAccess.open(output+"/godot-report.json",FileAccess.WRITE)
    file.store_string(JSON.stringify({"status":"passed" if failures.is_empty() else "failed","checks":checks,"failures":failures,"godot":Engine.get_version_info()}))
    file.close()
    quit(0 if failures.is_empty() else 1)
