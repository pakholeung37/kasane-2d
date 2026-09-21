extends RefCounted

func run(w) -> Dictionary:
	var d = w.document
	var before: Dictionary = d.get_document_summary()
	var binding: Dictionary = {}
	var surface: Dictionary = {}
	for candidate in before.scene_bindings:
		if candidate.axes.size() != 1 or candidate.axes[0].keys.size() < 2:
			continue
		for os in before.offscreens:
			if os.part_id == candidate.target_id:
				binding = candidate.duplicate(true)
				surface = os.duplicate(true)
				break
		if not binding.is_empty():
			break
	if binding.is_empty():
		return {"ok": false, "message": "No Part/Offscreen binding to exercise"}
	var axis: Dictionary = binding.axes[0]
	var middle: float = (float(axis.keys[0]) + float(axis.keys[1])) * 0.5
	axis.keys.insert(1, middle)
	var form: Dictionary = binding.keyforms[0].duplicate(true)
	form.keys[0] = middle
	binding.keyforms.insert(1, form)
	if surface.part_keyform_indices.is_empty():
		surface.part_keyform_indices.resize(binding.keyforms.size() - 1)
		surface.part_keyform_indices.fill(-1)
	# Explicit caller policy: the inserted slot has a new, chosen appearance.
	surface.part_keyform_indices.insert(1, surface.keyforms.size())
	surface.keyforms.append({"opacity": 0.6, "multiply": [0.7, 0.8, 0.9], "screen": [0.0, 0.0, 0.0]})
	var bad: Dictionary = surface.duplicate(true)
	bad.part_keyform_indices.pop_back()
	var rejected: Dictionary = d.replace_part_binding_with_offscreen(binding, bad)
	var untouched: Dictionary = d.get_document_summary()
	var reject_ok: bool = not rejected.ok and untouched.revision == before.revision and untouched.offscreens == before.offscreens and untouched.scene_bindings == before.scene_bindings
	var operations: Array = [w.begin_action("M3C joint keyform insert")]
	operations.append(d.replace_part_binding_with_offscreen(binding, surface))
	operations.append(w.end_action())
	var edited: Dictionary = d.get_document_summary()
	var revision_ok: bool = edited.revision == before.revision + 1
	w.undo_redo.undo()
	var undone: Dictionary = d.get_document_summary()
	var undo_ok: bool = undone.offscreens == before.offscreens and undone.scene_bindings == before.scene_bindings
	w.undo_redo.redo()
	var redone: Dictionary = d.get_document_summary()
	var redo_ok: bool = redone.offscreens == edited.offscreens and redone.scene_bindings == edited.scene_bindings
	return {"ok": operations.all(func(r): return r.ok) and reject_ok and revision_ok and undo_ok and redo_ok,
		"parameter_id": axis.parameter_id, "samples": [float(axis.keys[0]), middle, (middle + float(axis.keys[2])) * 0.5],
		"operations": operations, "reject_ok": reject_ok, "revision_ok": revision_ok, "undo_ok": undo_ok, "redo_ok": redo_ok}
