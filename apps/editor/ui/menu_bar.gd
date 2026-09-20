extends PanelContainer

const EditorTheme = preload("res://ui/theme.gd")
const EditorLayoutManager = preload("res://ui/layout_manager.gd")

signal new_project_requested()
signal open_project_requested()
signal save_project_requested()
signal save_as_requested()
signal import_png_requested()
signal import_model_requested()
signal export_model_requested()
signal ui_scale_selected(factor: float)

var status_label: Label
var badge_container: PanelContainer
var scale_choice: OptionButton

func _ready() -> void:
	var style := EditorTheme.make_flat_style(EditorTheme.BG_CARD, EditorTheme.BORDER, 0, 8, 4)
	style.border_width_bottom = 1
	add_theme_stylebox_override("panel", style)

	var hbox := HBoxContainer.new()
	hbox.add_theme_constant_override("separation", 8)
	add_child(hbox)

	# Brand & App title
	var title_box := HBoxContainer.new()
	title_box.add_theme_constant_override("separation", 6)
	hbox.add_child(title_box)

	var title := Label.new()
	title.text = "KASANE"
	title.add_theme_font_size_override("font_size", 13)
	title.add_theme_color_override("font_color", EditorTheme.accent)
	title_box.add_child(title)

	var subtitle := Label.new()
	subtitle.text = "Editor"
	subtitle.add_theme_font_size_override("font_size", 13)
	subtitle.add_theme_color_override("font_color", EditorTheme.TEXT_PRIMARY)
	title_box.add_child(subtitle)

	var sep1 := VSeparator.new()
	hbox.add_child(sep1)

	# Project Operations
	_add_action_button(hbox, "新建", func(): new_project_requested.emit(), "创建新工程")
	_add_action_button(hbox, "打开", func(): open_project_requested.emit(), "打开现有 Kasane 工程 (.json)")
	_add_action_button(hbox, "保存", func(): save_project_requested.emit(), "保存当前工程")
	_add_action_button(hbox, "另存为", func(): save_as_requested.emit(), "将工程另存为新文件")

	var sep2 := VSeparator.new()
	hbox.add_child(sep2)

	# Import / Export
	_add_action_button(hbox, "导入 PNG", func(): import_png_requested.emit(), "导入 PNG 原画素材")
	_add_action_button(hbox, "导入 model3", func(): import_model_requested.emit(), "导入 Cubism model3 模型")
	_add_action_button(hbox, "导出 MOC3", func(): export_model_requested.emit(), "导出可用于运行时的 MOC3 包")

	var spacer := Control.new()
	spacer.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	hbox.add_child(spacer)

	# UI Scale Selector
	var scale_box := HBoxContainer.new()
	scale_box.add_theme_constant_override("separation", 4)
	hbox.add_child(scale_box)

	var scale_lbl := Label.new()
	scale_lbl.text = "界面缩放"
	scale_lbl.add_theme_font_size_override("font_size", 11)
	scale_lbl.add_theme_color_override("font_color", EditorTheme.TEXT_MUTED)
	scale_box.add_child(scale_lbl)

	scale_choice = OptionButton.new()
	scale_choice.custom_minimum_size = Vector2(100, 22)
	scale_choice.add_theme_font_size_override("font_size", 11)
	_setup_scale_options()
	scale_choice.item_selected.connect(_on_scale_selected)
	scale_box.add_child(scale_choice)

	var sep3 := VSeparator.new()
	hbox.add_child(sep3)

	# Status Badge
	badge_container = PanelContainer.new()
	var badge_style := EditorTheme.make_flat_style(EditorTheme.BG_INPUT, EditorTheme.BORDER_SUBTLE, 10, 8, 2)
	badge_container.add_theme_stylebox_override("panel", badge_style)
	hbox.add_child(badge_container)

	status_label = Label.new()
	status_label.add_theme_font_size_override("font_size", 11)
	status_label.add_theme_color_override("font_color", EditorTheme.TEXT_MUTED)
	status_label.text = "未打开工程"
	badge_container.add_child(status_label)

func _setup_scale_options() -> void:
	scale_choice.clear()
	var rec := int(round(EditorLayoutManager.get_recommended_scale() * 100.0))
	scale_choice.add_item("自动 (%d%%)" % rec)
	scale_choice.add_item("100% (标准)")
	scale_choice.add_item("125%")
	scale_choice.add_item("150% (Mac推荐)")
	scale_choice.add_item("175%")
	scale_choice.add_item("200% (4K)")

func _on_scale_selected(idx: int) -> void:
	var factors := [-1.0, 1.0, 1.25, 1.5, 1.75, 2.0]
	if idx >= 0 and idx < factors.size():
		ui_scale_selected.emit(factors[idx])

func set_scale_display(factor: float) -> void:
	if scale_choice == null:
		return
	if factor <= 0.0:
		scale_choice.select(0)
	elif is_equal_approx(factor, 1.0):
		scale_choice.select(1)
	elif is_equal_approx(factor, 1.25):
		scale_choice.select(2)
	elif is_equal_approx(factor, 1.5):
		scale_choice.select(3)
	elif is_equal_approx(factor, 1.75):
		scale_choice.select(4)
	elif is_equal_approx(factor, 2.0):
		scale_choice.select(5)
	else:
		scale_choice.select(0)

func _add_action_button(parent: Control, label_text: String, action: Callable, tip: String = "") -> Button:
	var btn := Button.new()
	btn.text = label_text
	btn.tooltip_text = tip
	btn.pressed.connect(action)
	parent.add_child(btn)
	return btn

func update_status(path: String, modified: bool, generation: int, revision: int) -> void:
	var filename := path.get_file() if not path.is_empty() else "未保存工程"
	var mod_marker := " *" if modified else ""
	status_label.text = "%s%s  |  gen: %d · rev: %d" % [filename, mod_marker, generation, revision]
	if modified:
		status_label.add_theme_color_override("font_color", EditorTheme.accent_hover)
	else:
		status_label.add_theme_color_override("font_color", EditorTheme.TEXT_MUTED)
