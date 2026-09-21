extends RefCounted

func run(w) -> Dictionary:
	var d = w.document
	var summary = d.get_document_summary()
	var operations: Array = []
	operations.append(w.begin_action("M3B semantic edits"))
	var table: Dictionary = summary.blend_key_tables[0].duplicate(true)
	table.base_key_idx = (int(table.base_key_idx) + 1) % table.keys.size()
	operations.append(d.write_blend_key_table(table, true))
	var constraint: Dictionary = summary.blend_constraints[0].duplicate(true)
	constraint.weights[0] = 0.25
	operations.append(d.write_blend_constraint(constraint, true))
	var kinds: Array = []
	for source in summary.blend_bindings:
		if source.target_kind in kinds:
			continue
		var binding: Dictionary = source.duplicate(true)
		kinds.append(binding.target_kind)
		var forms: Array = binding.keyforms.items
		for form in forms:
			match binding.target_kind:
				"mesh": form.positions[0].x += 0.001
				"warp": form.points[0].x += 0.001
				"rotation": form.angle = float(form.get("angle", 0.0)) + 0.1
				"part": form.draw_order += 1.0
		operations.append(d.write_blend_binding(binding, true))
	var parameter = w.new_id()
	operations.append(d.create_parameter({"id": parameter, "runtime_id": "M3BGlue", "name": "Glue strength", "minimum": 0.0, "maximum": 1.0, "default_value": 0.0, "kind": "normal"}))
	var glue: Dictionary = summary.glues[0].duplicate(true)
	glue.binding = {"axes": [{"parameter_id": parameter, "keys": [0.0, 0.5, 1.0]}], "keyforms": [{"intensity": 0.0}, {"intensity": 0.4}, {"intensity": 1.0}]}
	operations.append(d.write_glue(glue, true))
	# Rename a stable vertex while replacing all dependent geometry and Glue references.
	var topology: Dictionary = d.get_mesh_topology_snapshot(glue.mesh_a_id)
	var old_vertex: int = topology.mesh.vertex_ids[0]
	var new_vertex: int = topology.mesh.vertex_ids.max() + 1
	topology.mesh.vertex_ids[0] = new_vertex
	for triangle in topology.mesh.triangles:
		for i in triangle.size():
			if triangle[i] == old_vertex: triangle[i] = new_vertex
	topology.vertex_mapping[0][1] = new_vertex
	for g in topology.glues:
		for pair in g.pairs:
			if g.mesh_a_id == topology.mesh.id and pair.vertex_a == old_vertex: pair.vertex_a = new_vertex
			if g.mesh_b_id == topology.mesh.id and pair.vertex_b == old_vertex: pair.vertex_b = new_vertex
	operations.append(d.replace_mesh_topology(topology))
	var ended: Dictionary = w.end_action()
	var edited = d.get_document_summary()
	# Complex edits are explicit history barriers until their local deltas are implemented.
	var denied_undo: Dictionary = w.undo()
	var denied_redo: Dictionary = w.redo()
	var redone: Dictionary = d.get_document_summary()
	var history_barrier_ok: bool = ended.code == "NO_ACTION" and denied_undo.code == "NO_UNDO" and denied_redo.code == "NO_REDO" and redone == edited and d.get_history_state().warning == "HISTORY_UNSUPPORTED_EDIT"
	# Both rejected writes must preserve content/revision, including through the bridge.
	var revision: int = redone.revision
	var bad: Dictionary = glue.duplicate(true)
	bad.binding.keyforms.pop_back()
	var rejected: Dictionary = d.write_glue(bad, true)
	var invalid_ok: bool = not rejected.ok and d.get_document_summary().revision == revision
	var application = w.surface.get_ref().get_tree().root.get_child(0)
	var inspection_ok := true
	for object_id in [table.id, constraint.id, summary.blend_bindings[0].id, glue.id, parameter]:
		application._on_object_selected(object_id)
		inspection_ok = inspection_ok and application.inspector_dock.id_val.text == object_id
	application._on_object_selected("")
	return {"ok": operations.all(func(r): return r.ok) and history_barrier_ok and invalid_ok and kinds.size() == 4 and inspection_ok,
		"inspection_ok": inspection_ok, "operations": operations, "history_barrier_ok": history_barrier_ok, "invalid_ok": invalid_ok, "edited_kinds": kinds,
		"summary": redone, "glue_parameter": parameter}
