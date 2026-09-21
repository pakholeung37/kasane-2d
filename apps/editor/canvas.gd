extends Control
## The same M4 renderer draws the canvas and observation images.
const EditorTheme = preload("res://ui/theme.gd")
signal view_changed(zoom: float, offset: Vector2)
var zoom := 1.0
var offset := Vector2.ZERO
var workspace: RefCounted
var viewport: SubViewport
var preview: Node2D
var selection: Node2D
var textures = ClassDB.instantiate("KasaneTextureStore")
var selected_id := ""
var generation := -1
var observing := false

func _ready() -> void:
	clip_contents = true
	mouse_default_cursor_shape = Control.CURSOR_MOVE
	viewport = SubViewport.new()
	viewport.transparent_bg = true
	viewport.render_target_update_mode = SubViewport.UPDATE_ALWAYS
	add_child(viewport)
	preview = ClassDB.instantiate("KasaneDocumentPreview")
	viewport.add_child(preview)
	preview.set_texture_store(textures)
	selection = ClassDB.instantiate("KasaneSelectionOverlay")
	preview.add_child(selection)
	selection.set_preview(preview)
	resized.connect(update_camera)
	update_camera()

func attach(owner: RefCounted) -> void:
	workspace = owner
	workspace.surface = weakref(self)
	workspace.document.changed.connect(document_changed)
	preview.set_document(workspace.document)
	document_changed({})

func document_changed(_change: Dictionary) -> void:
	var summary: Dictionary = workspace.document.get_document_summary()
	if generation != summary.generation:
		generation = summary.generation
		textures.clear()
		selected_id = ""
		selection.select("")
		reset_view()
	for asset in summary.assets:
		textures.load_asset(workspace.document, asset.id)
	preview.refresh()
	queue_redraw()

func update_camera() -> void:
	if viewport == null:
		return
	viewport.size = Vector2i(maxi(1, int(size.x)), maxi(1, int(size.y)))
	preview.position = size / 2.0 + offset
	preview.scale = Vector2.ONE * zoom
	preview.refresh_geometry()
	queue_redraw()
	view_changed.emit(zoom, offset)

func set_zoom_centered(new_zoom: float) -> void:
	zoom = clampf(new_zoom, 0.001, 100.0)
	update_camera()

func reset_view() -> void:
	zoom = 1.0
	offset = Vector2.ZERO
	if workspace != null:
		var summary: Dictionary = workspace.document.get_document_state()
		if summary.initialized:
			zoom = minf(size.x / summary.canvas_size.x, size.y / summary.canvas_size.y) * 0.9
			offset = -summary.canvas_size * zoom / 2.0
	update_camera()

func bounds_for(id: String = "") -> Dictionary:
	var frame: Dictionary = workspace.document.get_frame()
	if not frame.ok:
		return frame
	var summary: Dictionary = workspace.document.get_document_state()
	var bounds := Rect2()
	var populated := false
	for drawable in frame.drawables:
		if id.is_empty() and not drawable.get("visible", false):
			continue
		if not id.is_empty() and drawable.id != id:
			continue
		for p in drawable.positions:
			var point := Vector2(p.x * summary.pixels_per_unit + summary.canvas_origin.x, summary.canvas_origin.y - p.y * summary.pixels_per_unit)
			if not populated:
				bounds = Rect2(point, Vector2.ZERO)
				populated = true
			else:
				bounds = bounds.expand(point)
	return {"ok": populated, "bounds": bounds, "code": "OK" if populated else "NO_DRAWABLE"}

func fit_content(id: String = "") -> void:
	var result := bounds_for(id)
	if not result.ok:
		return
	var bounds: Rect2 = result.bounds
	zoom = clampf(minf(size.x / maxf(bounds.size.x, 1), size.y / maxf(bounds.size.y, 1)) * 0.8, 0.001, 100)
	offset = -bounds.get_center() * zoom
	update_camera()

func select(id: String) -> void:
	selected_id = id
	selection.select(id)

func fingerprint() -> Dictionary:
	var summary: Dictionary = workspace.document.get_document_state()
	var frame: Dictionary = workspace.document.get_frame()
	return {"generation": summary.generation, "revision": summary.revision, "parameters": frame.get("parameters", []),
		"camera": {"zoom": zoom, "offset": [offset.x, offset.y]}, "image_size": [viewport.size.x, viewport.size.y]}

func observe(path: String, object_id: String = "") -> Dictionary:
	if observing:
		return workspace.failure("OBSERVATION_BUSY", "An observation is already running.")
	if DisplayServer.get_name() == "headless":
		return workspace.failure("GPU_UNAVAILABLE", "Headless rendering cannot produce an observation.")
	observing = true
	selection.hide()
	var requested: Dictionary = workspace.document.get_document_state()
	var requested_values: Array = workspace.document.get_frame().get("parameters", [])
	var requested_camera := Vector3(zoom, offset.x, offset.y)
	# Container minimum sizes and viewport resizes settle after the script returns.
	# Capture the settled camera, while still rejecting any document replacement/edit.
	await get_tree().process_frame
	await get_tree().process_frame
	var current: Dictionary = workspace.document.get_document_state()
	if requested.generation != current.generation or requested.revision != current.revision or requested_values != workspace.document.get_frame().get("parameters", []) or requested_camera != Vector3(zoom, offset.x, offset.y):
		selection.show()
		observing = false
		var failure: Dictionary = workspace.failure("OBSERVATION_CHANGED", "Document, preview values or camera changed while layout settled.")
		failure.requested_revision = requested.revision
		failure.current_revision = current.revision
		failure.camera_before = [requested_camera.x, requested_camera.y, requested_camera.z]
		failure.camera_after = [zoom, offset.x, offset.y]
		failure.parameters_changed = requested_values != workspace.document.get_frame().get("parameters", [])
		return failure
	var expected := fingerprint()
	var refreshed: Dictionary = preview.refresh()
	var result: Dictionary = refreshed
	if refreshed.ok:
		result = workspace.failure("FRAME_NOT_READY", "The renderer did not finish the requested state.")
		for _frame in 120:
			await get_tree().process_frame
			var actual := fingerprint()
			if actual != expected:
				result = workspace.failure("OBSERVATION_CHANGED", "Document, parameters or camera changed during capture.")
				result.changed_fields = []
				for field in expected:
					if expected[field] != actual[field]:
						result.changed_fields.append(field)
				result.camera_before = expected.camera
				result.camera_after = actual.camera
				result.size_before = expected.image_size
				result.size_after = actual.image_size
				break
			var state: Dictionary = preview.get_observation_state()
			# macOS can suspend automatic draws for an occluded application while
			# process_frame keeps running. Capture still needs real GPU submissions;
			# force the normal renderer and let its pre/post-draw signals certify them.
			if state.ok and not state.ready:
				RenderingServer.force_draw(false)
				state = preview.get_observation_state()
			result.renderer = state
			result.expected_revision = expected.revision
			result.viewport_size = [viewport.size.x, viewport.size.y]
			if not state.ok:
				result = state
				break
			if state.ready and state.get("revision", -1) == expected.revision:
				var image := viewport.get_texture().get_image()
				if image == null or image.is_empty():
					result = workspace.failure("IMAGE_MISSING", "Renderer returned no pixels.")
					break
				var crop := Rect2i(Vector2i.ZERO, image.get_size())
				if not object_id.is_empty():
					var bound := bounds_for(object_id)
					if not bound.ok:
						result = bound
						break
					var screen := Rect2(preview.position + bound.bounds.position * zoom, bound.bounds.size * zoom).grow(4)
					crop = Rect2i(Vector2i(screen.position.floor()), Vector2i(screen.end.ceil() - screen.position.floor())).intersection(crop)
					if crop.size.x <= 0 or crop.size.y <= 0:
						result = workspace.failure("OBJECT_OFFSCREEN", "Locate the object before taking a crop.")
						break
					image = image.get_region(crop)
				var error := image.save_png(path)
				result = expected.duplicate(true)
				result.ok = error == OK
				result.code = "OK" if error == OK else "IMAGE_WRITE_FAILED"
				result.path = path
				result.object_id = object_id
				result.crop = [crop.position.x, crop.position.y, crop.size.x, crop.size.y]
				result.output_size = [image.get_width(), image.get_height()]
				result.renderer = state
				if result.ok:
					result.sha256 = FileAccess.get_sha256(path)
				break
	selection.show()
	observing = false
	return result

func _gui_input(event: InputEvent) -> void:
	if event is InputEventMouseButton and event.pressed:
		var factor := 1.0
		if event.button_index == MOUSE_BUTTON_WHEEL_UP:
			factor = 1.1
		elif event.button_index == MOUSE_BUTTON_WHEEL_DOWN:
			factor = 1.0 / 1.1
		if factor != 1.0:
			var next_zoom := clampf(zoom * factor, 0.001, 100)
			var anchor: Vector2 = event.position - size / 2.0
			offset = anchor - (anchor - offset) * next_zoom / zoom
			zoom = next_zoom
			update_camera()
			accept_event()
	if event is InputEventMouseMotion and event.button_mask & MOUSE_BUTTON_MASK_MIDDLE:
		offset += event.relative
		update_camera()
		accept_event()

func _process(_delta: float) -> void:
	queue_redraw()

func _draw() -> void:
	draw_rect(Rect2(Vector2.ZERO, size), EditorTheme.BG_BASE)
	if viewport != null:
		draw_texture_rect(viewport.get_texture(), Rect2(Vector2.ZERO, size), false)
