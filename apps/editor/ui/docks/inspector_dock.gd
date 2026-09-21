extends PanelContainer

const EditorTheme = preload("res://ui/theme.gd")

signal jump_to_object_requested(target_id: String)
signal binding_selected(index: int)
signal keyform_selected(index: int)

var dock_info: Dictionary

# Sections
var empty_label: Label
var inspector_scroll: ScrollContainer
var inspector_content: VBoxContainer

# Identity Card
var name_val: Label
var id_val: Label
var kind_val: Label

# Hierarchy Card
var deform_parent_btn: Button
var org_parent_val: Label

# Render Properties Card
var render_card: PanelContainer
var blend_val: Label
var opacity_val: Label
var draw_order_val: Label
var masks_val: Label

# Bindings & Keyforms Card
var reference_links: VBoxContainer
var binding_choice: OptionButton
var keyform_choice: OptionButton

# Raw JSON Section
var raw_container: VBoxContainer
var raw_json_text: RichTextLabel
var raw_toggle_btn: Button

func _ready() -> void:
	dock_info = EditorTheme.make_dock("属性检查 (Inspector)")
	add_child(dock_info.panel)

	var content: VBoxContainer = dock_info.content

	empty_label = Label.new()
	empty_label.text = "未选择任何对象\n在左侧组织树或画布中点击选中"
	empty_label.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
	empty_label.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	empty_label.add_theme_color_override("font_color", EditorTheme.TEXT_DIM)
	empty_label.size_flags_vertical = Control.SIZE_EXPAND_FILL
	content.add_child(empty_label)

	inspector_scroll = ScrollContainer.new()
	inspector_scroll.size_flags_vertical = Control.SIZE_EXPAND_FILL
	inspector_scroll.horizontal_scroll_mode = ScrollContainer.SCROLL_MODE_DISABLED
	inspector_scroll.visible = false
	content.add_child(inspector_scroll)

	inspector_content = VBoxContainer.new()
	inspector_content.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	inspector_content.add_theme_constant_override("separation", 8)
	inspector_scroll.add_child(inspector_content)

	# --- Section 1: Basic Identity ---
	var id_card := _create_section_card("基础信息")
	inspector_content.add_child(id_card)
	var id_box: VBoxContainer = id_card.get_node("Content")

	name_val = _add_field_row(id_box, "名称")
	id_val = _add_field_row(id_box, "ID")
	kind_val = _add_field_row(id_box, "类型")

	# --- Section 2: Hierarchy & Deform Parent ---
	var hier_card := _create_section_card("层级关系")
	inspector_content.add_child(hier_card)
	var hier_box: VBoxContainer = hier_card.get_node("Content")

	org_parent_val = _add_field_row(hier_box, "组织所属")

	var deform_row := HBoxContainer.new()
	deform_row.add_theme_constant_override("separation", 6)
	hier_box.add_child(deform_row)
	var deform_label := Label.new()
	deform_label.text = "变形父"
	deform_label.custom_minimum_size.x = 60
	deform_label.add_theme_font_size_override("font_size", 11)
	deform_label.add_theme_color_override("font_color", EditorTheme.TEXT_MUTED)
	deform_row.add_child(deform_label)

	deform_parent_btn = Button.new()
	deform_parent_btn.text = "无"
	deform_parent_btn.clip_text = true
	deform_parent_btn.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	deform_parent_btn.disabled = true
	deform_parent_btn.pressed.connect(_on_deform_parent_pressed)
	deform_row.add_child(deform_parent_btn)

	reference_links = VBoxContainer.new()
	hier_box.add_child(reference_links)

	# --- Section 3: Render Properties ---
	render_card = _create_section_card("渲染属性")
	inspector_content.add_child(render_card)
	var render_box: VBoxContainer = render_card.get_node("Content")

	blend_val = _add_field_row(render_box, "混色模式")
	opacity_val = _add_field_row(render_box, "不透明度")
	draw_order_val = _add_field_row(render_box, "绘制顺序")
	masks_val = _add_field_row(render_box, "剪切蒙版")

	# --- Section 4: Bindings & Keyforms ---
	var binding_card := _create_section_card("绑定与 Keyform")
	inspector_content.add_child(binding_card)
	var binding_box: VBoxContainer = binding_card.get_node("Content")

	binding_choice = OptionButton.new()
	binding_choice.clip_text = true
	binding_choice.item_selected.connect(func(idx): binding_selected.emit(idx))
	binding_box.add_child(binding_choice)

	keyform_choice = OptionButton.new()
	keyform_choice.clip_text = true
	keyform_choice.item_selected.connect(func(idx): keyform_selected.emit(idx))
	binding_box.add_child(keyform_choice)

	# --- Section 5: Raw JSON (Collapsible) ---
	raw_container = VBoxContainer.new()
	raw_container.add_theme_constant_override("separation", 4)
	inspector_content.add_child(raw_container)

	raw_toggle_btn = Button.new()
	raw_toggle_btn.text = "▶ 原始数据 (JSON)"
	raw_toggle_btn.flat = true
	raw_toggle_btn.add_theme_font_size_override("font_size", 11)
	raw_toggle_btn.pressed.connect(_toggle_raw_json)
	raw_container.add_child(raw_toggle_btn)

	raw_json_text = RichTextLabel.new()
	raw_json_text.custom_minimum_size.y = 120
	raw_json_text.size_flags_vertical = Control.SIZE_EXPAND_FILL
	raw_json_text.visible = false
	raw_json_text.scroll_following = false
	raw_container.add_child(raw_json_text)

func _create_section_card(title: String) -> PanelContainer:
	var card := PanelContainer.new()
	var style := EditorTheme.make_flat_style(EditorTheme.BG_CARD, EditorTheme.BORDER_SUBTLE, 3, 6, 6)
	card.add_theme_stylebox_override("panel", style)

	var vbox := VBoxContainer.new()
	vbox.name = "Content"
	vbox.add_theme_constant_override("separation", 4)
	card.add_child(vbox)

	var title_lbl := Label.new()
	title_lbl.text = title
	title_lbl.add_theme_font_size_override("font_size", 11)
	title_lbl.add_theme_color_override("font_color", EditorTheme.accent)
	vbox.add_child(title_lbl)

	var sep := HSeparator.new()
	vbox.add_child(sep)

	return card

func _add_field_row(parent: VBoxContainer, label_text: String) -> Label:
	var row := HBoxContainer.new()
	row.add_theme_constant_override("separation", 6)
	parent.add_child(row)

	var lbl := Label.new()
	lbl.text = label_text
	lbl.custom_minimum_size.x = 60
	lbl.add_theme_font_size_override("font_size", 11)
	lbl.add_theme_color_override("font_color", EditorTheme.TEXT_MUTED)
	row.add_child(lbl)

	var val := Label.new()
	val.text = "-"
	val.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	val.clip_text = true
	val.add_theme_font_size_override("font_size", 11)
	val.add_theme_color_override("font_color", EditorTheme.TEXT_PRIMARY)
	row.add_child(val)

	return val

func _toggle_raw_json() -> void:
	raw_json_text.visible = not raw_json_text.visible
	raw_toggle_btn.text = "▼ 原始数据 (JSON)" if raw_json_text.visible else "▶ 原始数据 (JSON)"

func _on_deform_parent_pressed() -> void:
	var target: String = deform_parent_btn.get_meta("target", "")
	if not target.is_empty():
		jump_to_object_requested.emit(target)

func update_selection(data: Dictionary, binding_idx: int, keyform_idx: int) -> void:
	if data.is_empty():
		empty_label.visible = true
		inspector_scroll.visible = false
		return

	empty_label.visible = false
	inspector_scroll.visible = true

	# Identity
	name_val.text = str(data.get("name", data.get("id", "")))
	id_val.text = str(data.get("id", ""))
	id_val.tooltip_text = str(data.get("id", ""))
	kind_val.text = str(data.get("kind", "Mesh" if data.has("vertex_ids") else ("Part" if data.has("parent_id") and not data.has("points") else "Transform")))

	if data.has("pairs"): kind_val.text = "Glue · %d pairs · intensity %s" % [data.pairs.size(), str(data.intensity)]
	elif data.has("base_key_idx"): kind_val.text = "BlendShape table · base %d" % data.base_key_idx
	elif data.has("weights"): kind_val.text = "BlendShape constraint"
	elif data.has("target_kind"): kind_val.text = "BlendShape · " + str(data.target_kind)

	# Hierarchy
	org_parent_val.text = str(data.get("organization_parent", data.get("parent_id", "无")))
	var parent_id: String = data.get("deform_parent", data.get("parent_id", "") if data.has("kind") else "")
	if parent_id.is_empty():
		deform_parent_btn.text = "无"
		deform_parent_btn.disabled = true
		deform_parent_btn.set_meta("target", "")
	else:
		deform_parent_btn.text = parent_id
		deform_parent_btn.tooltip_text = "点击跳转至父变形器: " + parent_id
		deform_parent_btn.disabled = false
		deform_parent_btn.set_meta("target", parent_id)

	for child in reference_links.get_children():
		child.queue_free()
	for field in ["mesh_a_id", "mesh_b_id", "parameter_id", "target_id", "key_table_id"]:
		if data.has(field) and not str(data[field]).is_empty():
			var target := str(data[field])
			var link := Button.new()
			link.text = field + ": " + target
			link.clip_text = true
			link.tooltip_text = target
			link.pressed.connect(func(): jump_to_object_requested.emit(target))
			reference_links.add_child(link)
	for target in data.get("constraint_ids", []):
		var link := Button.new()
		link.text = "Constraint: " + str(target)
		link.clip_text = true
		link.pressed.connect(func(): jump_to_object_requested.emit(str(target)))
		reference_links.add_child(link)

	# Render Properties
	var props: Dictionary = data.get("properties", {})
	if not props.is_empty():
		render_card.visible = true
		var blend_modes := ["Normal (正常)", "Additive (叠加加色)", "Multiplicative (正片叠底)"]
		var bm_int: int = props.get("blend_mode", 0)
		blend_val.text = blend_modes[bm_int] if bm_int >= 0 and bm_int < blend_modes.size() else str(bm_int)
		var app: Dictionary = props.get("appearance", {})
		opacity_val.text = "%.2f" % app.get("opacity", 1.0)
		draw_order_val.text = str(props.get("draw_order", 0))
		var masks: Array = props.get("masks", [])
		masks_val.text = "%d 个" % masks.size() if not masks.is_empty() else "无"
	else:
		render_card.visible = false

	# Bindings & Keyforms
	binding_choice.clear()
	keyform_choice.clear()

	var bindings: Array = data.get("bindings", [])
	for b in bindings:
		binding_choice.add_item(str(b.get("label", b.get("id", ""))))

	if not bindings.is_empty():
		var b_idx := clampi(binding_idx, 0, bindings.size() - 1)
		binding_choice.select(b_idx)
		var forms: Array = bindings[b_idx].get("keyforms", [])
		for f in forms:
			keyform_choice.add_item(str(f.get("keys", [])))
		if not forms.is_empty():
			var k_idx := clampi(keyform_idx, 0, forms.size() - 1)
			keyform_choice.select(k_idx)

	binding_choice.disabled = binding_choice.item_count == 0
	keyform_choice.disabled = keyform_choice.item_count == 0
	if binding_choice.item_count == 0:
		binding_choice.add_item("无绑定")
	if keyform_choice.item_count == 0:
		keyform_choice.add_item("无 Keyform")

	# Raw JSON
	raw_json_text.text = JSON.stringify(data, "  ")
