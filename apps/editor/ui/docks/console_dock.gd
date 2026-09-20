extends PanelContainer

const EditorTheme = preload("res://ui/theme.gd")

signal run_script_requested()
signal undo_requested()
signal redo_requested()

var dock_info: Dictionary
var tabs: TabContainer
var output_text: RichTextLabel
var agent_dir_label: Label

func _ready() -> void:
	dock_info = EditorTheme.make_dock("控制台与 Agent (Console)")
	add_child(dock_info.panel)

	# Add "Clear" button to header
	var header_box: HBoxContainer = dock_info.header_box
	var clear_btn := Button.new()
	clear_btn.text = "清空"
	clear_btn.tooltip_text = "清空控制台日志输出"
	clear_btn.flat = true
	clear_btn.add_theme_font_size_override("font_size", 10)
	clear_btn.pressed.connect(clear_output)
	header_box.add_child(clear_btn)
	header_box.move_child(clear_btn, header_box.get_child_count() - 2)

	var content: VBoxContainer = dock_info.content

	tabs = TabContainer.new()
	tabs.size_flags_vertical = Control.SIZE_EXPAND_FILL
	tabs.custom_minimum_size.y = 80
	content.add_child(tabs)

	# Tab 1: Output
	output_text = RichTextLabel.new()
	output_text.name = "运行反馈"
	output_text.scroll_following = true
	output_text.size_flags_vertical = Control.SIZE_EXPAND_FILL
	output_text.add_theme_font_size_override("normal_font_size", 11)
	tabs.add_child(output_text)

	# Tab 2: Agent Scripts & Actions
	var agent_panel := VBoxContainer.new()
	agent_panel.name = "Agent 自动化"
	agent_panel.add_theme_constant_override("separation", 6)
	tabs.add_child(agent_panel)

	agent_dir_label = Label.new()
	agent_dir_label.text = "Agent 目录：未连接"
	agent_dir_label.add_theme_font_size_override("font_size", 11)
	agent_dir_label.add_theme_color_override("font_color", EditorTheme.TEXT_MUTED)
	agent_panel.add_child(agent_dir_label)

	var btn_row := HBoxContainer.new()
	btn_row.add_theme_constant_override("separation", 8)
	agent_panel.add_child(btn_row)

	var run_btn := Button.new()
	run_btn.text = "执行脚本文件"
	run_btn.tooltip_text = "手动选择并执行 .gd 外部脚本"
	run_btn.pressed.connect(func(): run_script_requested.emit())
	btn_row.add_child(run_btn)

	var undo_btn := Button.new()
	undo_btn.text = "撤销 Action"
	undo_btn.tooltip_text = "撤销最近一次 Document 事务/操作"
	undo_btn.pressed.connect(func(): undo_requested.emit())
	btn_row.add_child(undo_btn)

	var redo_btn := Button.new()
	redo_btn.text = "重做 Action"
	redo_btn.tooltip_text = "重做已撤销的 Document 事务/操作"
	redo_btn.pressed.connect(func(): redo_requested.emit())
	btn_row.add_child(redo_btn)

	var note := Label.new()
	note.text = "脚本必须实现 run(w) -> Dictionary，同步返回结构化结果。执行不自动回滚。"
	note.add_theme_font_size_override("font_size", 11)
	note.add_theme_color_override("font_color", EditorTheme.TEXT_DIM)
	agent_panel.add_child(note)

func report(text: String) -> void:
	if output_text == null:
		return
	output_text.append_text(text.left(16000) + "\n")
	if output_text.get_total_character_count() > 64000:
		output_text.text = output_text.text.right(48000)

func clear_output() -> void:
	if output_text != null:
		output_text.clear()

func set_agent_directory(dir: String) -> void:
	if agent_dir_label != null:
		agent_dir_label.text = "Agent IPC 目录：" + dir
