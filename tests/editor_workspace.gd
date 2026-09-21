extends SceneTree

var checks := 0
var failures: Array[String] = []

func check(value: bool, message: String) -> void:
	checks += 1
	if not value:
		failures.append(message)

func _initialize() -> void:
	call_deferred("run")

func run() -> void:
	var app = load("res://main.tscn").instantiate()
	root.add_child(app)
	var workspace = app.workspace
	var doc = workspace.document
	check(workspace.new_project(Vector2(32, 32)).ok, "New project from application service")
	var id: String = workspace.new_id()
	check(doc.write_part({"id": id, "runtime_id": "Part", "name": "First", "parent_id": "", "enabled": true, "draw_order": 0}).ok, "Create Part through public binding")
	check(app.object_tree.get_root().get_first_child().get_text(0) == "Part · First", "UI observes script-created object")
	app.selected_id = id
	app.show_selection()
	check(app.inspector.text.contains(id), "Inspector exposes stable ID")
	var path: String = OS.get_cmdline_user_args()[0] + "/editor-project.json"
	check(workspace.save_project(path).ok, "Save from application service")
	check(not doc.get_document_summary().modified, "Save clears dirty state")
	check(workspace.new_project().ok, "Replace with another project")
	check(workspace.open_project(path).ok, "Reopen saved project")
	check(doc.get_document_summary().parts[0].id == id, "Stable identity survives reopen")
	check(app.object_tree.get_root().get_first_child().get_text(0) == "Part · First", "UI refreshes on reopen")
	var png_path: String = OS.get_cmdline_user_args()[0] + "/history.png"
	var image := Image.create(32, 32, false, Image.FORMAT_RGBA8)
	image.fill(Color.WHITE)
	check(image.save_png(png_path) == OK, "Create history texture")
	var asset: Dictionary = workspace.import_png(png_path)
	check(asset.ok, "Import history texture")
	var mesh: Dictionary = workspace.create_rectangle(asset.asset_id)
	check(mesh.ok, "Create history mesh")
	var old_name: String = doc.get_mesh_snapshot(mesh.mesh_id).name
	check(workspace.begin_action("UI name edit").ok, "Workspace begins native action")
	check(doc.rename_mesh(mesh.mesh_id, "intermediate").ok, "First name edit")
	check(doc.rename_mesh(mesh.mesh_id, "final").ok, "Repeated name edit")
	check(workspace.end_action().ok and doc.get_history_state().undo_steps == 1, "Workspace commits one delta")
	app.console_dock.undo_requested.emit()
	check(doc.get_mesh_snapshot(mesh.mesh_id).name == old_name, "Undo button invokes Rust history")
	app.console_dock.redo_requested.emit()
	check(doc.get_mesh_snapshot(mesh.mesh_id).name == "final", "Redo button invokes Rust history")
	check(doc.get_history_state().estimated_bytes < 1024, "UI history retains only the name")
	var report := {"status": "passed" if failures.is_empty() else "failed", "checks": checks, "failures": failures}
	var file := FileAccess.open(OS.get_cmdline_user_args()[0] + "/editor-workspace-report.json", FileAccess.WRITE)
	file.store_string(JSON.stringify(report, "  "))
	file.close()
	print(JSON.stringify(report))
	app.free()
	quit(0 if failures.is_empty() else 1)
