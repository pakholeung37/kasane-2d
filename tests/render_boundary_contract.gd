extends "res://m3c_surface_lifecycle.gd"

# Characterize the current renderer before changing its logical dependency and
# invalidation contracts. The inherited helpers construct real Document data.
func run() -> void:
	output = OS.get_cmdline_user_args()[0]
	DirAccess.make_dir_recursive_absolute(output)
	doc = ClassDB.instantiate("KasaneDocumentBridge")
	ok(doc.initialize(id(1), Vector2(128, 128), Vector2(64, 64), 100), "initialize")
	var atlas := Image.create(1, 1, false, Image.FORMAT_RGBA8)
	atlas.fill(Color(1, 1, 1, 0.5))
	atlas.save_png(output.path_join("atlas.png"))
	ok(doc.add_image_asset(id(2), "atlas", output.path_join("atlas.png"), 1, 1), "asset")
	ok(doc.write_part(part(10, 0, 0)), "part")
	ok(doc.write_offscreen(surface(20, 10)), "surface")
	mesh(30, 10, Rect2(16, 16, 96, 96), Color(1, 1, 1, 0), 0)
	mesh(31, 10, Rect2(16, 16, 96, 96), Color.RED, 1)
	mesh(32, 10, Rect2(16, 16, 96, 96), Color(1, 1, 1, 0), 2, true)
	var group: Dictionary = doc.get_offscreen_snapshot(id(20))
	group.masks = [id(30)]
	ok(doc.write_offscreen(group, true), "group mask source is inside its own target")
	var properties: Dictionary = doc.get_mesh_snapshot(id(30)).properties
	properties.masks = [id(32)]
	ok(doc.set_mesh_properties(id(30), properties), "mask source itself has an empty mask")
	var textures = ClassDB.instantiate("KasaneTextureStore")
	var texture := ImageTexture.create_from_image(atlas)
	ok(textures.set_texture(id(2), texture), "texture")
	viewport = SubViewport.new()
	viewport.size = Vector2i(128, 128)
	viewport.transparent_bg = true
	viewport.disable_3d = true
	viewport.render_target_update_mode = SubViewport.UPDATE_ALWAYS
	root.add_child(viewport)
	preview = ClassDB.instantiate("KasaneDocumentPreview")
	viewport.add_child(preview)
	preview.set_texture_store(textures)
	preview.set_document(doc)
	ok(preview.get_last_result(), "render")
	var image: Image = await draw()
	pixel(image, Vector2i(48, 48), Color(0.25, 0, 0, 0.25), "mask reads raw alpha, ignores source opacity and source mask")
	check(not preview.get_mesh_view(id(30)).visible, "mask source has no visible color draw")
	# Offscreen validation currently permits duplicates. Godot keeps one source
	# node per ID, so the duplicate must not multiply coverage a second time.
	group.masks = [id(30), id(30)]
	ok(doc.write_offscreen(group, true), "duplicate offscreen mask source accepted")
	image = await draw()
	pixel(image, Vector2i(48, 48), Color(0.25, 0, 0, 0.25), "duplicate source is drawn once by existing backend")
	group.masks = [id(30)]
	ok(doc.write_offscreen(group, true), "restore mask")
	await draw()
	var stable_before: Dictionary = preview.get_render_stats()
	ok(preview.refresh_geometry(), "refresh unchanged model")
	await draw()
	check(preview.get_render_stats().order_syncs == stable_before.order_syncs, "unchanged model skips node ordering")
	check(preview.get_render_stats().material_syncs == stable_before.material_syncs, "unchanged model skips material sync")
	var changed: Dictionary = doc.get_mesh_snapshot(id(31)).properties
	changed.appearance.opacity = 0.5
	ok(doc.set_mesh_properties(id(31), changed), "change drawable appearance")
	image = await draw()
	pixel(image, Vector2i(48, 48), Color(0.125, 0, 0, 0.125), "changed material is synchronized")
	check(preview.get_render_stats().order_syncs == stable_before.order_syncs, "appearance change skips node ordering")
	check(preview.get_render_stats().material_syncs > stable_before.material_syncs, "appearance change updates material")
	changed.appearance.opacity = 1.0
	ok(doc.set_mesh_properties(id(31), changed), "restore drawable appearance")
	await draw()
	changed.draw_order = 3.0
	ok(doc.set_mesh_properties(id(31), changed), "change draw order")
	await draw()
	check(preview.get_render_stats().order_syncs > stable_before.order_syncs, "draw order change synchronizes nodes")
	changed.draw_order = 1.0
	ok(doc.set_mesh_properties(id(31), changed), "restore draw order")
	await draw()
	var uploads_before: int = preview.get_render_stats().uploads
	var sync_before: Dictionary = preview.get_render_stats()
	var evaluations_before: int = doc.get_parameter_samples().evaluation_count
	preview.position = Vector2(0.25, 0.75)
	preview.scale = Vector2(0.75, 0.75)
	image = await draw()
	pixel(image, Vector2i(preview.transform * Vector2(48, 48)), Color(0.25, 0, 0, 0.25), "camera preserves mask coverage")
	check(preview.get_render_stats().uploads == uploads_before, "camera already skips unchanged GPU geometry uploads")
	check(doc.get_parameter_samples().evaluation_count == evaluations_before, "camera reuses published core frame")
	check(preview.get_render_stats().scene_submissions == sync_before.scene_submissions, "camera does not prepare or submit a model frame")
	check(preview.get_render_stats().geometry_syncs == sync_before.geometry_syncs, "camera skips CPU geometry synchronization")
	check(preview.get_render_stats().view_updates > sync_before.view_updates, "camera uses view-only update")
	var mask_creations_before: int = preview.get_render_stats().mask_creations
	preview.scale = Vector2(0.7, 0.7)
	await draw()
	check(preview.get_render_stats().mask_creations == mask_creations_before, "mask resize preserves viewport identity")
	preview.position = Vector2.ZERO
	preview.scale = Vector2.ONE
	await draw()
	preview.transform = Transform2D(Vector2.ZERO, Vector2.ZERO, Vector2.ZERO)
	await draw()
	check(not preview.get_last_result().ok and not preview.get_observation_state().ready, "invalid view does not certify the old image")
	preview.transform = Transform2D.IDENTITY
	await draw()
	check(preview.get_last_result().ok and preview.get_observation_state().ready, "view recovers when restored to its previous valid transform")
	preview.transform = Transform2D(Vector2.ZERO, Vector2.ZERO, Vector2.ZERO)
	check(not preview.refresh_geometry().ok, "model submission rejects invalid view")
	preview.transform = Transform2D.IDENTITY
	await draw()
	check(preview.get_last_result().ok and preview.get_observation_state().ready, "camera change retries a previously rejected document submission")
	# A texture's identity and dimensions do not certify unchanged pixel data.
	var texture_id := texture.get_instance_id()
	atlas.fill(Color(1, 1, 1, 0.75))
	texture.update(atlas)
	ok(preview.refresh_geometry(), "explicit refresh after in-place texture update")
	image = await draw()
	check(texture.get_instance_id() == texture_id, "texture identity unchanged")
	pixel(image, Vector2i(48, 48), Color(0.5625, 0, 0, 0.5625), "mask reflects new alpha from same texture handle")
	var mesh_stats_before: Dictionary = preview.get_mesh_view(id(31)).get_render_stats()
	var view_id: int = preview.get_mesh_view(id(31)).get_instance_id()
	ok(textures.set_texture(id(2), ImageTexture.create_from_image(atlas)), "replace native texture handle")
	image = await draw()
	pixel(image, Vector2i(48, 48), Color(0.5625, 0, 0, 0.5625), "replacement preserves image")
	check(preview.get_mesh_view(id(31)).get_instance_id() == view_id, "texture replacement preserves mesh view")
	var mesh_stats_after: Dictionary = preview.get_mesh_view(id(31)).get_render_stats()
	check(mesh_stats_after.creations == mesh_stats_before.creations, "texture binding change does not rebuild geometry")
	var report := {
		"status": "passed" if failures.is_empty() else "failed",
		"checks": checks,
		"failures": failures,
		"texture_replacement_mesh_creations": mesh_stats_after.creations - mesh_stats_before.creations,
		"note": "Creation delta is a diagnostic, not a required behavior for the new renderer."
	}
	var file := FileAccess.open(output.path_join("report.json"), FileAccess.WRITE)
	file.store_string(JSON.stringify(report, "  "))
	file.close()
	print(JSON.stringify(report))
	quit(0 if failures.is_empty() else 1)
