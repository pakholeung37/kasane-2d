extends RefCounted
## Application services shared by UI and future script host.
var document
var files
var startup_error := ""

func new_id() -> String:
	var bytes := Crypto.new().generate_random_bytes(16)
	bytes[6] = (bytes[6] & 15) | 64
	bytes[8] = (bytes[8] & 63) | 128
	var hex := bytes.hex_encode()
	return "%s-%s-%s-%s-%s" % [hex.substr(0, 8), hex.substr(8, 4), hex.substr(12, 4), hex.substr(16, 4), hex.substr(20, 12)]

func new_project(size: Vector2 = Vector2(1024, 1024)) -> Dictionary:
	return document.new_project(new_id(), size, size / 2.0, 100.0)

func open_project(path: String) -> Dictionary:
	return files.open_project(document, path)

func save_project(path: String) -> Dictionary:
	return files.save_project(document, path)

func import_model(path: String) -> Dictionary:
	return files.import_model3(document, path)

func export_model(path: String) -> Dictionary:
	return files.export_package(document, path)

var undo_redo := UndoRedo.new()
var action_before: RefCounted
var action_name := ""
var action_generation := 0

func _notification(what: int) -> void:
	if what == NOTIFICATION_PREDELETE and is_instance_valid(undo_redo):
		undo_redo.free()

func _init() -> void:
	for native_class in ["KasaneDocumentBridge", "KasaneProjectIO", "KasaneDocumentPreview"]:
		if not ClassDB.class_exists(native_class):
			startup_error = "原生扩展未加载（%s）。\n在仓库根目录运行：\npython3 tools/build_editor.py --prepare-source\n然后关闭并重新打开 apps/editor/project.godot。\n若仍失败，请查看此前的 GDExtension 加载错误。" % native_class
			return
	document = ClassDB.instantiate("KasaneDocumentBridge")
	files = ClassDB.instantiate("KasaneProjectIO")
	if document == null or files == null:
		startup_error = "原生扩展对象创建失败，请检查 Godot 的 GDExtension 加载错误。"
		return
	document.changed.connect(_document_changed)

func _document_changed(_change: Dictionary) -> void:
	var generation: int = document.get_document_state().generation
	if action_generation != generation:
		undo_redo.clear_history()
		action_before = null
		action_generation = generation

func failure(code: String, message: String) -> Dictionary:
	return {"ok": false, "code": code, "message": message}

func begin_action(label: String) -> Dictionary:
	if action_before != null:
		return failure("ACTION_ACTIVE", "Finish the current Action first.")
	action_before = document.capture_state()
	if action_before == null:
		return failure("SNAPSHOT_UNAVAILABLE", "Finish any explicit document transaction first.")
	action_name = label
	return {"ok": true}

func end_action() -> Dictionary:
	if action_before == null:
		return failure("NO_ACTION", "No Action is active.")
	var after = document.capture_state()
	if after == null:
		return failure("SNAPSHOT_UNAVAILABLE", "Finish any explicit document transaction first.")
	undo_redo.create_action(action_name)
	undo_redo.add_do_method(document.restore_state.bind(after))
	undo_redo.add_undo_method(document.restore_state.bind(action_before))
	undo_redo.commit_action(false)
	action_before = null
	return {"ok": true}

func cancel_action() -> Dictionary:
	if action_before == null:
		return failure("NO_ACTION", "No Action is active.")
	var result: Dictionary = document.restore_state(action_before)
	if result.ok:
		action_before = null
	return result

func import_png(path: String, original_size: Vector2 = Vector2.ZERO, crop_offset: Vector2 = Vector2.ZERO) -> Dictionary:
	var summary: Dictionary = document.get_document_state()
	if not summary.initialized:
		return failure("UNINITIALIZED", "Create a project with the original canvas size first.")
	if path.get_extension().to_lower() != "png":
		return failure("INVALID_ASSET", "Select a PNG file.")
	var image := Image.new()
	var error := image.load(path)
	if error != OK:
		return failure("IMAGE_READ_FAILED", "Cannot decode PNG (%d)." % error)
	var dimensions := Vector2(image.get_size())
	if original_size == Vector2.ZERO:
		original_size = dimensions
	if not original_size.is_finite() or not crop_offset.is_finite() or original_size != summary.canvas_size or crop_offset.x < 0 or crop_offset.y < 0 or (crop_offset + dimensions).x > original_size.x or (crop_offset + dimensions).y > original_size.y:
		return failure("INVALID_CANVAS", "Original size must match the project; cropped image must fit at its explicit offset.")
	var id := new_id()
	var result: Dictionary = document.add_image_asset(id, path.get_file().get_basename(), ProjectSettings.globalize_path(path), image.get_width(), image.get_height())
	if result.ok:
		result.asset_id = id
		result.original_size = original_size
		result.crop_offset = crop_offset
		result.image_size = dimensions
	return result

func create_rectangle(asset_id: String, crop_offset: Vector2 = Vector2.ZERO, name: String = "") -> Dictionary:
	var asset: Dictionary = document.get_asset_snapshot(asset_id)
	if not asset.ok:
		return asset
	var id := new_id()
	var size := Vector2(asset.width, asset.height)
	var result: Dictionary = document.create_mesh({"id": id, "name": name if not name.is_empty() else asset.name,
		"runtime_id": "ArtMesh_" + id.replace("-", ""), "texture_asset_id": asset_id,
		"vertex_ids": PackedInt64Array([1, 2, 3, 4]),
		"base_positions": PackedVector2Array([crop_offset, crop_offset + Vector2(size.x, 0), crop_offset + size, crop_offset + Vector2(0, size.y)]),
		"uvs": PackedVector2Array([Vector2(0, 0), Vector2(1, 0), Vector2(1, 1), Vector2(0, 1)]),
		"triangles": PackedInt64Array([1, 2, 3, 1, 3, 4])})
	if result.ok:
		result.mesh_id = id
	return result

func find_object(id: String) -> Dictionary:
	var summary: Dictionary = document.get_document_summary()
	for group in ["assets", "parts", "parameters", "transforms", "bindings", "scene_bindings", "blend_key_tables", "blend_constraints", "blend_bindings", "glues"]:
		for item in summary[group]:
			if item.id == id:
				return {"ok": true, "kind": group, "data": item}
	var mesh: Dictionary = document.get_mesh_snapshot(id)
	if mesh.ok:
		return {"ok": true, "kind": "meshes", "data": mesh}
	return failure("MISSING_OBJECT", id)

func _same_keys(left: Variant, right: Variant) -> bool:
	if not left is Array or not right is Array or left.size() != right.size():
		return false
	for value in left + right:
		if not (value is int or value is float) or not is_finite(float(value)):
			return false
	# The model stores key values as float32; use the same canonical precision.
	return PackedFloat32Array(left) == PackedFloat32Array(right)

func get_keyform(binding_id: String, keys: Array) -> Dictionary:
	var found := find_object(binding_id)
	if not found.ok:
		return found
	if found.kind not in ["bindings", "scene_bindings"]:
		return failure("INVALID_BINDING", binding_id)
	for form in found.data.keyforms:
		if _same_keys(form.keys, keys):
			return {"ok": true, "data": form.duplicate(true)}
	return failure("INVALID_KEY_COMBINATION", "Use one exact key from each axis, in axis order.")

func set_keyform(binding_id: String, form: Dictionary) -> Dictionary:
	var found := find_object(binding_id)
	if not found.ok:
		return found
	if found.kind not in ["bindings", "scene_bindings"] or not form.has("keys"):
		return failure("INVALID_BINDING", binding_id)
	for i in found.data.keyforms.size():
		if _same_keys(found.data.keyforms[i].keys, form.keys):
			found.data.keyforms[i] = form.duplicate(true)
			return document.write_binding(found.data, true) if found.kind == "bindings" else document.write_scene_binding(found.data, true)
	return failure("INVALID_KEY_COMBINATION", "Replace the complete binding to change its axes.")

func complete_binding(description: Dictionary, template: Dictionary, scene: bool = false, replace: bool = false) -> Dictionary:
	# Fill only absent combinations. Core validates all fields and duplicate keys atomically.
	var data := description.duplicate(true)
	if not data.get("axes") is Array or data.axes.is_empty() or data.axes.size() > 3:
		return failure("INVALID_AXES", "Provide one to three binding axes.")
	var combinations: Array = [[]]
	for axis in data.axes:
		if not axis is Dictionary or not axis.get("keys") is Array or axis.keys.is_empty():
			return failure("INVALID_AXES", "Each axis requires a nonempty keys array.")
		if combinations.size() * axis.keys.size() > 4096:
			return failure("TOO_MANY_KEYFORMS", "At most 4096 combinations per helper call.")
		var next: Array = []
		for combination in combinations:
			for key in axis.keys:
				next.append(combination + [key])
		combinations = next
	if not data.has("keyforms"):
		data.keyforms = []
	if not data.keyforms is Array:
		return failure("INVALID_KEYFORMS", "keyforms must be an Array.")
	for keys in combinations:
		var exists := false
		for form in data.keyforms:
			if not form is Dictionary:
				return failure("INVALID_KEYFORMS", "Each keyform must be a Dictionary.")
			exists = exists or _same_keys(form.get("keys"), keys)
		if not exists:
			var form := template.duplicate(true)
			form.keys = keys
			data.keyforms.append(form)
	return document.write_scene_binding(data, replace) if scene else document.write_binding(data, replace)

var surface: WeakRef

func fit_view(id: String = "") -> Dictionary:
	if surface == null or surface.get_ref() == null:
		return failure("NO_CANVAS", "No canvas is attached.")
	surface.get_ref().fit_content(id)
	return {"ok": true}

func json_value(value: Variant) -> Variant:
	if value is Dictionary:
		var out := {}
		for key in value:
			out[str(key)] = json_value(value[key])
		return out
	if value is Vector2 or value is Vector2i:
		return [value.x, value.y]
	if value is Color:
		return [value.r, value.g, value.b, value.a]
	if value is Array or value is PackedVector2Array or value is PackedFloat32Array or value is PackedFloat64Array or value is PackedInt32Array or value is PackedInt64Array or value is PackedStringArray or value is PackedByteArray:
		var out: Array = []
		for item in value:
			out.append(json_value(item))
		return out
	return value
