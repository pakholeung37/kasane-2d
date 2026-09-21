extends SceneTree

var failures: Array[String] = []
var memory_samples: Array = []

func sample_memory() -> void:
	memory_samples.append({"frame": Engine.get_frames_drawn(), "texture_bytes":RenderingServer.get_rendering_info(RenderingServer.RENDERING_INFO_TEXTURE_MEM_USED), "video_bytes":RenderingServer.get_rendering_info(RenderingServer.RENDERING_INFO_VIDEO_MEM_USED)})

func require_result(result: Dictionary, label: String) -> void:
	if not result.get("ok", false):
		failures.append(label + ": " + str(result))

func _initialize() -> void:
	RenderingServer.frame_post_draw.connect(sample_memory)
	run.call_deferred()

func run() -> void:
	var args := OS.get_cmdline_user_args()
	var model3 := args[0]
	var output_dir := args[1]
	var doc = ClassDB.instantiate("KasaneDocumentBridge")
	var io = ClassDB.instantiate("KasaneProjectIO")
	require_result(io.import_model3(doc, model3), "import Ren")
	var frame: Dictionary = doc.get_frame()
	if frame.get("offscreens", []).size() != 24:
		failures.append("expected 24 offscreens")
	if frame.get("render_plan", []).is_empty():
		failures.append("render plan is empty")

	var textures = ClassDB.instantiate("KasaneTextureStore")
	var texture_path := model3.get_base_dir().path_join("Ren.2048/texture_00.png")
	var image := Image.load_from_file(texture_path)
	image.generate_mipmaps()
	var asset_id: String = frame.drawables[0].texture_asset_id
	require_result(textures.set_texture(asset_id, ImageTexture.create_from_image(image)), "texture")

	var viewport := SubViewport.new()
	viewport.size = Vector2i(2048, 2048)
	viewport.disable_3d = true
	viewport.transparent_bg = true
	viewport.render_target_update_mode = SubViewport.UPDATE_ALWAYS
	root.add_child(viewport)
	var preview = ClassDB.instantiate("KasaneDocumentPreview")
	viewport.add_child(preview)
	var canvas: Dictionary = frame.canvas
	var bounds := Rect2(Vector2.ZERO, Vector2(canvas.width, canvas.height))
	var fit_scale: float = minf(1800.0 / canvas.width, 1800.0 / canvas.height)
	preview.scale = Vector2.ONE * fit_scale
	preview.position = Vector2(1024, 1024) - Vector2(canvas.width, canvas.height) * 0.5 * fit_scale
	preview.set_texture_store(textures)
	preview.set_document(doc)
	require_result(preview.get_last_result(), "preview")

	for unused in 6:
		await process_frame
		await RenderingServer.frame_post_draw
	var stats: Dictionary = preview.get_render_stats()
	if stats.get("offscreen_groups", 0) != 24:
		failures.append("expected 24 persistent offscreen resources: " + str(stats))
	var baseline_creations: int = stats.get("creations", -1)
	var baseline_offscreen_creations: int = stats.get("offscreen_creations", -1)
	var baseline_resizes: int = stats.get("offscreen_resizes", -1)
	var baseline_rids := []
	for offscreen in frame.offscreens:
		var texture: Texture2D = preview.get_offscreen_texture(offscreen.id)
		baseline_rids.append(texture.get_rid())
		if texture.get_width() > 2048 or texture.get_height() > 2048:
			failures.append("offscreen exceeds preview resolution")
	if stats.get("offscreen_reserved_bytes", 0) > stats.get("offscreen_budget_bytes", 0):
		failures.append("offscreen attachment budget exceeded")
	DirAccess.make_dir_recursive_absolute(output_dir)
	var baseline_capture := viewport.get_texture().get_image()
	baseline_capture.save_png(output_dir.path_join("ren-kasane-baseline.png"))
	for index in mini(frame.offscreens.size(), 2):
		var offscreen_texture: Texture2D = preview.get_offscreen_texture(frame.offscreens[index].id)
		if offscreen_texture:
			offscreen_texture.get_image().save_png(output_dir.path_join("offscreen-%02d.png" % index))
	for unused in 3:
		require_result(preview.refresh_geometry(), "stable refresh")
	for unused in 4:
		await process_frame
		await RenderingServer.frame_post_draw
	stats = preview.get_render_stats()
	if stats.get("creations", -2) != baseline_creations or stats.get("offscreen_groups", 0) != 24:
		failures.append("stable frame rebuilt GPU resources: " + str(stats))

	if stats.get("offscreen_creations", -2) != baseline_offscreen_creations or stats.get("offscreen_resizes", -2) != baseline_resizes:
		failures.append("stable refresh recreated/resized offscreens")
	for index in frame.offscreens.size():
		if preview.get_offscreen_texture(frame.offscreens[index].id).get_rid() != baseline_rids[index]:
			failures.append("stable refresh replaced viewport texture")
	var capture := viewport.get_texture().get_image()
	if capture.get_data() != baseline_capture.get_data():
		failures.append("stable refresh changed rendered pixels")
	# Regions absent in the original collar/hand-only failure. These are smoke
	# checks for composition, not a substitute for the official image oracle.
	for region in [Rect2i(960, 240, 128, 160), Rect2i(960, 600, 128, 240), Rect2i(960, 1300, 128, 240)]:
		var covered := 0
		for y in range(region.position.y, region.end.y, 4):
			for x in range(region.position.x, region.end.x, 4):
				if capture.get_pixel(x, y).a > 0.1:
					covered += 1
		if covered < region.size.x * region.size.y / 64:
			failures.append("missing composed head/body/legs: " + str(region))
	var image_path := output_dir.path_join("ren-kasane.png")
	if capture.is_empty() or capture.save_png(image_path) != OK:
		failures.append("failed to capture Ren preview")
	preview.scale *= 0.5
	for unused in 4:
		await process_frame
		await RenderingServer.frame_post_draw
	var zoom_stats: Dictionary = preview.get_render_stats()
	if zoom_stats.get("offscreen_color_bytes", 0) >= stats.get("offscreen_color_bytes", 0):
		failures.append("zoom did not reduce offscreen allocation")
	preview.scale *= 2.0
	for unused in 6:
		await process_frame
		await RenderingServer.frame_post_draw
	if viewport.get_texture().get_image().get_data() != capture.get_data():
		failures.append("zoom A-B-A changed composition")
	if preview.get_render_stats().get("offscreen_creations", -2) != baseline_offscreen_creations:
		failures.append("zoom recreated viewport nodes")
	if not frame.parameters.is_empty():
		var parameter: Dictionary = frame.parameters[0]
		require_result(doc.set_preview_values({parameter.id: parameter.value + 10.0}), "parameter update")
		for unused in 12:
			await process_frame
			await RenderingServer.frame_post_draw
			if preview.get_observation_state().get("ready", false):
				break
		if not preview.get_observation_state().get("ready", false):
			failures.append("parameter submission never ready")
		var ready_image: Image = viewport.get_texture().get_image()
		for unused in 6:
			await process_frame
			await RenderingServer.frame_post_draw
		if ready_image.get_data() != viewport.get_texture().get_image().get_data():
			failures.append("ready signaled before nested composition settled")
		require_result(doc.set_preview_values({}), "restore parameter")
	var before_budget: Dictionary = preview.get_render_stats()
	viewport.size = Vector2i(4096, 4096)
	preview.scale = Vector2.ONE
	require_result({"ok": preview.refresh_geometry().get("code", "") == "OFFSCREEN_BUDGET_EXCEEDED"}, "budget preflight rejects oversized surfaces")
	var rejected_stats: Dictionary = preview.get_render_stats()
	if rejected_stats.offscreen_creations != before_budget.offscreen_creations or rejected_stats.offscreen_resizes != before_budget.offscreen_resizes:
		failures.append("budget failure changed GPU allocation")
	viewport.size = Vector2i(2048, 2048)
	preview.scale = Vector2.ONE * fit_scale
	require_result(preview.refresh_geometry(), "recover after budget failure")
	var stable_memory: Array = []
	for unused in 30:
		require_result(preview.refresh_geometry(), "stable memory refresh")
		await process_frame
		await RenderingServer.frame_post_draw
		stable_memory.append(preview.get_render_stats())
	for state in stable_memory.slice(3):
		if state.gpu_texture_bytes != stable_memory[3].gpu_texture_bytes or state.offscreen_creations != stable_memory[3].offscreen_creations or state.offscreen_resizes != stable_memory[3].offscreen_resizes:
			failures.append("GPU allocations grew during stable frames")
	var peak_gpu := 0
	for sample in memory_samples:
		peak_gpu = maxi(peak_gpu, sample.video_bytes)
	if peak_gpu == 0 or peak_gpu > 768 * 1024 * 1024:
		failures.append("measured renderer allocation exceeds 768 MiB or unavailable: " + str(peak_gpu))
	var report := {
		"memory_samples": memory_samples,
		"stable_memory": stable_memory,
		"peak_renderer_video_bytes": peak_gpu,
		"renderer_video_budget_bytes": 768 * 1024 * 1024,
		"status": "passed" if failures.is_empty() else "failed",
		"failures": failures,
		"offscreen_count": frame.get("offscreens", []).size(),
		"offscreens": frame.get("offscreens", []),
		"render_command_count": frame.get("render_plan", []).size(),
		"render_plan": frame.get("render_plan", []),
		"render_stats": stats,
		"bounds": [bounds.position.x, bounds.position.y, bounds.size.x, bounds.size.y],
		"fit_scale": fit_scale,
		"renderer": RenderingServer.get_current_rendering_method(),
		"adapter": RenderingServer.get_video_adapter_name(),
		"image": image_path,
	}
	var file := FileAccess.open(output_dir.path_join("godot-report.json"), FileAccess.WRITE)
	file.store_string(JSON.stringify(report, "  "))
	file.close()
	if failures.is_empty():
		quit(0)
	else:
		for failure in failures:
			push_error(failure)
		quit(1)
