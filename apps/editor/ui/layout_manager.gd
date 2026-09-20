extends RefCounted
class_name EditorLayoutManager

const CONFIG_PATH := "user://editor_layout.cfg"

var main_hsplit: HSplitContainer
var left_vsplit: VSplitContainer
var center_right_hsplit: HSplitContainer
var center_vsplit: VSplitContainer

func setup(
	p_main_hsplit: HSplitContainer,
	p_left_vsplit: VSplitContainer,
	p_center_right_hsplit: HSplitContainer,
	p_center_vsplit: VSplitContainer
) -> void:
	main_hsplit = p_main_hsplit
	left_vsplit = p_left_vsplit
	center_right_hsplit = p_center_right_hsplit
	center_vsplit = p_center_vsplit

	load_layout()

	# Connect dragged signals to persist on user resize
	if main_hsplit != null:
		main_hsplit.dragged.connect(func(_offset): save_layout())
	if left_vsplit != null:
		left_vsplit.dragged.connect(func(_offset): save_layout())
	if center_right_hsplit != null:
		center_right_hsplit.dragged.connect(func(_offset): save_layout())
	if center_vsplit != null:
		center_vsplit.dragged.connect(func(_offset): save_layout())

var current_ui_scale: float = -1.0

func save_layout() -> void:
	var config := ConfigFile.new()
	var _err := config.load(CONFIG_PATH)
	if main_hsplit != null:
		config.set_value("splits", "main_hsplit", main_hsplit.split_offset)
	if left_vsplit != null:
		config.set_value("splits", "left_vsplit", left_vsplit.split_offset)
	if center_right_hsplit != null:
		config.set_value("splits", "center_right_hsplit", center_right_hsplit.split_offset)
	if center_vsplit != null:
		config.set_value("splits", "center_vsplit", center_vsplit.split_offset)
	config.set_value("appearance", "ui_scale", current_ui_scale)
	config.save(CONFIG_PATH)

func load_layout() -> void:
	var config := ConfigFile.new()
	if config.load(CONFIG_PATH) != OK:
		# Set sensible defaults for Cubism-style layout
		if main_hsplit != null:
			main_hsplit.split_offset = 260
		if left_vsplit != null:
			left_vsplit.split_offset = 320
		if center_right_hsplit != null:
			center_right_hsplit.split_offset = -300
		if center_vsplit != null:
			center_vsplit.split_offset = -170
		current_ui_scale = -1.0
		return

	if main_hsplit != null:
		main_hsplit.split_offset = config.get_value("splits", "main_hsplit", 260)
	if left_vsplit != null:
		left_vsplit.split_offset = config.get_value("splits", "left_vsplit", 320)
	if center_right_hsplit != null:
		center_right_hsplit.split_offset = config.get_value("splits", "center_right_hsplit", -300)
	if center_vsplit != null:
		center_vsplit.split_offset = config.get_value("splits", "center_vsplit", -170)
	current_ui_scale = config.get_value("appearance", "ui_scale", -1.0)

static func get_recommended_scale() -> float:
	if DisplayServer.get_name() == "headless":
		return 1.0
	if DisplayServer.get_screen_count() == 0:
		return 1.0
	var screen := DisplayServer.SCREEN_OF_MAIN_WINDOW
	var s: float = DisplayServer.screen_get_scale(screen)
	var dpi: int = DisplayServer.screen_get_dpi(screen)
	if s >= 1.8 or dpi >= 200:
		return 1.5
	elif s >= 1.3 or dpi >= 140:
		return 1.25
	var size: Vector2i = DisplayServer.screen_get_size(screen)
	if size.y >= 2160:
		return 1.75
	elif size.y >= 1440:
		return 1.25
	return 1.0

func apply_ui_scale(window: Window, factor: float) -> void:
	current_ui_scale = factor
	if window == null:
		return
	var actual := factor
	if actual <= 0.0:
		actual = get_recommended_scale()
	window.content_scale_factor = actual

func save_ui_scale(factor: float) -> void:
	current_ui_scale = factor
	save_layout()

func get_saved_ui_scale() -> float:
	return current_ui_scale

