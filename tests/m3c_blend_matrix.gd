extends SceneTree

func _initialize() -> void:
	run.call_deferred()

func pixel_texture(values: Array) -> ImageTexture:
	var image := Image.create(1, 1, false, Image.FORMAT_RGBA8)
	image.fill(Color(values[0], values[1], values[2], values[3]))
	return ImageTexture.create_from_image(image)

func run() -> void:
	var args := OS.get_cmdline_user_args()
	var cases: Array = JSON.parse_string(FileAccess.get_file_as_string(args[0]))
	var viewport := SubViewport.new()
	viewport.size = Vector2i(cases.size() * 8, 90 * 8)
	viewport.disable_3d = true
	viewport.transparent_bg = true
	viewport.render_target_update_mode = SubViewport.UPDATE_ALWAYS
	root.add_child(viewport)
	var shaders := []
	var base := "res://../modules/kasane-render-godot/shaders/"
	for name in ["drawable_extended.gdshader", "offscreen.gdshader"]:
		var shader := Shader.new()
		shader.code = FileAccess.get_file_as_string(base + name).replace("__BLEND_FUNCTIONS__", FileAccess.get_file_as_string(base + "blend_functions.gdshaderinc"))
		shaders.append(shader)
	var destination_shader := Shader.new()
	destination_shader.code = "shader_type canvas_item; render_mode blend_disabled, unshaded; void fragment(){COLOR=texture(TEXTURE,UV);}"
	var destination_material := ShaderMaterial.new()
	destination_material.shader = destination_shader
	for color in 18:
		for alpha in 5:
			for index in cases.size():
				var sample: Dictionary = cases[index]
				var position := Vector2(index * 8, (color * 5 + alpha) * 8)
				var destination := Sprite2D.new()
				destination.centered = false
				destination.texture = pixel_texture(sample.destination)
				destination.scale = Vector2(8, 8)
				destination.position = position
				destination.material = destination_material
				viewport.add_child(destination)
				var copy := BackBufferCopy.new()
				copy.copy_mode = BackBufferCopy.COPY_MODE_RECT
				copy.rect = Rect2(position, Vector2(8, 8))
				viewport.add_child(copy)
				var source := Sprite2D.new()
				source.centered = false
				source.position = position
				source.texture = pixel_texture(sample.source)
				source.scale = Vector2(8, 8)
				var material := ShaderMaterial.new()
				material.shader = shaders[int(sample.premultiplied)]
				material.set_shader_parameter("main_texture", source.texture)
				material.set_shader_parameter("color_blend_mode", color)
				material.set_shader_parameter("alpha_blend_mode", alpha)
				material.set_shader_parameter("multiply_color", Vector3.ONE)
				material.set_shader_parameter("screen_color", Vector3.ZERO)
				material.set_shader_parameter("opacity", sample.opacity)
				material.set_shader_parameter("enabled", true)
				material.set_shader_parameter("texture_to_model", Basis.IDENTITY)
				material.set_shader_parameter("masked", true)
				material.set_shader_parameter("inverted", sample.inverted)
				material.set_shader_parameter("mask_bounds", Vector4(0, 0, 1, 1))
				material.set_shader_parameter("mask_texture", pixel_texture([0, 0, 0, 1.0 - sample.mask if sample.inverted else sample.mask]))
				source.material = material
				viewport.add_child(source)
	for unused in 4:
		await process_frame
		await RenderingServer.frame_post_draw
	var result := viewport.get_texture().get_image().save_png(args[1])
	quit(0 if result == OK else 1)
