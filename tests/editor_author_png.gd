extends RefCounted
const OUTPUT = "__OUTPUT__"
var operations: Array = []

func record(result: Dictionary, operation: String) -> bool:
	operations.append({"operation": operation, "result": result})
	return result.ok

func run(w) -> Dictionary:
	var d = w.document
	if not record(w.new_project(Vector2(320, 240)), "new"):
		return {"ok": false, "operations": operations}
	var objects: Array = []
	for group in 2:
		var dimensions = Vector2i(96, 64) if group == 0 else Vector2i(64, 120)
		var offset = Vector2(32, 48) if group == 0 else Vector2(180, 90)
		var img = Image.create(dimensions.x, dimensions.y, false, Image.FORMAT_RGBA8)
		img.fill(Color(1, 0.25, 0.15, 0.85) if group == 0 else Color(0.15, 0.4, 1, 0.9))
		img.fill_rect(Rect2i(0, 0, 20, 20), Color(0.15, 1, 0.3))
		var path = OUTPUT.path_join("layer-%d.png" % group)
		if img.save_png(path) != OK:
			return {"ok": false, "code": "PNG_WRITE_FAILED"}
		var asset = w.import_png(path, Vector2(320, 240), offset)
		if not record(asset, "PNG import"):
			return {"ok": false, "operations": operations}
		var made = w.create_rectangle(asset.asset_id, offset, "Layer %d" % group)
		if not record(made, "rectangle"):
			return {"ok": false, "operations": operations}
		var part = w.new_id()
		var rotation = w.new_id()
		var warp = w.new_id()
		var parameter = w.new_id()
		var binding = w.new_id()
		var scene_binding = w.new_id()
		var center = offset + Vector2(dimensions) / 2
		for entry in [
			[d.write_part({"id": part, "runtime_id": "Part%d" % group, "name": "Group %d" % group, "parent_id": "", "enabled": true, "draw_order": group}), "Part"],
			[d.create_rotation(rotation, "Rotation %d" % group, center, 0), "Rotation"],
			[d.create_warp(warp, "Warp %d" % group, offset - Vector2(10, 10), Vector2(dimensions) + Vector2(20, 20), 2, 2), "Warp"],
			[d.set_organization_parent(made.mesh_id, part), "mesh organization"],
			[d.set_organization_parent(rotation, part), "rotation organization"],
			[d.set_organization_parent(warp, part), "warp organization"],
			[d.set_deform_parent(warp, rotation) if group == 0 else d.set_deform_parent(rotation, warp), "nested deformation"],
			[d.set_deform_parent(made.mesh_id, warp if group == 0 else rotation), "mesh deformation"],
			[d.create_parameter({"id": parameter, "runtime_id": "Movement%d" % group, "name": "Movement %d" % group, "minimum": -1.0, "maximum": 1.0, "default_value": 0.0}), "parameter"],
		]:
			if not record(entry[0], entry[1]):
				return {"ok": false, "operations": operations}
		# Parent assignment changes coordinate domain; supply explicit local data.
		var positions = PackedVector2Array()
		if group == 0:
			var controls = PackedVector2Array()
			for y in 3:
				for x in 3:
					controls.append(Vector2((-dimensions.x/2.0-10+x*(dimensions.x+20)/2.0)/100, (dimensions.y/2.0+10-y*(dimensions.y+20)/2.0)/100))
			d.set_warp_points(warp, controls)
			positions = PackedVector2Array([Vector2(10,10),Vector2(dimensions.x+10,10),Vector2(dimensions.x+10,dimensions.y+10),Vector2(10,dimensions.y+10)])
			for i in positions.size():
				positions[i] /= Vector2(dimensions)+Vector2(20,20)
		else:
			var transform = w.find_object(rotation).data
			transform.rotation.origin = [0.5, 0.5]
			transform.base_angle = 180.0 # Cancel the parent grid's downward Y direction.
			d.write_transform(transform, true)
			positions = PackedVector2Array([Vector2(-dimensions.x/2.0,dimensions.y/2.0),Vector2(dimensions.x/2.0,dimensions.y/2.0),Vector2(dimensions.x/2.0,-dimensions.y/2.0),Vector2(-dimensions.x/2.0,-dimensions.y/2.0)])
			for i in positions.size():
				positions[i] /= 100
		d.set_vertex_positions(made.mesh_id, PackedInt64Array([1,2,3,4]), positions)
		if not record(w.complete_binding({"id": binding, "mesh_id": made.mesh_id, "axes": [{"parameter_id": parameter, "keys": [-1.0, 0.0, 1.0]}]}, {"positions": positions}), "mesh binding"):
			return {"ok": false, "operations": operations}
		for key in [-1.0, 1.0]:
			var form = w.get_keyform(binding, [key]).data
			for point in form.positions:
				point[0] += key * (10.0/(dimensions.x+20) if group == 0 else 0.1)
			if not record(w.set_keyform(binding, form), "mesh keyform"):
				return {"ok": false, "operations": operations}
		var rotation_data = w.find_object(rotation).data
		if not record(w.complete_binding({"id": scene_binding, "target_id": rotation, "axes": [{"parameter_id": parameter, "keys": [-1.0, 0.0, 1.0]}]}, {"positions": [], "rotation": rotation_data.rotation, "appearance": rotation_data.appearance, "draw_order": 0}, true), "rotation binding"):
			return {"ok": false, "operations": operations}
		for key in [-1.0, 1.0]:
			var form = w.get_keyform(scene_binding, [key]).data
			form.rotation.angle = key * 8
			if not record(w.set_keyform(scene_binding, form), "rotation keyform"):
				return {"ok": false, "operations": operations}
		objects.append({"mesh": made.mesh_id, "part": part, "rotation": rotation, "warp": warp, "parameter": parameter, "binding": binding, "scene_binding": scene_binding})
	w.fit_view()
	return {"ok": true, "objects": objects, "operations": operations, "frame": d.get_frame(), "summary": d.get_document_summary()}
