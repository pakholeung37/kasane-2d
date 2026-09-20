extends RefCounted
class_name EditorTheme

# --- Palette: Neutral Dark Pro ---
const BG_BASE := Color("141416")
const BG_PANEL := Color("1c1c1f")
const BG_CARD := Color("232328")
const BG_INPUT := Color("2a2a30")
const BG_HOVER := Color("33333b")
const BG_ACTIVE := Color("3c3c46")

const BORDER := Color("383842")
const BORDER_FOCUS := Color("52525e")
const BORDER_SUBTLE := Color("2c2c34")

const TEXT_PRIMARY := Color("f4f4f5")
const TEXT_MUTED := Color("a1a1aa")
const TEXT_DIM := Color("71717a")

# Switchable Accent Color (Default: Live2D Emerald Green)
static var accent := Color("10b981")
static var accent_hover := Color("34d399")
static var accent_dim := Color("065f46")
static var accent_bg := Color("043628")

static func set_accent_color(new_accent: Color) -> void:
	accent = new_accent
	accent_hover = new_accent.lightened(0.2)
	accent_dim = new_accent.darkened(0.3)
	accent_bg = new_accent.darkened(0.7)

# --- StyleBox Factory Helpers ---
static func make_flat_style(bg: Color, border: Color = Color.TRANSPARENT, corner: int = 3, m_h: int = 6, m_v: int = 4) -> StyleBoxFlat:
	var style := StyleBoxFlat.new()
	style.bg_color = bg
	style.set_corner_radius_all(corner)
	style.content_margin_left = m_h
	style.content_margin_right = m_h
	style.content_margin_top = m_v
	style.content_margin_bottom = m_v
	if border != Color.TRANSPARENT:
		style.border_width_left = 1
		style.border_width_right = 1
		style.border_width_top = 1
		style.border_width_bottom = 1
		style.border_color = border
	return style

static func make_dock_header_style() -> StyleBoxFlat:
	var style := StyleBoxFlat.new()
	style.bg_color = BG_CARD
	style.set_corner_radius_all(3)
	style.content_margin_left = 8
	style.content_margin_right = 8
	style.content_margin_top = 4
	style.content_margin_bottom = 4
	style.border_width_bottom = 1
	style.border_color = BORDER
	return style

# --- Build Full High-Density Godot Theme ---
static func create_theme() -> Theme:
	var t := Theme.new()
	t.default_font_size = 12

	# Button
	var btn_normal := make_flat_style(BG_INPUT, BORDER, 3, 8, 4)
	var btn_hover := make_flat_style(BG_HOVER, BORDER_FOCUS, 3, 8, 4)
	var btn_pressed := make_flat_style(BG_ACTIVE, accent, 3, 8, 4)
	var btn_disabled := make_flat_style(BG_PANEL, BORDER_SUBTLE, 3, 8, 4)
	var btn_focus := make_flat_style(BG_INPUT, accent, 3, 8, 4)

	t.set_stylebox("normal", "Button", btn_normal)
	t.set_stylebox("hover", "Button", btn_hover)
	t.set_stylebox("pressed", "Button", btn_pressed)
	t.set_stylebox("disabled", "Button", btn_disabled)
	t.set_stylebox("focus", "Button", btn_focus)
	t.set_color("font_color", "Button", TEXT_PRIMARY)
	t.set_color("font_hover_color", "Button", Color.WHITE)
	t.set_color("font_pressed_color", "Button", Color.WHITE)
	t.set_color("font_disabled_color", "Button", TEXT_DIM)

	# LineEdit / SpinBox
	var input_normal := make_flat_style(BG_BASE, BORDER, 3, 6, 3)
	var input_focus := make_flat_style(BG_BASE, accent, 3, 6, 3)
	t.set_stylebox("normal", "LineEdit", input_normal)
	t.set_stylebox("focus", "LineEdit", input_focus)
	t.set_color("font_color", "LineEdit", TEXT_PRIMARY)
	t.set_color("font_placeholder_color", "LineEdit", TEXT_DIM)

	# Tree
	var tree_bg := make_flat_style(BG_BASE, BORDER_SUBTLE, 3, 4, 4)
	t.set_stylebox("panel", "Tree", tree_bg)
	t.set_color("font_color", "Tree", TEXT_PRIMARY)
	t.set_color("font_selected_color", "Tree", Color.WHITE)

	# TabContainer
	var tab_bar := make_flat_style(BG_PANEL, Color.TRANSPARENT, 0, 8, 4)
	var tab_selected := make_flat_style(BG_CARD, BORDER, 3, 10, 5)
	tab_selected.border_width_bottom = 0
	var tab_unselected := make_flat_style(BG_PANEL, Color.TRANSPARENT, 3, 10, 5)
	t.set_stylebox("tab_selected", "TabContainer", tab_selected)
	t.set_stylebox("tab_unselected", "TabContainer", tab_unselected)
	t.set_stylebox("panel", "TabContainer", make_flat_style(BG_CARD, BORDER, 3, 8, 6))

	# OptionButton
	t.set_stylebox("normal", "OptionButton", btn_normal)
	t.set_stylebox("hover", "OptionButton", btn_hover)
	t.set_stylebox("pressed", "OptionButton", btn_pressed)

	# PopupMenu
	var popup_bg := make_flat_style(BG_CARD, BORDER_FOCUS, 4, 6, 6)
	t.set_stylebox("panel", "PopupMenu", popup_bg)
	t.set_color("font_color", "PopupMenu", TEXT_PRIMARY)

	# Panel / PanelContainer
	t.set_stylebox("panel", "PanelContainer", make_flat_style(BG_PANEL, BORDER, 3, 4, 4))

	# ScrollContainer
	var scroll_bg := StyleBoxEmpty.new()
	t.set_stylebox("panel", "ScrollContainer", scroll_bg)

	# Labels
	t.set_color("font_color", "Label", TEXT_PRIMARY)

	return t

# --- Helper to create styled Dock Panel Container ---
static func make_dock(title_text: String, collapsible: bool = true) -> Dictionary:
	var panel := PanelContainer.new()
	var panel_style := make_flat_style(BG_PANEL, BORDER, 3, 0, 0)
	panel.add_theme_stylebox_override("panel", panel_style)

	var layout := VBoxContainer.new()
	layout.add_theme_constant_override("separation", 0)
	panel.add_child(layout)

	var header := PanelContainer.new()
	header.add_theme_stylebox_override("panel", make_dock_header_style())
	layout.add_child(header)

	var header_box := HBoxContainer.new()
	header_box.add_theme_constant_override("separation", 6)
	header.add_child(header_box)

	var title := Label.new()
	title.text = title_text
	title.add_theme_font_size_override("font_size", 12)
	title.add_theme_color_override("font_color", TEXT_PRIMARY)
	header_box.add_child(title)

	var spacer := Control.new()
	spacer.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	header_box.add_child(spacer)

	var toggle_btn: Button = null
	if collapsible:
		toggle_btn = Button.new()
		toggle_btn.text = "▼"
		toggle_btn.flat = true
		toggle_btn.custom_minimum_size = Vector2(20, 20)
		toggle_btn.add_theme_font_size_override("font_size", 10)
		header_box.add_child(toggle_btn)

	var content := VBoxContainer.new()
	content.size_flags_vertical = Control.SIZE_EXPAND_FILL
	content.add_theme_constant_override("separation", 6)
	var content_margin := MarginContainer.new()
	content_margin.size_flags_vertical = Control.SIZE_EXPAND_FILL
	content_margin.add_theme_constant_override("margin_left", 6)
	content_margin.add_theme_constant_override("margin_right", 6)
	content_margin.add_theme_constant_override("margin_top", 6)
	content_margin.add_theme_constant_override("margin_bottom", 6)
	content_margin.add_child(content)
	layout.add_child(content_margin)

	if toggle_btn != null:
		toggle_btn.pressed.connect(func():
			content_margin.visible = not content_margin.visible
			toggle_btn.text = "▼" if content_margin.visible else "▶")

	return {
		"panel": panel,
		"header": header,
		"header_box": header_box,
		"title": title,
		"toggle_btn": toggle_btn,
		"content": content,
		"content_margin": content_margin
	}
