extends SceneTree

const DEFAULT_WARMUP_FRAMES := 60
const DEFAULT_SAMPLE_FRAMES := 300

var _failures: Array[String] = []


func _initialize() -> void:
	call_deferred("_run")


func _run() -> void:
	var args := OS.get_cmdline_user_args()
	if args.is_empty():
		_failures.append("missing benchmark data directory")
		_finish({})
		return

	var data_dir: String = args[0]
	var warmup_frames := _argument_int(args, "--warmup-frames=", DEFAULT_WARMUP_FRAMES)
	var sample_frames := _argument_int(args, "--sample-frames=", DEFAULT_SAMPLE_FRAMES)
	var source_path := data_dir.path_join("gpu-source.json")
	var parsed: Variant = JSON.parse_string(FileAccess.get_file_as_string(source_path))
	if not parsed is Dictionary or not parsed.has("document"):
		_failures.append("invalid Kasane benchmark fixture")
		_finish({})
		return

	var source: Dictionary = parsed
	var document_data: Dictionary = source.document
	var document = ClassDB.instantiate("KasaneDocumentBridge")
	var io = ClassDB.instantiate("KasaneProjectIO")
	_require(io.open_project(document, source_path), "open fixture")

	var textures = ClassDB.instantiate("KasaneTextureStore")
	for asset in document_data.assets:
		var image := Image.load_from_file(data_dir.path_join(asset.source))
		if image == null or image.is_empty():
			_failures.append("failed to load texture: " + str(asset.source))
			continue
		image.generate_mipmaps()
		_require(
			textures.set_texture(asset.id, ImageTexture.create_from_image(image)),
			"load texture " + str(asset.id),
		)

	var viewport := SubViewport.new()
	viewport.size = Vector2i(640, 480)
	viewport.disable_3d = true
	viewport.transparent_bg = true
	viewport.render_target_update_mode = SubViewport.UPDATE_ALWAYS
	get_root().add_child(viewport)
	var preview = ClassDB.instantiate("KasaneDocumentPreview")
	viewport.add_child(preview)
	preview.set_texture_store(textures)
	preview.set_document(document)
	_require(preview.get_last_result(), "initial preview")

	for unused in warmup_frames:
		await process_frame
		await RenderingServer.frame_post_draw

	var parameters: Array = document_data.parameters
	if parameters.is_empty():
		_failures.append("benchmark fixture has no parameter")
		_finish({})
		return
	var parameter_id: String = parameters[0].id
	var refresh_samples: Array[float] = []
	var frame_samples: Array[float] = []
	var sample_start := Time.get_ticks_usec()
	for index in sample_frames:
		var value := sin(float(index) * 0.071)
		var frame_start := Time.get_ticks_usec()
		var refresh_start := frame_start
		var result: Dictionary = document.set_preview_values({parameter_id: value})
		var refresh_end := Time.get_ticks_usec()
		_require(result, "parameter update")
		await process_frame
		await RenderingServer.frame_post_draw
		var frame_end := Time.get_ticks_usec()
		refresh_samples.append(float(refresh_end - refresh_start) / 1000.0)
		frame_samples.append(float(frame_end - frame_start) / 1000.0)

	var stats: Dictionary = preview.get_render_stats()
	var elapsed_ms := float(Time.get_ticks_usec() - sample_start) / 1000.0
	_finish({
		"schema_version": 1,
		"benchmark": "kasane-preview",
		"workload": "gpu-fixture",
		"renderer": RenderingServer.get_current_rendering_method(),
		"adapter": RenderingServer.get_video_adapter_name(),
		"godot": Engine.get_version_info(),
		"warmup_frames": warmup_frames,
		"sample_frames": sample_frames,
		"elapsed_ms": elapsed_ms,
		"refresh_cpu_ms": _summary(refresh_samples),
		"frame_ms": _summary(frame_samples),
		"render_stats": stats,
	})


func _argument_int(args: Array[String], prefix: String, fallback: int) -> int:
	for argument in args:
		if argument.begins_with(prefix):
			return maxi(int(argument.trim_prefix(prefix)), 1)
	return fallback


func _require(result: Dictionary, label: String) -> void:
	if not result.get("ok", false):
		_failures.append(label + ": " + str(result))


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
