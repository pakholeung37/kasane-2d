extends SceneTree
func _initialize() -> void:
	run.call_deferred()
func run() -> void:
	var config: Dictionary = JSON.parse_string(FileAccess.get_file_as_string(OS.get_cmdline_user_args()[0]))
	ProjectSettings.set_setting("gd_cubism/rendering/batching", false)
	var viewport := SubViewport.new()
	viewport.transparent_bg = true
	viewport.size = Vector2i(config.size[0], config.size[1])
	viewport.render_target_update_mode = SubViewport.UPDATE_ALWAYS
	root.add_child(viewport)
	var model = ClassDB.instantiate("GDCubismUserModel")
	viewport.add_child(model)
	model.assets = "res://package/model.model3.json"
	model.physics_evaluate = false
	model.pose_update = false
	model.playback_process_mode = 2
	for sample in config.samples:
		var scale_value: float = sample.observation.camera.zoom
		var offset := Vector2(sample.observation.camera.offset[0], sample.observation.camera.offset[1]) + Vector2(viewport.size) / 2
		model.scale = Vector2.ONE * scale_value
		model.position = offset + Vector2(config.origin[0], config.origin[1]) * scale_value
		var parameters = model.get_parameters()
		for i in parameters.size():
			parameters[i].value = sample.values[i]
			parameters[i].hold = true
		for frame in 8:
			model.advance(0)
			await process_frame
		await RenderingServer.frame_post_draw
		var img := viewport.get_texture().get_image()
		if img.save_png(sample.reference_path) != OK:
			quit(1)
			return
	model.queue_free()
	await process_frame
	quit()
