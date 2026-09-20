extends RefCounted
const MODEL = "__MODEL__"
func run(w) -> Dictionary:
	var imported = w.import_model(MODEL)
	if not imported.ok:
		return imported
	var d = w.document
	var summary = d.get_document_summary()
	var mesh = d.get_mesh_snapshot(summary.meshes[0].id)
	var operations = []
	# Explicitly detach this mesh, replacing its keyforms in the new root domain.
	var evaluated = d.evaluate_mesh(mesh.id)
	var positions = PackedVector2Array()
	for point in evaluated.positions:
		positions.append(Vector2(point.x * summary.pixels_per_unit + summary.canvas_origin.x + 2.0, summary.canvas_origin.y - point.y * summary.pixels_per_unit))
	operations.append(d.set_deform_parent(mesh.id, ""))
	operations.append(d.set_vertex_positions(mesh.id, mesh.vertex_ids, positions))
	var binding_id = ""
	for binding in summary.bindings:
		if binding.mesh_id == mesh.id:
			binding_id = binding.id
			# Preserve identity while rebinding to a newly created parameter.
			operations.append(d.erase_object(binding.id))
	var parameter = w.new_id()
	operations.append(d.create_parameter({"id": parameter, "runtime_id": "M5Edit", "name": "M5 edit", "minimum": -1.0, "maximum": 1.0, "default_value": 0.0}))
	if binding_id.is_empty():
		binding_id = w.new_id()
	operations.append(w.complete_binding({"id": binding_id, "mesh_id": mesh.id, "axes": [{"parameter_id": parameter, "keys": [-1.0,0.0,1.0]}]}, {"positions": positions}))
	var form = w.get_keyform(binding_id, [1.0]).data
	for point in form.positions:
		point[0] += 3.0
	operations.append(w.set_keyform(binding_id, form))
	var properties = d.get_mesh_snapshot(mesh.id).properties
	properties.appearance.opacity *= 0.8
	properties.double_sided = true
	operations.append(d.set_mesh_properties(mesh.id, properties))
	w.fit_view()
	return {"ok": operations.all(func(r): return r.ok), "operations": operations, "mesh_id": mesh.id, "parameter_id": parameter, "summary": d.get_document_summary(), "frame": d.get_frame(), "import": imported}
