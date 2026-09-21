extends PanelContainer

const EditorTheme = preload("res://ui/theme.gd")

signal object_selected(id: String)

var tree: Tree
var dock_info: Dictionary
var current_selected_id := ""
var id_to_tree_item := {}

func _ready() -> void:
	dock_info = EditorTheme.make_dock("对象与组织 (Hierarchy)")
	add_child(dock_info.panel)

	var content: VBoxContainer = dock_info.content

	tree = Tree.new()
	tree.size_flags_vertical = Control.SIZE_EXPAND_FILL
	tree.custom_minimum_size.y = 120
	tree.hide_root = false
	tree.item_selected.connect(_on_item_selected)
	content.add_child(tree)

	var root := tree.create_item()
	root.set_text(0, "尚无 Document")

func populate(summary: Dictionary, workspace: RefCounted) -> void:
	tree.clear()
	id_to_tree_item.clear()

	var root := tree.create_item()
	var doc_title: String = summary.id if summary.get("initialized", false) else "尚无 Document"
	root.set_text(0, doc_title)
	root.set_metadata(0, "")
	id_to_tree_item[""] = root

	var pending: Array = summary.get("parts", []).duplicate()
	while not pending.is_empty():
		var progress := false
		for part in pending.duplicate():
			var p_parent: String = str(part.get("parent_id", ""))
			if id_to_tree_item.has(p_parent):
				var parent_item: TreeItem = id_to_tree_item[p_parent]
				var item := tree.create_item(parent_item)
				item.set_text(0, "Part · " + str(part.get("name", part.get("id", ""))))
				item.set_metadata(0, part.get("id", ""))
				id_to_tree_item[part.get("id", "")] = item
				pending.erase(part)
				progress = true
		if not progress:
			for part in pending:
				var item := tree.create_item(root)
				item.set_text(0, "Part · " + str(part.get("name", part.get("id", ""))))
				item.set_metadata(0, part.get("id", ""))
				id_to_tree_item[part.get("id", "")] = item
			pending.clear()

	var meshes: Array = summary.get("meshes", [])
	var deformers: Array = summary.get("deformers", [])

	for mesh in meshes:
		var snapshot: Dictionary = workspace.document.get_mesh_snapshot(mesh.id) if workspace != null else {}
		var org_parent: String = str(snapshot.get("organization_parent", ""))
		var parent_item: TreeItem = id_to_tree_item.get(org_parent, root)
		var item := tree.create_item(parent_item)
		item.set_text(0, "Mesh · " + str(mesh.get("name", mesh.id)))
		item.set_metadata(0, mesh.id)
		id_to_tree_item[mesh.id] = item

	for def in deformers:
		var snapshot: Dictionary = workspace.document.get_deformer_snapshot(def.id) if workspace != null else {}
		var org_parent: String = str(snapshot.get("organization_parent", ""))
		var parent_item: TreeItem = id_to_tree_item.get(org_parent, root)
		var kind: String = snapshot.get("kind", "Deformer")
		var item := tree.create_item(parent_item)
		item.set_text(0, kind + " · " + str(def.get("name", def.id)))
		item.set_metadata(0, def.id)
		id_to_tree_item[def.id] = item

	for group in ["blend_key_tables", "blend_constraints", "blend_bindings", "glues"]:
		var folder := tree.create_item(root)
		folder.set_text(0, group + " (%d)" % summary.get(group, []).size())
		folder.set_metadata(0, "")
		folder.collapsed = true
		for entry in summary.get(group, []):
			var item := tree.create_item(folder)
			item.set_text(0, str(entry.get("name", entry.id)))
			item.set_metadata(0, entry.id)
			id_to_tree_item[entry.id] = item

	if not current_selected_id.is_empty():
		select_id(current_selected_id)

func select_id(id: String) -> void:
	current_selected_id = id
	if id_to_tree_item.has(id):
		var item: TreeItem = id_to_tree_item[id]
		tree.set_selected(item, 0)
		tree.scroll_to_item(item)

func _on_item_selected() -> void:
	var selected := tree.get_selected()
	if selected != null:
		var id := str(selected.get_metadata(0))
		current_selected_id = id
		object_selected.emit(id)
