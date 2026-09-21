extends SceneTree

var failures: Array[String] = []
var checks := 0

func check(condition: bool, label: String) -> void:
	checks += 1
	if not condition:
		failures.append(label)
		push_error(label)

func id(n: int) -> String:
	return "%08x-1111-4111-8111-111111111111" % n

func mesh(n: int, mask_ids: Array, opacity: float) -> Dictionary:
	return {"id": id(n), "name": "mesh%d" % n, "texture_asset_id": id(2),
		"vertex_ids": PackedInt64Array([1, 2, 3]),
		"base_positions": PackedVector2Array([Vector2(0, 0), Vector2(64, 0), Vector2(0, 64)]),
		"uvs": PackedVector2Array([Vector2.ZERO, Vector2.RIGHT, Vector2.DOWN]),
		"triangles": PackedInt64Array([1, 2, 3]),
		"properties": {"part_id": "", "deformer_id": "", "blend_mode": 0, "enabled": true, "double_sided": true, "inverted_mask": false, "masks": mask_ids, "appearance": {"opacity": opacity, "multiply": [1, 1, 1], "screen": [0, 0, 0]}}}

func _initialize() -> void:
	run.call_deferred()

func run() -> void:
	var output := OS.get_cmdline_user_args()[0]
	var image := Image.create(8, 8, false, Image.FORMAT_RGBA8)
	image.fill(Color.WHITE)
	var source := output.path_join("texture.png")
	image.save_png(source)
	var texture := ImageTexture.create_from_image(image)
	var view = ClassDB.instantiate("KasaneMeshView")
	root.add_child(view)
	check(view.initialize(PackedVector2Array([Vector2.ZERO, Vector2.RIGHT, Vector2.DOWN]), PackedVector2Array([Vector2.ZERO, Vector2.RIGHT, Vector2.DOWN]), PackedInt32Array([0, 1, 2]), texture).ok, "initialize mesh view")
	view.clear()
	check(view.mesh == null and not view.visible, "clear removes engine mesh and hides view")
	view.queue_free()
	var doc = ClassDB.instantiate("KasaneDocumentBridge")
	check(doc.initialize(id(1), Vector2(64, 64), Vector2.ZERO, 1).ok, "initialize document")
	check(doc.add_image_asset(id(2), "texture", source, 8, 8).ok, "add asset")
	check(doc.create_mesh(mesh(3, [], 0)).ok, "mask source")
	check(doc.create_mesh(mesh(4, [id(3)], 1)).ok, "first masked target")
	check(doc.create_mesh(mesh(5, [id(3)], 1)).ok, "second masked target")
	var textures = ClassDB.instantiate("KasaneTextureStore")
	check(textures.set_texture(id(2), texture).ok, "set texture")
	var viewport := SubViewport.new()
	viewport.size = Vector2i(64, 64)
	viewport.transparent_bg = true
	viewport.render_target_update_mode = SubViewport.UPDATE_ALWAYS
	root.add_child(viewport)
	var preview = ClassDB.instantiate("KasaneDocumentPreview")
	viewport.add_child(preview)
	preview.set_texture_store(textures)
	preview.set_document(doc)
	for unused in 4:
		await process_frame
		await RenderingServer.frame_post_draw
	check(preview.get_last_result().ok, "render masked targets")
	check(preview.get_render_stats().mask_viewports == 1, "targets share mask viewport")
	check(viewport.get_texture().get_image().get_pixel(8, 8).a > 0.9, "shared mask covers target")
	for child in preview.get_children():
		if child is SubViewport:
			check(RenderingServer.viewport_get_update_mode(child.get_viewport_rid()) == RenderingServer.VIEWPORT_UPDATE_DISABLED, "static mask stops rendering")
	var source_mesh: Dictionary = doc.get_mesh_snapshot(id(3))
	source_mesh.triangles = PackedInt64Array()
	check(doc.replace_mesh(source_mesh).ok, "remove source faces")
	for unused in 4:
		await process_frame
		await RenderingServer.frame_post_draw
	check(viewport.get_texture().get_image().get_pixel(8, 8).a < 0.01, "shared mask invalidates after geometry edit")
	# Geometry and metadata edits use the verified texture; explicit refresh
	# must still detect external changes to the saved resource.
	var io = ClassDB.instantiate("KasaneProjectIO")
	var saved := output.path_join("saved")
	check(io.save_project(doc, saved).ok, "save project with loaded texture")
	var manifest: Dictionary = JSON.parse_string(FileAccess.get_file_as_string(saved.path_join("project.kasane.json")))
	var asset_path: String = saved.path_join(manifest.document.assets[0].source)
	var backup := asset_path + ".backup"
	check(DirAccess.rename_absolute(asset_path, backup) == OK, "temporarily remove disk asset")
	check(doc.rename_mesh(id(4), "renamed").ok, "metadata edit")
	check(preview.get_last_result().ok, "metadata edit avoids disk reload")
	check(doc.set_vertex_positions(id(4), PackedInt64Array([1]), PackedVector2Array([Vector2(1, 1)])).ok, "vertex edit")
	check(preview.get_last_result().ok, "vertex edit avoids disk reload")
	check(not preview.refresh().ok, "explicit refresh detects missing disk asset")
	check(DirAccess.rename_absolute(backup, asset_path) == OK, "restore disk asset")
	check(preview.refresh().ok, "explicit refresh recovers restored asset")
	viewport.queue_free()
	await process_frame
	print(JSON.stringify({"status": "passed" if failures.is_empty() else "failed", "checks": checks, "failures": failures}))
	quit(0 if failures.is_empty() else 1)
