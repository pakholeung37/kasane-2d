extends VBoxContainer

const EditorTheme = preload("res://ui/theme.gd")

signal value_committed(id: String, value: float)

var param_id := ""
var param_name := ""
var min_val := -1.0
var max_val := 1.0
var def_val := 0.0
var decimals := 2

var keys: Array[float] = []

var name_label: Label
var value_display: LineEdit
var reset_btn: Button
var slider: HSlider
var dots_overlay: Control

func _init(p_id: String, p_name: String, p_min: float, p_max: float, p_def: float, p_decimals: int = 2) -> void:
	param_id = p_id
	param_name = p_name
	min_val = p_min
	max_val = p_max
	def_val = p_def
	decimals = p_decimals
	add_theme_constant_override("separation", 2)

func _ready() -> void:
	# Top line: Name + Value display + Reset button
	var top_line := HBoxContainer.new()
	top_line.add_theme_constant_override("separation", 4)
	add_child(top_line)

	name_label = Label.new()
	name_label.text = param_name
	name_label.tooltip_text = "%s (%s)\n范围: [%.2f, %.2f] 默认: %.2f\n双击重置为默认值" % [param_name, param_id, min_val, max_val, def_val]
	name_label.add_theme_font_size_override("font_size", 11)
	name_label.add_theme_color_override("font_color", EditorTheme.TEXT_MUTED)
	name_label.mouse_filter = Control.MOUSE_FILTER_STOP
	name_label.mouse_default_cursor_shape = Control.CURSOR_POINTING_HAND
	name_label.gui_input.connect(_on_label_gui_input)
	name_label.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	name_label.clip_text = true
	top_line.add_child(name_label)

	value_display = LineEdit.new()
	value_display.custom_minimum_size = Vector2(46, 18)
	value_display.alignment = HORIZONTAL_ALIGNMENT_RIGHT
	value_display.add_theme_font_size_override("font_size", 11)
	value_display.text = _format_value(def_val)
	value_display.text_submitted.connect(_on_text_submitted)
	value_display.focus_exited.connect(func(): _on_text_submitted(value_display.text))
	top_line.add_child(value_display)

	reset_btn = Button.new()
	reset_btn.text = "↺"
	reset_btn.tooltip_text = "重置为默认值 (%.2f)" % def_val
	reset_btn.flat = true
	reset_btn.custom_minimum_size = Vector2(18, 18)
	reset_btn.add_theme_font_size_override("font_size", 10)
	reset_btn.pressed.connect(reset_to_default)
	top_line.add_child(reset_btn)

	# Bottom line: Slider with Keyform Dots Overlay
	var slider_container := MarginContainer.new()
	slider_container.custom_minimum_size.y = 16
	add_child(slider_container)

	slider = HSlider.new()
	slider.min_value = min_val
	slider.max_value = max_val
	slider.step = pow(10, -clampi(decimals, 1, 4))
	slider.value = def_val
	slider.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	slider.size_flags_vertical = Control.SIZE_SHRINK_CENTER
	slider_container.add_child(slider)

	# Overlay for drawing keyform dots
	dots_overlay = Control.new()
	dots_overlay.mouse_filter = Control.MOUSE_FILTER_IGNORE
	dots_overlay.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	slider_container.add_child(dots_overlay)
	dots_overlay.draw.connect(_on_draw_dots)

	slider.value_changed.connect(_on_slider_value_changed)
	slider.gui_input.connect(_on_slider_gui_input)

func set_keyforms(p_keys: Array[float]) -> void:
	keys = p_keys.duplicate()
	keys.sort()
	if dots_overlay != null:
		dots_overlay.queue_redraw()

func _format_value(val: float) -> String:
	return ("%." + str(clampi(decimals, 0, 3)) + "f") % val

func _on_slider_value_changed(new_val: float) -> void:
	value_display.text = _format_value(new_val)
	if dots_overlay != null:
		dots_overlay.queue_redraw()
	value_committed.emit(param_id, new_val)

func set_value_silent(new_val: float) -> void:
	if slider != null:
		slider.set_value_no_signal(new_val)
		value_display.text = _format_value(new_val)
		if dots_overlay != null:
			dots_overlay.queue_redraw()

func reset_to_default() -> void:
	if slider != null:
		slider.value = def_val

func _on_label_gui_input(event: InputEvent) -> void:
	if event is InputEventMouseButton and event.pressed and event.double_click and event.button_index == MOUSE_BUTTON_LEFT:
		reset_to_default()

func _on_slider_gui_input(event: InputEvent) -> void:
	if event is InputEventMouseButton and event.pressed and event.double_click and event.button_index == MOUSE_BUTTON_LEFT:
		reset_to_default()

func _on_text_submitted(text: String) -> void:
	var val := text.to_float()
	val = clampf(val, min_val, max_val)
	slider.value = val

func _on_draw_dots() -> void:
	if keys.is_empty() or slider == null:
		return
	var w: float = dots_overlay.size.x
	var h: float = dots_overlay.size.y
	var track_inset := 8.0 # Grabber edge offset approximation
	var available_w := maxf(1.0, w - track_inset * 2.0)
	var range_span := maxf(0.00001, max_val - min_val)

	var current_val: float = slider.value
	var snap_threshold := range_span * 0.02

	for k in keys:
		var ratio := (k - min_val) / range_span
		var dot_x := track_inset + ratio * available_w
		var dot_y := h / 2.0

		var is_aligned := absf(current_val - k) <= snap_threshold
		if is_aligned:
			# Highlighted keyform dot (aligned)
			dots_overlay.draw_circle(Vector2(dot_x, dot_y), 3.5, EditorTheme.accent)
			dots_overlay.draw_arc(Vector2(dot_x, dot_y), 4.5, 0, TAU, 16, EditorTheme.accent_hover, 1.0)
		else:
			# Normal keyform dot on track
			dots_overlay.draw_circle(Vector2(dot_x, dot_y), 2.2, EditorTheme.BORDER_FOCUS)
			dots_overlay.draw_circle(Vector2(dot_x, dot_y), 1.5, EditorTheme.TEXT_MUTED)
