extends SceneTree


func _initialize() -> void:
	run.call_deferred()


func fail(message: String) -> void:
	push_error(message)
	quit(1)


func frame_summary(frame: Dictionary) -> Dictionary:
	var canvas: Dictionary = frame.canvas
	var drawable_ids: Array[String] = []
	var offscreen_ids: Array[String] = []
	var texture_ids: Array[String] = []
	var commands: Array[String] = []
	var parameters: Array[Dictionary] = []
	for drawable in frame.drawables:
		drawable_ids.append(drawable.id)
		if not texture_ids.has(drawable.texture_asset_id):
			texture_ids.append(drawable.texture_asset_id)
	for offscreen in frame.offscreens:
		offscreen_ids.append(offscreen.id)
	for command in frame.render_plan:
		commands.append(command.command + ":" + command.id)
	for parameter in frame.parameters:
		parameters.append({"id": parameter.id, "value": parameter.value})
	texture_ids.sort()
	return {
		"canvas": {
			"width": canvas.width,
			"height": canvas.height,
			"origin": [canvas.origin.x, canvas.origin.y],
			"pixels_per_unit": canvas.pixels_per_unit,
		},
		"drawable_ids": drawable_ids,
		"offscreen_ids": offscreen_ids,
		"render_commands": commands,
		"texture_ids": texture_ids,
		"parameters": parameters,
	}


func offscreen_regions(frame: Dictionary, scale: float, offset: Vector2, size: Vector2i) -> Array:
	var drawables := {}
	for drawable in frame.drawables:
		drawables[drawable.id] = drawable
	var bounds := {}
	var stack: Array[String] = []
	var canvas: Dictionary = frame.canvas
	for command in frame.render_plan:
		match command.command:
			"begin_offscreen":
				stack.append(command.id)
				bounds[command.id] = Vector4(INF, INF, -INF, -INF)
			"end_offscreen":
				stack.pop_back()
			"draw_mesh":
				if stack.is_empty():
					continue
				var drawable: Dictionary = drawables[command.id]
				for vertex in drawable.positions:
					var canvas_pixel := Vector2(
						vertex.x * canvas.pixels_per_unit + canvas.origin.x,
						canvas.origin.y - vertex.y * canvas.pixels_per_unit
					)
					var pixel := canvas_pixel * scale + offset
					for id in stack:
						var box: Vector4 = bounds[id]
						bounds[id] = Vector4(
							minf(box.x, pixel.x), minf(box.y, pixel.y),
							maxf(box.z, pixel.x), maxf(box.w, pixel.y)
						)
	var regions := []
	for offscreen in frame.offscreens:
		if not offscreen.enabled or not bounds.has(offscreen.id):
			continue
		var box: Vector4 = bounds[offscreen.id]
		if not is_finite(box.x):
			continue
		var rect := [
			clampi(floori(box.x) - 2, 0, size.x),
			clampi(floori(box.y) - 2, 0, size.y),
			clampi(ceili(box.z) + 2, 0, size.x),
			clampi(ceili(box.w) + 2, 0, size.y),
		]
		if rect[2] > rect[0] and rect[3] > rect[1]:
			regions.append({"name": offscreen.id, "rect": rect})
	return regions


func run() -> void:
	var args := OS.get_cmdline_user_args()
	if args.size() != 2:
		fail("usage: wgpu_real_model_capture.gd CASE_JSON OUTPUT_DIR")
		return
	var case: Dictionary = JSON.parse_string(FileAccess.get_file_as_string(args[0]))
	if case.is_empty() or not ["linear_no_mipmap", "linear_mipmap"].has(case.get("texture_profile", "")):
		fail("invalid case or unsupported texture profile")
		return
	var output_dir: String = args[1]
	DirAccess.make_dir_recursive_absolute(output_dir)
	var model3: String = case.model3
	var doc = ClassDB.instantiate("KasaneDocumentBridge")
	var io = ClassDB.instantiate("KasaneProjectIO")
	var imported: Dictionary = io.import_model3(doc, model3)
	if not imported.get("ok", false):
		fail("model import failed: " + str(imported))
		return
	var requested: Dictionary = case.get("parameters", {})
	if not requested.is_empty():
		var values := {}
		var document_summary: Dictionary = doc.get_document_summary()
		for runtime_id in requested:
			var found := false
			for parameter in document_summary.parameters:
				if parameter.runtime_id == runtime_id:
					values[parameter.id] = requested[runtime_id]
					found = true
					break
			if not found:
				fail("unknown parameter runtime ID: " + runtime_id)
				return
		var updated: Dictionary = doc.set_preview_values(values)
		if not updated.get("ok", false):
			fail("parameter evaluation failed: " + str(updated))
			return
	var frame: Dictionary = doc.get_frame()
	if not frame.get("ok", false):
		fail("frame evaluation failed: " + str(frame))
		return
	var model3_json: Dictionary = JSON.parse_string(FileAccess.get_file_as_string(model3))
	var texture_paths: Array = model3_json.FileReferences.Textures
	var textures = ClassDB.instantiate("KasaneTextureStore")
	var loaded: Array[String] = []
	var mip_hashes := {}
	for drawable in frame.drawables:
		var asset_id: String = drawable.texture_asset_id
		if loaded.has(asset_id):
			continue
		loaded.append(asset_id)
		var slot: int = drawable.texture_slot
		if slot < 0 or slot >= texture_paths.size():
			fail("missing texture slot for " + asset_id)
			return
		var texture_path: String = model3.get_base_dir().path_join(texture_paths[slot])
		var image := Image.load_from_file(texture_path)
		if image.is_empty():
			fail("cannot read " + texture_path)
			return
		if case.texture_profile == "linear_mipmap" and image.generate_mipmaps() != OK:
			fail("cannot generate mipmaps for " + texture_path)
			return
		var hasher := HashingContext.new()
		hasher.start(HashingContext.HASH_SHA256)
		hasher.update(image.get_data())
		mip_hashes[asset_id] = hasher.finish().hex_encode()
		var loaded_result: Dictionary = textures.set_texture(asset_id, ImageTexture.create_from_image(image))
		if not loaded_result.get("ok", false):
			fail("texture upload failed: " + str(loaded_result))
			return
	var viewport := SubViewport.new()
	viewport.size = Vector2i(int(case.width), int(case.height))
	viewport.disable_3d = true
	viewport.transparent_bg = true
	viewport.render_target_update_mode = SubViewport.UPDATE_ALWAYS
	root.add_child(viewport)
	var preview = ClassDB.instantiate("KasaneDocumentPreview")
	preview.texture_filter = CanvasItem.TEXTURE_FILTER_LINEAR
	viewport.add_child(preview)
	var canvas: Dictionary = frame.canvas
	var scale: float = float(case.fit_long_side) / maxf(canvas.width, canvas.height)
	var offset := (Vector2(viewport.size) - Vector2(canvas.width, canvas.height) * scale) * 0.5
	preview.scale = Vector2.ONE * scale
	preview.position = offset
	preview.set_texture_store(textures)
	preview.set_document(doc)
	var submitted: Dictionary = preview.get_last_result()
	if not submitted.get("ok", false):
		fail("preview submission failed: " + str(submitted))
		return
	var ready := false
	for unused in 40:
		await process_frame
		await RenderingServer.frame_post_draw
		if preview.get_observation_state().get("ready", false):
			ready = true
			break
	if not ready:
		fail("Godot preview did not report completed rendering")
		return
	var image_path := output_dir.path_join("godot.png")
	if viewport.get_texture().get_image().save_png(image_path) != OK:
		fail("cannot save " + image_path)
		return
	var report := {
		"status": "passed",
		"image": image_path,
		"frame_summary": frame_summary(frame),
		"object_regions": offscreen_regions(frame, scale, offset, viewport.size),
		"view": {"scale": scale, "offset": [offset.x, offset.y]},
		"texture_profile": case.texture_profile,
		"mip_sha256": mip_hashes,
		"renderer": RenderingServer.get_current_rendering_method(),
		"adapter": RenderingServer.get_video_adapter_name(),
		"render_stats": preview.get_render_stats(),
	}
	var file := FileAccess.open(output_dir.path_join("godot-report.json"), FileAccess.WRITE)
	if file == null:
		fail("cannot write Godot report")
		return
	file.store_string(JSON.stringify(report, "  "))
	file.close()
	quit(0)
