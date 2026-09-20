extends PanelContainer

const EditorTheme = preload("res://ui/theme.gd")

signal preview_values_changed(values: Dictionary)
signal report_requested(msg: String)

var dock_info: Dictionary
var scroll: ScrollContainer
var list_box: VBoxContainer
var count_label: Label
var reset_all_btn: Button

var workspace_ref: WeakRef
var parameter_controls := {}
var cached_summary := {}
var current_selected_id := ""

func _ready() -> void:
	dock_info = EditorTheme.make_dock("参数列表 (Parameters)")
	add_child(dock_info.panel)

	# Add extra header actions
	var header_box: HBoxContainer = dock_info.header_box
	var toggle_btn: Button = dock_info.toggle_btn

	count_label = Label.new()
	count_label.text = "0 项"
	count_label.add_theme_font_size_override("font_size", 11)
	count_label.add_theme_color_override("font_color", EditorTheme.TEXT_DIM)
	header_box.add_child(count_label)
	header_box.move_child(count_label, 1)

	reset_all_btn = Button.new()
	reset_all_btn.text = "全部归零"
	reset_all_btn.tooltip_text = "将所有参数恢复至默认初值"
	reset_all_btn.add_theme_font_size_override("font_size", 11)
	reset_all_btn.pressed.connect(_on_reset_all_pressed)
	header_box.add_child(reset_all_btn)
	header_box.move_child(reset_all_btn, 3)

	var content: VBoxContainer = dock_info.content

	scroll = ScrollContainer.new()
	scroll.size_flags_vertical = Control.SIZE_EXPAND_FILL
	scroll.horizontal_scroll_mode = ScrollContainer.SCROLL_MODE_DISABLED
	content.add_child(scroll)

	list_box = VBoxContainer.new()
	list_box.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	list_box.add_theme_constant_override("separation", 6)
	scroll.add_child(list_box)

func setup(workspace: RefCounted) -> void:
	workspace_ref = weakref(workspace)

func rebuild(summary: Dictionary, selected_id: String = "") -> void:
	cached_summary = summary
	current_selected_id = selected_id

	for child in list_box.get_children():
		list_box.remove_child(child)
		child.queue_free()
	parameter_controls.clear()

	var params: Array = summary.get("parameters", [])
	count_label.text = "%d 项" % params.size()

	# Pre-gather keyforms associated with currently selected object
	var param_to_keys := _gather_keys_for_object(summary, selected_id)

	const ParamSliderRow = preload("res://ui/components/param_slider_row.gd")

	for p in params:
		var p_id: String = p.get("id", "")
		var p_name: String = p.get("name", p_id)
		var p_min: float = p.get("minimum", -1.0)
		var p_max: float = p.get("maximum", 1.0)
		var p_def: float = p.get("default_value", 0.0)
		var p_dec: int = p.get("decimal_places", 2)

		var row = ParamSliderRow.new(p_id, p_name, p_min, p_max, p_def, p_dec)
		list_box.add_child(row)
		parameter_controls[p_id] = row

		# Set keyform dots
		var key_list: Array[float] = []
		if param_to_keys.has(p_id):
			for k in param_to_keys[p_id]:
				key_list.append(float(k))
		row.set_keyforms(key_list)

		row.value_committed.connect(_on_row_value_committed)

	update_values()

func update_selection_keyforms(selected_id: String) -> void:
	current_selected_id = selected_id
	var param_to_keys := _gather_keys_for_object(cached_summary, selected_id)

	for p_id in parameter_controls:
		var row = parameter_controls[p_id]
		var key_list: Array[float] = []
		if param_to_keys.has(p_id):
			for k in param_to_keys[p_id]:
				key_list.append(float(k))
		row.set_keyforms(key_list)

func _gather_keys_for_object(summary: Dictionary, object_id: String) -> Dictionary:
	var result := {}
	if object_id.is_empty():
		return result

	var all_bindings: Array = summary.get("bindings", []) + summary.get("scene_bindings", [])
	for b in all_bindings:
		var target: String = b.get("mesh_id", b.get("target_id", ""))
		if target == object_id:
			for axis in b.get("axes", []):
				var pid: String = axis.get("parameter_id", "")
				if not result.has(pid):
					result[pid] = []
				for k in axis.get("keys", []):
					if not result[pid].has(k):
						result[pid].append(k)
	return result

func update_values() -> void:
	var ws: RefCounted = workspace_ref.get_ref() if workspace_ref != null else null
	if ws == null or ws.document == null:
		return
	var frame: Dictionary = ws.document.get_frame()
	if frame.get("ok", false):
		for sample in frame.get("parameters", []):
			var pid: String = sample.get("id", "")
			if parameter_controls.has(pid):
				parameter_controls[pid].set_value_silent(sample.get("value", 0.0))

func _on_row_value_committed(param_id: String, new_val: float) -> void:
	var ws: RefCounted = workspace_ref.get_ref() if workspace_ref != null else null
	if ws == null or ws.document == null:
		return
	var values := {}
	var frame: Dictionary = ws.document.get_frame()
	for sample in frame.get("parameters", []):
		values[sample.get("id")] = sample.get("value")
	values[param_id] = new_val

	var res: Dictionary = ws.document.set_preview_values(values)
	if not res.get("ok", false):
		report_requested.emit(JSON.stringify(res))

func _on_reset_all_pressed() -> void:
	var ws: RefCounted = workspace_ref.get_ref() if workspace_ref != null else null
	if ws == null or ws.document == null:
		return
	ws.document.set_preview_values({})
	update_values()
