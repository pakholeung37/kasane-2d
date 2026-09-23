extends SceneTree

const DEFAULT_WARMUP_FRAMES := 60
const DEFAULT_SAMPLE_FRAMES := 300

var _failures: Array[String] = []


func _initialize() -> void:
	call_deferred("_run")


func _run() -> void:
	var args := OS.get_cmdline_user_args()
	var model3_path := _argument_value(args, "--model3=")
	if model3_path.is_empty() or not FileAccess.file_exists(model3_path):
		_failures.append("missing Ren model3 fixture: " + model3_path)
		_finish({})
		return

	var warmup_frames := _argument_int(args, "--warmup-frames=", DEFAULT_WARMUP_FRAMES)
	var sample_frames := _argument_int(args, "--sample-frames=", DEFAULT_SAMPLE_FRAMES)
	var document = ClassDB.instantiate("KasaneDocumentBridge")
	var io = ClassDB.instantiate("KasaneProjectIO")
	var import_result: Dictionary = io.import_model3(document, model3_path)
	if not import_result.get("ok", false):
		_failures.append("import Ren: " + str(import_result))
		_finish({})
		return

	var frame: Dictionary = document.get_frame()
	var offscreen_count: int = frame.get("offscreens", []).size()
	if offscreen_count <= 0:
		_failures.append("Ren fixture has no offscreen surfaces")
		_finish({})
		return

	var textures = ClassDB.instantiate("KasaneTextureStore")
	var texture_path := model3_path.get_base_dir().path_join("Ren.2048/texture_00.png")
	var image := Image.load_from_file(texture_path)
	if image == null or image.is_empty():
		_failures.append("failed to load Ren texture: " + texture_path)
		_finish({})
		return
	image.generate_mipmaps()
	var asset_id: String = frame.drawables[0].texture_asset_id
	var texture_result: Dictionary = textures.set_texture(
		asset_id,
		ImageTexture.create_from_image(image),
	)
	if not texture_result.get("ok", false):
		_failures.append("load Ren texture: " + str(texture_result))
		_finish({})
		return

	var viewport := SubViewport.new()
	viewport.size = Vector2i(2048, 2048)
	viewport.disable_3d = true
	viewport.transparent_bg = true
	viewport.render_target_update_mode = SubViewport.UPDATE_ALWAYS
	get_root().add_child(viewport)
	var preview = ClassDB.instantiate("KasaneDocumentPreview")
	viewport.add_child(preview)
	var canvas: Dictionary = frame.canvas
	var fit_scale := minf(1800.0 / float(canvas.width), 1800.0 / float(canvas.height))
	preview.scale = Vector2.ONE * fit_scale
	preview.position = Vector2(1024, 1024) - Vector2(canvas.width, canvas.height) * 0.5 * fit_scale
	preview.set_texture_store(textures)
	preview.set_document(document)
	var preview_result: Dictionary = preview.get_last_result()
	if not preview_result.get("ok", false):
		_failures.append("initial Ren preview: " + str(preview_result))
		_finish({})
		return

	for unused in warmup_frames:
		await process_frame
		await RenderingServer.frame_post_draw

	var parameters: Array = frame.get("parameters", [])
	var parameter_id := ""
	if not parameters.is_empty():
		parameter_id = parameters[0].id
	var refresh_samples: Array[float] = []
	var frame_samples: Array[float] = []
	var sample_start := Time.get_ticks_usec()
	for index in sample_frames:
		var value := sin(float(index) * 0.071)
		var frame_start := Time.get_ticks_usec()
		var refresh_start := frame_start
		var result: Dictionary
		if parameter_id.is_empty():
			result = preview.refresh_geometry()
		else:
			result = document.set_preview_values({parameter_id: value})
		var refresh_end := Time.get_ticks_usec()
		if not result.get("ok", false):
			_failures.append("Ren frame update: " + str(result))
		await process_frame
		await RenderingServer.frame_post_draw
		var frame_end := Time.get_ticks_usec()
		refresh_samples.append(float(refresh_end - refresh_start) / 1000.0)
		frame_samples.append(float(frame_end - frame_start) / 1000.0)

	var stats: Dictionary = preview.get_render_stats()
	if stats.get("offscreen_groups", 0) != offscreen_count:
		_failures.append(
			"offscreen resource count mismatch: expected %d, got %s"
			% [offscreen_count, str(stats.get("offscreen_groups", 0))]
		)
	_finish({
		"schema_version": 1,
		"benchmark": "kasane-preview",
		"workload": "ren-offscreen",
		"model3": model3_path,
		"renderer": RenderingServer.get_current_rendering_method(),
		"adapter": RenderingServer.get_video_adapter_name(),
		"godot": Engine.get_version_info(),
		"warmup_frames": warmup_frames,
		"sample_frames": sample_frames,
		"elapsed_ms": float(Time.get_ticks_usec() - sample_start) / 1000.0,
		"refresh_cpu_ms": _summary(refresh_samples),
		"frame_ms": _summary(frame_samples),
		"offscreen_count": offscreen_count,
		"render_command_count": frame.get("render_plan", []).size(),
		"render_stats": stats,
	})


func _argument_value(args: Array[String], prefix: String) -> String:
	for argument in args:
		if argument.begins_with(prefix):
			return argument.trim_prefix(prefix)
	return ""


func _argument_int(args: Array[String], prefix: String, fallback: int) -> int:
	var value := _argument_value(args, prefix)
	return maxi(int(value), 1) if not value.is_empty() else fallback


func _summary(values: Array[float]) -> Dictionary:
	if values.is_empty():
		return {"mean": 0.0, "p50": 0.0, "p95": 0.0, "p99": 0.0}
	var sorted: Array[float] = values.duplicate()
	sorted.sort()
	var total := 0.0
	for value in sorted:
		total += value
	return {
		"mean": total / sorted.size(),
		"p50": _percentile(sorted, 0.50),
		"p95": _percentile(sorted, 0.95),
		"p99": _percentile(sorted, 0.99),
	}


func _percentile(sorted: Array[float], fraction: float) -> float:
	var index := mini(int(ceil(fraction * sorted.size())) - 1, sorted.size() - 1)
	return sorted[maxi(index, 0)]


func _finish(result: Dictionary) -> void:
	result["status"] = "passed" if _failures.is_empty() else "failed"
	result["failures"] = _failures
	var file := FileAccess.open("res://benchmark.json", FileAccess.WRITE)
	if file:
		file.store_string(JSON.stringify(result, "  "))
	print("BENCHMARK_RESULT " + JSON.stringify(result))
	quit(0 if _failures.is_empty() else 1)
