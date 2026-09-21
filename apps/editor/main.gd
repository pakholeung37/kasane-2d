extends Control

const EditorTheme = preload("res://ui/theme.gd")
const EditorLayoutManager = preload("res://ui/layout_manager.gd")
const CanvasSurface = preload("res://canvas.gd")
const TopMenuBar = preload("res://ui/menu_bar.gd")
const CanvasToolbar = preload("res://ui/components/canvas_toolbar.gd")
const HierarchyDock = preload("res://ui/docks/hierarchy_dock.gd")
const ParameterDock = preload("res://ui/docks/parameter_dock.gd")
const InspectorDock = preload("res://ui/docks/inspector_dock.gd")
const ConsoleDock = preload("res://ui/docks/console_dock.gd")

var workspace = preload("res://workspace.gd").new()
var canvas: Control
var agent: Node

# Docks and Components
var menu_bar: TopMenuBar
var canvas_toolbar: CanvasToolbar
var hierarchy_dock: HierarchyDock
var parameter_dock: ParameterDock
var inspector_dock: InspectorDock
var console_dock: ConsoleDock
var layout_mgr: EditorLayoutManager

# Compatibility properties for tests and services
var object_tree: Tree
var inspector: RichTextLabel
var output: RichTextLabel
var status: Label

var selected_id := ""
var inspected_object := ""
var binding_index := 0
var keyform_index := 0

func _exit_tree() -> void:
	if canvas != null and canvas.selection != null:
		canvas.selection.set_preview(null)
	if layout_mgr != null:
		layout_mgr.save_layout()

func _ready() -> void:
	if not workspace.startup_error.is_empty():
		var message := Label.new()
		message.text = workspace.startup_error
		message.position = Vector2(24, 24)
		add_child(message)
		push_error(workspace.startup_error)
		return

	# Apply theme
	theme = EditorTheme.create_theme()

	# Root Layout
	var root_margin := MarginContainer.new()
	root_margin.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	root_margin.add_theme_constant_override("margin_left", 0)
	root_margin.add_theme_constant_override("margin_right", 0)
	root_margin.add_theme_constant_override("margin_top", 0)
	root_margin.add_theme_constant_override("margin_bottom", 0)
	add_child(root_margin)

	var app_vbox := VBoxContainer.new()
	app_vbox.add_theme_constant_override("separation", 0)
	root_margin.add_child(app_vbox)

	# 1. Top MenuBar
	menu_bar = TopMenuBar.new()
	app_vbox.add_child(menu_bar)

	menu_bar.new_project_requested.connect(func(): replace_project(func(): perform(workspace.new_project())))
	menu_bar.open_project_requested.connect(func(): replace_project(func(): choose_file(FileDialog.FILE_MODE_OPEN_FILE, PackedStringArray(["*.json ; Kasane project"]), workspace.open_project)))
	menu_bar.save_project_requested.connect(save_current)
	menu_bar.save_as_requested.connect(save_as)
	menu_bar.import_png_requested.connect(func(): choose_file(FileDialog.FILE_MODE_OPEN_FILE, PackedStringArray(["*.png ; PNG image"]), import_png_ui))
	menu_bar.import_model_requested.connect(func(): replace_project(func(): choose_file(FileDialog.FILE_MODE_OPEN_FILE, PackedStringArray(["*.model3.json ; Cubism model"]), workspace.import_model)))
	menu_bar.export_model_requested.connect(func(): choose_file(FileDialog.FILE_MODE_OPEN_DIR, PackedStringArray(), workspace.export_model))

	# 2. Main Body Splitters
	var main_hsplit := HSplitContainer.new()
	main_hsplit.size_flags_vertical = Control.SIZE_EXPAND_FILL
	app_vbox.add_child(main_hsplit)

	# Left Column: Hierarchy (top) + Parameters (bottom)
	var left_vsplit := VSplitContainer.new()
	left_vsplit.custom_minimum_size.x = 240
	main_hsplit.add_child(left_vsplit)

	hierarchy_dock = HierarchyDock.new()
	hierarchy_dock.size_flags_vertical = Control.SIZE_EXPAND_FILL
	left_vsplit.add_child(hierarchy_dock)
	hierarchy_dock.object_selected.connect(_on_object_selected)

	parameter_dock = ParameterDock.new()
	parameter_dock.size_flags_vertical = Control.SIZE_EXPAND_FILL
	left_vsplit.add_child(parameter_dock)
	parameter_dock.setup(workspace)
	parameter_dock.report_requested.connect(report)

	# Right Area: Center Area + Inspector Dock
	var center_right_hsplit := HSplitContainer.new()
	center_right_hsplit.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	main_hsplit.add_child(center_right_hsplit)

	# Center Column: Canvas (top) + Console Drawer (bottom)
	var center_vsplit := VSplitContainer.new()
	center_vsplit.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	center_right_hsplit.add_child(center_vsplit)

	var canvas_container := VBoxContainer.new()
	canvas_container.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	canvas_container.size_flags_vertical = Control.SIZE_EXPAND_FILL
	canvas_container.add_theme_constant_override("separation", 2)
	center_vsplit.add_child(canvas_container)

	canvas = CanvasSurface.new()
	canvas.custom_minimum_size = Vector2(320, 220)
	canvas.size_flags_vertical = Control.SIZE_EXPAND_FILL

	canvas_toolbar = CanvasToolbar.new()
	canvas_toolbar.setup(canvas)
	canvas_container.add_child(canvas_toolbar)
	canvas_container.add_child(canvas)

	canvas_toolbar.reset_view_requested.connect(func(): canvas.reset_view())
	canvas_toolbar.fit_content_requested.connect(func(): canvas.fit_content())
	canvas_toolbar.locate_selected_requested.connect(func(): canvas.fit_content(selected_id))

	console_dock = ConsoleDock.new()
	console_dock.custom_minimum_size.y = 120
	center_vsplit.add_child(console_dock)

	console_dock.run_script_requested.connect(func(): choose_file(FileDialog.FILE_MODE_OPEN_FILE, PackedStringArray(["*.gd ; GDScript"]), run_script))
	console_dock.undo_requested.connect(func(): workspace.undo_redo.undo())
	console_dock.redo_requested.connect(func(): workspace.undo_redo.redo())

	# Right Column: Inspector Dock
	inspector_dock = InspectorDock.new()
	inspector_dock.custom_minimum_size.x = 260
	center_right_hsplit.add_child(inspector_dock)

	inspector_dock.jump_to_object_requested.connect(_on_jump_to_object)
	inspector_dock.binding_selected.connect(func(idx: int):
		binding_index = idx
		keyform_index = 0
		show_selection())
	inspector_dock.keyform_selected.connect(func(idx: int):
		keyform_index = idx
		show_selection())

	# Assign compatibility references
	object_tree = hierarchy_dock.tree
	inspector = inspector_dock.raw_json_text
	output = console_dock.output_text
	status = menu_bar.status_label

	# Initialize Layout Persistence
	layout_mgr = EditorLayoutManager.new()
	layout_mgr.setup(main_hsplit, left_vsplit, center_right_hsplit, center_vsplit)

	# Apply initial UI scale (saved or auto-detected)
	var saved_scale := layout_mgr.get_saved_ui_scale()
	layout_mgr.apply_ui_scale(get_window(), saved_scale)
	menu_bar.set_scale_display(saved_scale)

	menu_bar.ui_scale_selected.connect(func(factor: float):
		layout_mgr.save_ui_scale(factor)
		layout_mgr.apply_ui_scale(get_window(), factor)
		menu_bar.set_scale_display(factor))

	# Connect Document & Canvas
	canvas.attach(workspace)
	workspace.document.changed.connect(refresh_document)
	workspace.document.preview_changed.connect(parameter_dock.update_values)

	# Start Agent Queue
	agent = preload("res://agent_queue.gd").new()
	add_child(agent)
	var directory := "user://agent/" + str(OS.get_process_id())
	for argument in OS.get_cmdline_user_args():
		if argument.begins_with("--agent-dir="):
			directory = argument.trim_prefix("--agent-dir=")
	agent.completed.connect(perform)
	var queue_result: Dictionary = agent.start(workspace, canvas, directory)
	console_dock.set_agent_directory(agent.directory)
	report(JSON.stringify(queue_result))

	refresh_document()
	report("已就绪。Agent 请求目录见控制台面板。")

func refresh_document(_change: Dictionary = {}) -> void:
	var summary: Dictionary = workspace.document.get_document_summary()

	# Update Top Status
	menu_bar.update_status(summary.get("path", ""), summary.get("modified", false), summary.get("generation", 0), summary.get("revision", 0))

	# Update Hierarchy Tree
	hierarchy_dock.populate(summary, workspace)

	# Rebuild Parameters
	parameter_dock.rebuild(summary, selected_id)

	show_selection()

func show_selection() -> void:
	var summary: Dictionary = workspace.document.get_document_summary()
	var value: Dictionary = {}
	for group in [summary.get("parts", []), summary.get("meshes", []), summary.get("transforms", []), summary.get("blend_key_tables", []), summary.get("blend_constraints", []), summary.get("blend_bindings", []), summary.get("glues", []), summary.get("offscreens", []), summary.get("parameters", [])]:
		for entry in group:
			if entry.get("id") == selected_id:
				value = entry
				if group == summary.get("meshes"):
					value = workspace.document.get_mesh_snapshot(selected_id)

	if inspected_object != selected_id:
		binding_index = 0
		keyform_index = 0
		inspected_object = selected_id

	if not value.is_empty():
		value = value.duplicate(true)
		var bindings: Array = []
		value.bindings = []
		for binding in summary.get("bindings", []) + summary.get("scene_bindings", []) + summary.get("blend_bindings", []):
			if binding.get("mesh_id", binding.get("target_id", "")) == selected_id or binding.id == selected_id:
				bindings.append(binding)
				var metadata: Dictionary = binding.duplicate()
				if metadata.get("keyforms") is Dictionary:
					metadata.keyforms = metadata.keyforms.get("items", [])
					metadata.label = "BlendShape · " + str(binding.get("target_kind", ""))
					for table in summary.get("blend_key_tables", []):
						if table.id == binding.key_table_id:
							for i in metadata.keyforms.size():
								metadata.keyforms[i] = metadata.keyforms[i].duplicate(true)
								metadata.keyforms[i].keys = [table.keys[i]]
				else:
					metadata.label = "Normal · " + str(binding.id)
				value.bindings.append(metadata)

		if value.get("binding") is Dictionary:
			var glue_binding: Dictionary = value.binding.duplicate(true)
			glue_binding.id = "Glue intensity"
			bindings.append(glue_binding)
			value.bindings.append(glue_binding)

		if not bindings.is_empty():
			binding_index = clampi(binding_index, 0, bindings.size() - 1)
			var forms: Array = value.bindings[binding_index].get("keyforms", [])
			if not forms.is_empty():
				keyform_index = clampi(keyform_index, 0, forms.size() - 1)
				value.selected_keyform = forms[keyform_index]

		value.resources = ClassDB.instantiate("KasaneProjectIO").diagnose_resources(workspace.document)

	inspector_dock.update_selection(value, binding_index, keyform_index)
	parameter_dock.update_selection_keyforms(selected_id)
	canvas.select(selected_id)

func _on_object_selected(id: String) -> void:
	selected_id = id
	show_selection()

func _on_jump_to_object(target_id: String) -> void:
	selected_id = target_id
	hierarchy_dock.select_id(selected_id)
	show_selection()

func perform(result: Dictionary) -> void:
	if result.has("executed"):
		var visible := {}
		for key in ["ok", "id", "code", "phase", "executed", "generation", "end_generation", "start_revision", "end_revision", "errors", "logs", "observation"]:
			if result.has(key):
				visible[key] = result[key]
		if result.has("id"):
			visible.result_file = agent.directory.path_join("results").path_join(result.id + ".json")
		report(JSON.stringify(visible))
	else:
		report(JSON.stringify(result))
	parameter_dock.update_values()

func report(text: String) -> void:
	if console_dock != null:
		console_dock.report(text)

func run_script(path: String) -> Dictionary:
	return agent.host.execute(path, workspace.document.get_document_summary().generation)

func choose_file(mode: int, filters: PackedStringArray, action: Callable) -> void:
	var dialog := FileDialog.new()
	dialog.use_native_dialog = true
	dialog.access = FileDialog.ACCESS_FILESYSTEM
	dialog.file_mode = mode
	dialog.filters = filters
	add_child(dialog)
	dialog.file_selected.connect(func(path: String):
		perform(action.call(path))
		dialog.queue_free())
	dialog.dir_selected.connect(func(path: String):
		perform(action.call(path))
		dialog.queue_free())
	dialog.canceled.connect(dialog.queue_free)
	dialog.popup_centered(Vector2i(900, 600))

func save_as() -> void:
	choose_file(FileDialog.FILE_MODE_SAVE_FILE, PackedStringArray(["*.json ; Kasane project"]), workspace.save_project)

func save_current() -> void:
	var path: String = workspace.document.get_document_summary().get("path", "")
	if path.is_empty():
		save_as()
	else:
		perform(workspace.save_project(path))

func replace_project(action: Callable) -> void:
	if not workspace.document.get_document_summary().get("modified", false):
		action.call()
		return
	var dialog := ConfirmationDialog.new()
	dialog.dialog_text = "当前工程有未保存修改。继续会丢弃这些修改。"
	add_child(dialog)
	dialog.confirmed.connect(func():
		action.call()
		dialog.queue_free())
	dialog.canceled.connect(dialog.queue_free)
	dialog.popup_centered()

func import_png_ui(path: String) -> Dictionary:
	var dialog := ConfirmationDialog.new()
	dialog.title = "导入 PNG — 裁剪位置（原画像素）"
	var fields := VBoxContainer.new()
	dialog.add_child(fields)
	var summary: Dictionary = workspace.document.get_document_summary()

	var desc := Label.new()
	desc.text = "原画尺寸使用当前工程画布；全画布图偏移为 0。"
	desc.add_theme_font_size_override("font_size", 12)
	fields.add_child(desc)

	var controls: Array[SpinBox] = []
	for axis in ["X", "Y"]:
		var lbl := Label.new()
		lbl.text = axis
		lbl.add_theme_font_size_override("font_size", 12)
		fields.add_child(lbl)
		var value := SpinBox.new()
		value.max_value = 100000
		fields.add_child(value)
		controls.append(value)
	add_child(dialog)
	dialog.confirmed.connect(func():
		var offset := Vector2(controls[0].value, controls[1].value)
		var result: Dictionary = workspace.import_png(path, summary.canvas_size, offset)
		if result.ok:
			result = workspace.create_rectangle(result.asset_id, offset)
			if result.ok:
				selected_id = result.mesh_id
				canvas.select(selected_id)
				canvas.fit_content()
		perform(result)
		dialog.queue_free())
	dialog.canceled.connect(dialog.queue_free)
	dialog.popup_centered(Vector2i(520, 240))
	return {"ok": true, "message": "选择裁剪偏移后导入"}
