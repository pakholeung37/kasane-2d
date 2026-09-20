extends RefCounted
var records: Array = []
var directory := ""
var app: Control
var w: RefCounted
var doc: RefCounted

func check(actual: Variant, expected: Variant, name: String) -> void:
	records.append({"name": name, "expected": expected, "actual": actual, "status": "passed" if actual == expected else "failed"})

func write_file(name: String, source: String) -> String:
	var path := directory.path_join(name)
	var file := FileAccess.open(path, FileAccess.WRITE)
	file.store_string(source)
	file.close()
	return path

func verify(workspace: RefCounted, application: Control, output_directory: String, script_host: RefCounted) -> Dictionary:
	directory = output_directory
	app = application
	w = workspace
	doc = w.document
	check(w.new_project(Vector2(320, 240)).ok, true, "new project")
	var image := Image.create(96, 64, false, Image.FORMAT_RGBA8)
	image.fill(Color(1, 0.3, 0.2, 0.8))
	image.fill_rect(Rect2i(0, 0, 24, 16), Color.GREEN)
	var png := directory.path_join("layer.png")
	image.save_png(png)
	check(w.import_png(png).code, "INVALID_CANVAS", "implicit canvas mismatch rejected")
	var imported: Dictionary = w.import_png(png, Vector2(320, 240), Vector2(32, 48))
	check(imported.ok, true, "cropped PNG import")
	var mesh_result: Dictionary = w.create_rectangle(imported.asset_id, Vector2(32, 48))
	check(mesh_result.ok, true, "rectangle from PNG")
	var mesh: String = mesh_result.mesh_id
	var handle = doc.get_mesh(mesh)
	var copy: PackedVector2Array = handle.get_positions()
	copy[0] += Vector2(1, 2)
	check(handle.get_positions()[0], Vector2(32, 48), "Packed arrays read as copy")
	check(handle.set_vertex_positions(handle.get_vertex_ids(), copy).ok, true, "explicit bulk write")
	check(handle.get_positions()[0], Vector2(33, 50), "bulk write visible")
	var snapshot: Dictionary = doc.get_mesh_snapshot(mesh)
	check(doc.erase_object(mesh).ok, true, "delete mesh")
	check(doc.create_mesh(snapshot).ok, true, "recreate same ID")
	check(handle.is_valid(), false, "deleted handle cannot resurrect")
	check(handle.set_vertex_positions(handle.get_vertex_ids(), copy).code, "STALE_HANDLE", "stale writes rejected")
	var rotation: String = w.new_id()
	check(doc.create_rotation(rotation, "Rotation", Vector2(80, 80), 0).ok, true, "create rotation")
	var deformer = doc.get_deformer(rotation)
	check(doc.erase_object(rotation).ok, true, "delete rotation")
	check(doc.create_rotation(rotation, "Replacement", Vector2(80, 80), 0).ok, true, "recreate rotation ID")
	check(deformer.is_valid(), false, "deleted rotation handle cannot resurrect")
	check(deformer.update_rotation(Vector2.ZERO, 20).code, "STALE_HANDLE", "stale rotation writes rejected")
	var part: String = w.new_id()
	check(doc.write_part({"id": part, "runtime_id": "Part", "name": "Layer", "parent_id": "", "enabled": true, "draw_order": 0}).ok, true, "create Part")
	var part_data: Dictionary = w.find_object(part).data
	part_data.name = "Renamed"
	check(doc.write_part(part_data, true).ok, true, "rename Part")
	check(doc.set_organization_parent(mesh, part).ok, true, "mesh organization")
	check(doc.erase_object(part).code, "OBJECT_REFERENCED", "referenced Part deletion rejected")
	check(doc.set_organization_parent(part, part).ok, false, "organization cycle rejected")
	var warp: String = w.new_id()
	check(doc.create_warp(warp, "Warp", Vector2(20, 20), Vector2(150, 150), 2, 2).ok, true, "create Warp")
	check(doc.set_deform_parent(warp, rotation).ok, true, "nested deformers")
	check(doc.set_deform_parent(rotation, warp).ok, false, "deformer cycle rejected")
	check(doc.set_deform_parent(mesh, warp).ok, true, "mesh deformation parent")
	var param: String = w.new_id()
	var parameter := {"id": param, "runtime_id": "Move", "name": "Move", "minimum": -1.0, "maximum": 1.0, "default_value": 0.0}
	check(doc.create_parameter(parameter).ok, true, "create parameter")
	parameter.name = "Movement"
	check(doc.replace_parameter(parameter).ok, true, "replace parameter")
	var binding: String = w.new_id()
	check(w.complete_binding({"id": binding, "mesh_id": mesh, "axes": [{"parameter_id": param, "keys": [-1.0, 0.0, 1.0]}]}, {"positions": copy}).ok, true, "fill complete keyform product")
	check(doc.references_to(param).referrers.has(binding), true, "query parameter references")
	check(doc.erase_object(param).code, "OBJECT_REFERENCED", "bound parameter deletion rejected")
	var keyform: Dictionary = w.get_keyform(binding, [1.0]).data
	keyform.positions[0][0] += 10
	check(w.set_keyform(binding, keyform).ok, true, "write exact key combination")
	check(w.get_keyform(binding, [0.0]).data.positions[0], [33.0, 50.0], "keyform update preserves other array-based forms")
	check(w.get_keyform(binding, [1.0]).data.positions[0], [43.0, 50.0], "nested Array coordinates roundtrip without zero conversion")
	check(w.get_keyform(binding, [0.5]).code, "INVALID_KEY_COMBINATION", "sample is not a keyform")
	parameter.maximum = 0.5
	check(doc.replace_parameter(parameter).ok, false, "range cannot exclude binding keys")
	var geometry: Dictionary = doc.get_mesh_snapshot(mesh)
	geometry.vertex_ids.append(5)
	geometry.base_positions.append(Vector2(80, 80))
	geometry.uvs.append(Vector2(0.5, 0.5))
	geometry.triangles = PackedInt64Array([1,2,5,2,3,5,3,4,5,4,1,5])
	var bound: Dictionary = w.find_object(binding).data
	var old_revision: int = doc.get_document_summary().revision
	check(doc.replace_mesh_with_keyforms(geometry, bound).ok, false, "bound topology rejects missing vertex forms")
	check(doc.get_document_summary().revision, old_revision, "failed topology replacement is atomic")
	for form in bound.keyforms:
		form.positions.append([80.0,80.0])
	check(doc.replace_mesh_with_keyforms(geometry, bound).ok, true, "atomic bound topology and all keyforms")
	check(doc.get_document_summary().revision, old_revision + 1, "bound topology is one revision")
	check(doc.get_mesh_snapshot(mesh).vertex_ids.size(), 5, "topology vertex mapping updated")
	var fractional := bound.duplicate(true)
	fractional.axes[0].keys[1] = 0.1
	fractional.keyforms[1].keys = [0.1]
	check(doc.write_binding(fractional,true).ok,true,"fractional keyform grid")
	check(w.get_keyform(binding,[0.1]).ok,true,"explicit key lookup canonicalizes float32")
	check(doc.write_binding(bound,true).ok,true,"restore original key grid")
	check(doc.set_rotation(warp, Vector2.ZERO, 0).code, "WRONG_TRANSFORM_KIND", "rotation update rejects Warp")
	check(doc.set_warp_points(rotation,PackedVector2Array()).code,"WRONG_TRANSFORM_KIND","warp update rejects Rotation")
	var warp_binding: String = w.new_id()
	var warp_data: Dictionary = w.find_object(warp).data
	check(w.complete_binding({"id":warp_binding,"target_id":warp,"axes":[{"parameter_id":param,"keys":[-1.0,0.0,1.0]}]}, {"positions":warp_data.points,"rotation":warp_data.rotation,"appearance":warp_data.appearance,"draw_order":0.0}, true).ok, true, "Warp scene binding")
	var warp_form: Dictionary = w.get_keyform(warp_binding,[1.0]).data
	warp_form.positions[0][0] += 0.1
	check(w.set_keyform(warp_binding,warp_form).ok,true,"Warp keyform write")
	check(doc.erase_object(warp_binding).ok,true,"remove scene binding")
	var relocated := directory.path_join("relocated.png")
	var png_file := FileAccess.open(relocated,FileAccess.WRITE)
	png_file.store_buffer(FileAccess.get_file_as_bytes(png))
	png_file.close()
	check(w.files.relocate_asset(doc,imported.asset_id,relocated).ok,true,"explicit asset relocation")
	image.fill(Color.BLUE)
	image.save_png(png)
	check(w.files.replace_asset(doc,imported.asset_id,png).ok,true,"explicit asset replacement")
	check(w.files.relocate_asset(doc,imported.asset_id,relocated).ok,false,"relocation rejects different bytes")
	var before: Dictionary = doc.get_document_summary()
	check(doc.set_preview_values({param: 99.0}).parameters[0].value, 1.0, "preview clamps actual sample")
	check(doc.get_document_summary().revision, before.revision, "preview does not edit model")
	check(app.parameter_controls[param].value, 1.0, "UI shows actual preview value")
	app.parameter_controls[param].value = -0.5
	check(doc.get_frame().parameters[0].value, -0.5, "UI drives shared preview interface")
	check(doc.get_document_summary().revision, before.revision, "UI preview leaves Keyform unchanged")
	var source := "extends RefCounted\nfunc run(w):\n\tw.document.rename_mesh(\"%s\", \"script changed\")\n\tvar invalid = {}\n\tinvalid.missing_method()\n\treturn {\"ok\": true}\n" % mesh
	var runtime: Dictionary = script_host.execute(write_file("runtime_error.gd", source), before.generation)
	check(runtime.code, "RUNTIME_ERROR", "runtime error captured")
	check(runtime.executed, true, "runtime failed after execution")
	check(runtime.errors[0].line, 5, "runtime exact line")
	check(runtime.end_revision > runtime.start_revision, true, "partial write revisions recorded")
	check(doc.get_mesh_snapshot(mesh).name, "script changed", "runtime preserves completed write")
	var bad := "extends RefCounted\nfunc run(w)\n\treturn {}\n"
	var compile: Dictionary = script_host.execute(write_file("compile_error.gd", bad), before.generation)
	check(compile.code, "COMPILE_ERROR", "compile error captured")
	check(compile.executed, false, "compile failure not executed")
	check(compile.end_revision, compile.start_revision, "compile failure does not edit")
	check(compile.errors[0].line > 0, true, "compiler has line evidence")
	check(w.begin_action("rename").ok, true, "begin explicit Action")
	doc.rename_mesh(mesh, "Action name")
	check(w.end_action().ok, true, "commit Action")
	w.undo_redo.undo()
	check(doc.get_mesh_snapshot(mesh).name, "script changed", "Undo restores document snapshot")
	w.undo_redo.redo()
	check(doc.get_mesh_snapshot(mesh).name, "Action name", "Redo restores document snapshot")
	w.begin_action("cancel")
	doc.rename_mesh(mesh, "discard")
	check(w.cancel_action().ok, true, "cancel explicit Action")
	check(doc.get_mesh_snapshot(mesh).name, "Action name", "cancel restores document")
	var project := directory.path_join("project.json")
	check(w.save_project(project).ok, true, "save project")
	check(w.open_project(project).ok, true, "reopen project")
	check(script_host.execute(write_file("stale.gd", "extends RefCounted\nfunc run(w):\n return {\"ok\":true}\n"), before.generation).code, "STALE_DOCUMENT", "old request generation rejected")
	check(doc.get_mesh_snapshot(mesh).name, "Action name", "reopen preserves editing")
	check(doc.erase_object(binding).ok, true, "unbind mesh")
	check(doc.erase_object(param).ok, true, "delete unbound parameter")
	check(doc.set_deform_parent(mesh, "").ok, true, "detach mesh")
	check(doc.erase_object(warp).ok, true, "delete Warp")
	check(doc.erase_object(rotation).ok, true, "delete Rotation")
	check(doc.erase_object(mesh).ok, true, "delete final mesh")
	check(doc.erase_object(part).ok, true, "delete unreferenced Part")
	check(doc.erase_object(imported.asset_id).ok, true, "delete unused asset")
	var failures := records.filter(func(item): return item.status == "failed")
	var report := {"status": "passed" if failures.is_empty() else "failed", "checks": records.size(), "records": records, "script_results": [runtime, compile]}
	write_file("editor-contract-report.json", JSON.stringify(report, "  "))
	print(JSON.stringify({"status": report.status, "checks": records.size(), "failures": failures}))
	return report
