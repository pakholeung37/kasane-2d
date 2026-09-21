extends SceneTree

var failures: Array[String] = []

func _initialize() -> void:
	run.call_deferred()

func make_viewport() -> SubViewport:
	var viewport := SubViewport.new()
	viewport.size = Vector2i(64, 64)
	viewport.disable_3d = true
	viewport.transparent_bg = true
	viewport.render_target_update_mode = SubViewport.UPDATE_ALWAYS
	root.add_child(viewport)
	return viewport

func rectangle(parent: Node, rect: Rect2, color: Color) -> ColorRect:
	var node := ColorRect.new()
	node.position = rect.position
	node.size = rect.size
	node.color = color
	parent.add_child(node)
	return node

func copy_destination(parent: Node) -> BackBufferCopy:
	var copy := BackBufferCopy.new()
	copy.copy_mode = BackBufferCopy.COPY_MODE_VIEWPORT
	parent.add_child(copy)
	return copy

func blend_material(extended: bool, opacity: float = 1.0) -> ShaderMaterial:
	var shader := Shader.new()
	var base := "res://../modules/kasane-godot/shaders/"
	var code := FileAccess.get_file_as_string(base + ("offscreen.gdshader" if extended else "offscreen_normal.gdshader"))
	code = code.replace("__BLEND_FUNCTIONS__", FileAccess.get_file_as_string(base + "blend_functions.gdshaderinc"))
	shader.code = code
	var material := ShaderMaterial.new()
	material.shader = shader
	material.set_shader_parameter("multiply_color", Vector3.ONE)
	material.set_shader_parameter("screen_color", Vector3.ZERO)
	material.set_shader_parameter("opacity", opacity)
	material.set_shader_parameter("enabled", true)
	material.set_shader_parameter("texture_to_model", Basis.IDENTITY)
	return material

func expect_color(image: Image, point: Vector2i, expected: Color, label: String) -> void:
	var actual := image.get_pixelv(point)
	if maxf(absf(actual.r - expected.r), maxf(absf(actual.g - expected.g), maxf(absf(actual.b - expected.b), absf(actual.a - expected.a)))) > 0.025:
		failures.append(label + ": " + str(actual) + " expected " + str(expected))

func run() -> void:
	var output_dir := OS.get_cmdline_user_args()[0]
	DirAccess.make_dir_recursive_absolute(output_dir)
	var source := make_viewport()
	rectangle(source, Rect2(48, 0, 16, 64), Color(1, 1, 0, 0.5))
	var middle := make_viewport()
	rectangle(middle, Rect2(0, 0, 64, 64), Color.RED)
	copy_destination(middle)
	var first := rectangle(middle, Rect2(0, 0, 32, 64), Color.WHITE)
	first.material = blend_material(true, 0.5)
	rectangle(middle, Rect2(0, 0, 16, 64), Color.BLUE)
	var second_copy := copy_destination(middle)
	var middle_sprite := Sprite2D.new()
	middle_sprite.centered = false
	middle_sprite.texture = source.get_texture()
	middle_sprite.material = blend_material(true)
	middle.add_child(middle_sprite)
	var capture := make_viewport()
	var capture_sprite := Sprite2D.new()
	capture_sprite.centered = false
	capture_sprite.texture = middle.get_texture()
	capture_sprite.material = blend_material(false, 0.5)
	capture.add_child(capture_sprite)
	for unused in 8:
		await process_frame
		await RenderingServer.frame_post_draw
	expect_color(middle.get_texture().get_image(), Vector2i(8, 32), Color.BLUE, "normal sibling survives later extended blend")
	expect_color(middle.get_texture().get_image(), Vector2i(56, 32), Color(1, 0.5, 0, 1), "extended blend reads latest destination")
	expect_color(capture.get_texture().get_image(), Vector2i(8, 32), Color(0, 0, 0.5, 0.5), "nested normal premultiplied opacity")
	capture.get_texture().get_image().save_png(output_dir.path_join("capture.png"))
	# A negative control ensures the fixture detects stale automatic snapshots.
	second_copy.copy_mode = BackBufferCopy.COPY_MODE_DISABLED
	for unused in 4:
		await process_frame
		await RenderingServer.frame_post_draw
	if middle.get_texture().get_image().get_pixel(8, 32).b > 0.9:
		failures.append("negative control did not reproduce destination loss")
	for failure in failures:
		push_error(failure)
	print("destination-copy regression: ", "passed" if failures.is_empty() else "failed")
	quit(0 if failures.is_empty() else 1)
