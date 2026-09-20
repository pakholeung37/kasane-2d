extends PanelContainer

const EditorTheme = preload("res://ui/theme.gd")

signal reset_view_requested()
signal fit_content_requested()
signal locate_selected_requested()
signal zoom_changed(zoom_factor: float)

var canvas_ref: WeakRef
var zoom_input: LineEdit
var zoom_options: OptionButton
var is_updating_silent := false

func setup(canvas: Control) -> void:
	canvas_ref = weakref(canvas)
	if canvas.has_signal("view_changed"):
		canvas.connect("view_changed", _on_canvas_view_changed)
	_sync_zoom_display(canvas.zoom)

func _ready() -> void:
	var style := EditorTheme.make_flat_style(EditorTheme.BG_CARD, EditorTheme.BORDER, 3, 6, 3)
	add_theme_stylebox_override("panel", style)

	var hbox := HBoxContainer.new()
	hbox.add_theme_constant_override("separation", 6)
	add_child(hbox)

	var reset_btn := Button.new()
	reset_btn.text = "重置视图"
	reset_btn.tooltip_text = "恢复默认 1:1 视口居中"
	reset_btn.pressed.connect(func(): reset_view_requested.emit())
	hbox.add_child(reset_btn)

	var fit_btn := Button.new()
	fit_btn.text = "适配内容"
	fit_btn.tooltip_text = "缩放视口以适配当前模型全部元素"
	fit_btn.pressed.connect(func(): fit_content_requested.emit())
	hbox.add_child(fit_btn)

	var locate_btn := Button.new()
	locate_btn.text = "定位选中"
	locate_btn.tooltip_text = "将视口聚焦居中到选中的网格/部件"
	locate_btn.pressed.connect(func(): locate_selected_requested.emit())
	hbox.add_child(locate_btn)

	var vsep := VSeparator.new()
	hbox.add_child(vsep)

	# Zoom controls
	var zoom_out_btn := Button.new()
	zoom_out_btn.text = "－"
	zoom_out_btn.custom_minimum_size = Vector2(22, 22)
	zoom_out_btn.pressed.connect(_zoom_out)
	hbox.add_child(zoom_out_btn)

	zoom_input = LineEdit.new()
	zoom_input.custom_minimum_size = Vector2(48, 22)
	zoom_input.alignment = HORIZONTAL_ALIGNMENT_CENTER
	zoom_input.text = "100%"
	zoom_input.text_submitted.connect(_on_zoom_text_submitted)
	zoom_input.focus_exited.connect(func(): _on_zoom_text_submitted(zoom_input.text))
	hbox.add_child(zoom_input)

	var zoom_in_btn := Button.new()
	zoom_in_btn.text = "＋"
	zoom_in_btn.custom_minimum_size = Vector2(22, 22)
	zoom_in_btn.pressed.connect(_zoom_in)
	hbox.add_child(zoom_in_btn)

	zoom_options = OptionButton.new()
	zoom_options.custom_minimum_size = Vector2(65, 22)
	for preset in ["25%", "50%", "100%", "200%", "400%", "800%"]:
		zoom_options.add_item(preset)
	zoom_options.select(2) # 100%
	zoom_options.item_selected.connect(_on_zoom_preset_selected)
	hbox.add_child(zoom_options)

	var spacer := Control.new()
	spacer.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	hbox.add_child(spacer)

	var hint := Label.new()
	hint.text = "滚轮缩放 · 中键平移"
	hint.add_theme_font_size_override("font_size", 11)
	hint.add_theme_color_override("font_color", EditorTheme.TEXT_DIM)
	hbox.add_child(hint)

func _sync_zoom_display(zoom_val: float) -> void:
	if zoom_input == null:
		return
	is_updating_silent = true
	var percent := int(round(zoom_val * 100.0))
	zoom_input.text = str(percent) + "%"
	is_updating_silent = false

func _on_canvas_view_changed(zoom: float, _offset: Vector2) -> void:
	_sync_zoom_display(zoom)

func _zoom_in() -> void:
	var canvas: Control = canvas_ref.get_ref() if canvas_ref != null else null
	if canvas != null:
		canvas.set_zoom_centered(canvas.zoom * 1.25)

func _zoom_out() -> void:
	var canvas: Control = canvas_ref.get_ref() if canvas_ref != null else null
	if canvas != null:
		canvas.set_zoom_centered(canvas.zoom / 1.25)

func _on_zoom_preset_selected(idx: int) -> void:
	var text := zoom_options.get_item_text(idx).trim_suffix("%")
	var factor := text.to_float() / 100.0
	var canvas: Control = canvas_ref.get_ref() if canvas_ref != null else null
	if canvas != null:
		canvas.set_zoom_centered(factor)

func _on_zoom_text_submitted(text: String) -> void:
	if is_updating_silent:
		return
	var clean := text.trim_suffix("%").strip_edges()
	var factor := clean.to_float() / 100.0
	if factor > 0.0:
		var canvas: Control = canvas_ref.get_ref() if canvas_ref != null else null
		if canvas != null:
			canvas.set_zoom_centered(factor)
