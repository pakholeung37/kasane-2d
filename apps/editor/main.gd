extends Control

const CanvasSurface = preload("res://canvas.gd")
var canvas: Control
var status: Label
var output: RichTextLabel
var workspace = preload("res://workspace.gd").new()
var object_tree: Tree
var inspector: RichTextLabel
var selected_id := ""
var parameter_box: VBoxContainer
var parameter_controls := {}
var agent: Node
var deform_parent: Button
var binding_choice: OptionButton
var keyform_choice: OptionButton
var inspected_object := ""
var binding_index := 0
var keyform_index := 0

func update_parameters() -> void:
	var frame: Dictionary = workspace.document.get_frame()
	if frame.ok:
		for sample in frame.parameters:
			if parameter_controls.has(sample.id):
				parameter_controls[sample.id].set_value_no_signal(sample.value)

func rebuild_parameters(summary: Dictionary) -> void:
	for child in parameter_box.get_children():
		parameter_box.remove_child(child)
		child.queue_free()
	parameter_controls.clear()
	for parameter in summary.parameters:
		var row := VBoxContainer.new()
		parameter_box.add_child(row)
		row.add_child(label_for(parameter.name, 12))
		var value := SpinBox.new()
		value.min_value = parameter.minimum
		value.max_value = parameter.maximum
		value.step = pow(10, -parameter.decimal_places)
		value.value = parameter.default_value
		row.add_child(value)
		parameter_controls[parameter.id] = value
		value.value_changed.connect(func(number: float):
			var values := {}
			var frame: Dictionary = workspace.document.get_frame()
			for sample in frame.get("parameters", []):
				values[sample.id] = sample.value
			values[parameter.id] = number
			var result: Dictionary = workspace.document.set_preview_values(values)
			if not result.ok:
				report(JSON.stringify(result)))
	update_parameters()

func import_png_ui(path: String) -> Dictionary:
	var dialog := ConfirmationDialog.new()
	dialog.title = "导入 PNG — 裁剪位置（原画像素）"
	var fields := VBoxContainer.new()
	dialog.add_child(fields)
	var summary: Dictionary = workspace.document.get_document_summary()
	fields.add_child(label_for("原画尺寸使用当前工程画布；全画布图偏移为 0。", 13))
	var controls: Array[SpinBox] = []
	for axis in ["X", "Y"]:
		fields.add_child(label_for(axis))
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

func run_script(path: String) -> Dictionary:
	return agent.host.execute(path, workspace.document.get_document_summary().generation)

func _exit_tree() -> void:
	if canvas != null and canvas.selection != null:
		canvas.selection.set_preview(null)


func refresh_document(_change: Dictionary = {}) -> void:
	var summary: Dictionary = workspace.document.get_document_summary()
	status.text = "工程：%s%s | generation %s · revision %s" % [summary.path if not summary.path.is_empty() else "未保存", " *" if summary.modified else "", summary.generation, summary.revision]
	object_tree.clear()
	var root := object_tree.create_item()
	root.set_text(0, summary.id if summary.initialized else "尚无 Document")
	var items := {"": root}
	var pending: Array = summary.parts.duplicate()
	while not pending.is_empty():
		var progress := false
		for part in pending.duplicate():
			if items.has(part.parent_id):
				var item := object_tree.create_item(items[part.parent_id])
				item.set_text(0, "Part · " + part.name)
				item.set_metadata(0, part.id)
				items[part.id] = item
				pending.erase(part)
				progress = true
		if not progress:
			break
	for group in [summary.meshes, summary.deformers]:
		for entry in group:
			var snapshot: Dictionary = workspace.document.get_mesh_snapshot(entry.id) if group == summary.meshes else workspace.document.get_deformer_snapshot(entry.id)
			var item := object_tree.create_item(items.get(snapshot.organization_parent, root))
			item.set_text(0, snapshot.get("kind", "Mesh") + " · " + entry.name)
			item.set_metadata(0, entry.id)
	rebuild_parameters(summary)
	show_selection()

func show_selection() -> void:
	var summary: Dictionary = workspace.document.get_document_summary()
	var value: Dictionary = {}
	for group in [summary.parts, summary.meshes, summary.transforms]:
		for entry in group:
			if entry.id == selected_id:
				value = entry
				if group == summary.meshes:
					value = workspace.document.get_mesh_snapshot(selected_id)
	if inspected_object != selected_id:
		binding_index = 0
		keyform_index = 0
		inspected_object = selected_id
	binding_choice.clear()
	keyform_choice.clear()
	if not value.is_empty():
		value = value.duplicate(true)
		var bindings: Array = []
		value.bindings = []
		for binding in summary.bindings + summary.scene_bindings:
			if binding.get("mesh_id", binding.get("target_id", "")) == selected_id:
				bindings.append(binding)
				binding_choice.add_item(binding.id)
				var metadata: Dictionary = binding.duplicate()
				metadata.erase("keyforms")
				value.bindings.append(metadata)
		if not bindings.is_empty():
			binding_index = clampi(binding_index, 0, bindings.size()-1)
			binding_choice.select(binding_index)
			var forms: Array = bindings[binding_index].keyforms
			for form in forms:
				keyform_choice.add_item(str(form.keys))
			if not forms.is_empty():
				keyform_index = clampi(keyform_index, 0, forms.size()-1)
				keyform_choice.select(keyform_index)
				value.selected_keyform = forms[keyform_index]
		value.resources = ClassDB.instantiate("KasaneProjectIO").diagnose_resources(workspace.document)
	binding_choice.disabled = binding_choice.item_count == 0
	keyform_choice.disabled = keyform_choice.item_count == 0
	if binding_choice.item_count == 0:
		binding_choice.add_item("无绑定")
	if keyform_choice.item_count == 0:
		keyform_choice.add_item("无 Keyform")
	inspector.text = JSON.stringify(value, "  ") if not value.is_empty() else "未选择对象"
	var parent_id: String = value.get("deform_parent", value.get("parent_id", "") if value.has("kind") else "")
	deform_parent.text = "变形父：" + (parent_id if not parent_id.is_empty() else "无")
	deform_parent.clip_text = true
	deform_parent.tooltip_text = parent_id
	deform_parent.set_meta("target", parent_id)
	deform_parent.disabled = parent_id.is_empty()
	canvas.select(selected_id)

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
	# Document changes already refreshed the tree; preview-only scripts need no rebuild.
	update_parameters()

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
	var path: String = workspace.document.get_document_summary().path
	if path.is_empty():
		save_as()
	else:
		perform(workspace.save_project(path))

func replace_project(action: Callable) -> void:
	if not workspace.document.get_document_summary().modified:
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


func label_for(text: String, font_size: int = 14) -> Label:
	var label := Label.new()
	label.text = text
	label.add_theme_font_size_override("font_size", font_size)
	return label

func button_for(parent: Control, text: String, action: Callable, unavailable: String = "") -> Button:
	var button := Button.new()
	button.text = text
	button.disabled = not unavailable.is_empty()
	button.tooltip_text = unavailable
	button.pressed.connect(action)
	parent.add_child(button)
	return button

func panel(parent: Control, title: String, width: float = 240) -> VBoxContainer:
	var box := VBoxContainer.new()
	box.custom_minimum_size.x = width
	box.add_theme_constant_override("separation", 12)
	parent.add_child(box)
	box.add_child(label_for(title, 18))
	box.add_child(HSeparator.new())
	return box

func empty_text(parent: Control, text: String) -> void:
	var label := label_for(text)
	label.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	label.modulate = Color("9caac0")
	parent.add_child(label)

func report(text: String) -> void:
	output.append_text(text.left(16000) + "\n")
	if output.get_total_character_count() > 64000:
		output.text = output.text.right(48000)

func _ready() -> void:
	if not workspace.startup_error.is_empty():
		var message := Label.new()
		message.text = workspace.startup_error
		message.position = Vector2(24, 24)
		add_child(message)
		push_error(workspace.startup_error)
		return
	var skin := Theme.new()
	skin.default_font_size = 14
	var style := StyleBoxFlat.new()
	style.bg_color = Color("253149")
	style.set_corner_radius_all(5)
	style.content_margin_left = 12
	style.content_margin_right = 12
	style.content_margin_top = 8
	style.content_margin_bottom = 8
	skin.set_stylebox("normal", "Button", style)
	theme = skin
	var margin := MarginContainer.new()
	margin.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	for side in ["left", "right", "top", "bottom"]:
		margin.add_theme_constant_override("margin_" + side, 16)
	add_child(margin)
	var layout := VBoxContainer.new()
	layout.add_theme_constant_override("separation", 12)
	margin.add_child(layout)
	var header := HBoxContainer.new()
	layout.add_child(header)
	header.add_child(label_for("KASANE  /  Editor", 24))
	var spacer := Control.new()
	spacer.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	header.add_child(spacer)
	header.add_child(label_for("Agent-first 模型编辑", 13))
	var toolbar := HBoxContainer.new()
	layout.add_child(toolbar)
	button_for(toolbar, "新建", func(): replace_project(func(): perform(workspace.new_project())))
	button_for(toolbar, "打开", func(): replace_project(func(): choose_file(FileDialog.FILE_MODE_OPEN_FILE, PackedStringArray(["*.json ; Kasane project"]), workspace.open_project)))
	button_for(toolbar, "保存", save_current)
	button_for(toolbar, "另存为", save_as)
	button_for(toolbar, "导入 PNG", func(): choose_file(FileDialog.FILE_MODE_OPEN_FILE, PackedStringArray(["*.png ; PNG image"]), import_png_ui))
	button_for(toolbar, "导入 model3", func(): replace_project(func(): choose_file(FileDialog.FILE_MODE_OPEN_FILE, PackedStringArray(["*.model3.json ; Cubism model"]), workspace.import_model)))
	button_for(toolbar, "导出 MOC3 包", func(): choose_file(FileDialog.FILE_MODE_OPEN_DIR, PackedStringArray(), workspace.export_model))
	var body := HSplitContainer.new()
	body.size_flags_vertical = Control.SIZE_EXPAND_FILL
	layout.add_child(body)
	var left := panel(body, "对象与组织", 230)
	var tree := Tree.new()
	object_tree = tree
	tree.item_selected.connect(func():
		selected_id = str(tree.get_selected().get_metadata(0))
		show_selection())
	tree.size_flags_vertical = Control.SIZE_EXPAND_FILL
	tree.custom_minimum_size.y = 160
	left.add_child(tree)
	tree.create_item().set_text(0, "尚无 Document")
	empty_text(left, "Part / ArtMesh / Deformer\n\n组织树与变形父关系将分别显示。")
	var right_split := HSplitContainer.new()
	right_split.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	body.add_child(right_split)
	var center := VBoxContainer.new()
	center.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	right_split.add_child(center)
	var navigation := HBoxContainer.new()
	center.add_child(navigation)
	button_for(navigation, "重置视图", func(): canvas.reset_view())
	button_for(navigation, "适配内容", func(): canvas.fit_content())
	button_for(navigation, "定位选中", func(): canvas.fit_content(selected_id))
	navigation.add_child(label_for("滚轮缩放 · 中键平移", 12))
	canvas = CanvasSurface.new()
	canvas.custom_minimum_size = Vector2(320, 220)
	canvas.size_flags_vertical = Control.SIZE_EXPAND_FILL
	center.add_child(canvas)
	var right := panel(right_split, "属性检查", 260)
	binding_choice = OptionButton.new()
	binding_choice.clip_text = true
	binding_choice.item_selected.connect(func(index: int):
		binding_index = index
		keyform_index = 0
		show_selection())
	right.add_child(binding_choice)
	keyform_choice = OptionButton.new()
	keyform_choice.item_selected.connect(func(index: int):
		keyform_index = index
		show_selection())
	right.add_child(keyform_choice)
	inspector = RichTextLabel.new()
	inspector.custom_minimum_size.y = 170
	inspector.size_flags_vertical = Control.SIZE_EXPAND_FILL
	right.add_child(inspector)
	deform_parent = button_for(right, "变形父：无", func():
		selected_id = str(deform_parent.get_meta("target", ""))
		show_selection())
	right.add_child(HSeparator.new())
	right.add_child(label_for("参数预览", 18))
	var scroll := ScrollContainer.new()
	scroll.custom_minimum_size.y = 110
	right.add_child(scroll)
	parameter_box = VBoxContainer.new()
	parameter_box.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	scroll.add_child(parameter_box)
	button_for(right, "恢复参数默认值", func(): workspace.document.set_preview_values({}))
	var tabs := TabContainer.new()
	tabs.custom_minimum_size.y = 125
	layout.add_child(tabs)
	output = RichTextLabel.new()
	output.name = "运行反馈"
	output.scroll_following = true
	tabs.add_child(output)
	var script_panel := VBoxContainer.new()
	script_panel.name = "Agent 脚本"
	tabs.add_child(script_panel)
	empty_text(script_panel, "脚本定义 run(workspace)，同步返回含 ok 的 Dictionary。执行不自动回滚。")
	button_for(script_panel, "执行脚本文件", func(): choose_file(FileDialog.FILE_MODE_OPEN_FILE, PackedStringArray(["*.gd ; GDScript"]), run_script))
	button_for(script_panel, "撤销 Action", func(): workspace.undo_redo.undo())
	button_for(script_panel, "重做 Action", func(): workspace.undo_redo.redo())
	status = label_for("工程：未打开  |  Document：未连接", 12)
	status.clip_text = true
	layout.add_child(status)
	canvas.attach(workspace)
	workspace.document.changed.connect(refresh_document)
	workspace.document.preview_changed.connect(update_parameters)
	agent = preload("res://agent_queue.gd").new()
	add_child(agent)
	var directory := "user://agent/" + str(OS.get_process_id())
	for argument in OS.get_cmdline_user_args():
		if argument.begins_with("--agent-dir="):
			directory = argument.trim_prefix("--agent-dir=")
	agent.completed.connect(perform)
	var queue_result: Dictionary = agent.start(workspace, canvas, directory)
	empty_text(script_panel, "Agent 目录：" + agent.directory)
	report(JSON.stringify(queue_result))
	refresh_document()
	report("已就绪。Agent 请求目录见脚本面板。")
