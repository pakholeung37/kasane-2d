extends Node
## Atomic file publication, serial execution and durable at-most-once claims.
signal completed(result: Dictionary)
var workspace: RefCounted
var canvas: Control
var host: RefCounted
var directory := ""
var app_id := ""
var busy := false
var enabled := false
var elapsed := 0.0
var owns_lock := false

func start(owner: RefCounted, surface: Control, path: String) -> Dictionary:
	workspace = owner
	canvas = surface
	host = preload("res://script_host.gd").new(workspace)
	app_id = workspace.new_id()
	directory = ProjectSettings.globalize_path(path)
	for folder in ["requests", "results", "claims", "scripts", "observations"]:
		var error := DirAccess.make_dir_recursive_absolute(directory.path_join(folder))
		if error != OK:
			return workspace.failure("QUEUE_WRITE_FAILED", directory)
	var lock := directory.path_join("owner.lock")
	if DirAccess.dir_exists_absolute(lock):
		var previous_owner: Variant = JSON.parse_string(FileAccess.get_file_as_string(lock.path_join("owner.json")))
		if not previous_owner is Dictionary or not previous_owner.get("pid") is float or OS.is_process_running(int(previous_owner.pid)):
			return workspace.failure("QUEUE_IN_USE", "Choose a separate agent directory for each running application.")
		DirAccess.remove_absolute(lock.path_join("owner.json"))
		DirAccess.remove_absolute(lock)
	if DirAccess.make_dir_absolute(lock) != OK:
		return workspace.failure("QUEUE_IN_USE", "Another application claimed this directory.")
	owns_lock = true
	if not write_json(lock.path_join("owner.json"), {"pid": OS.get_process_id(), "app_id": app_id}):
		return workspace.failure("QUEUE_WRITE_FAILED", "Cannot publish queue ownership.")
	workspace.document.changed.connect(publish_status)
	enabled = true
	publish_status({})
	return {"ok": true, "directory": directory, "app_id": app_id}

func _exit_tree() -> void:
	if host != null:
		host.close()
	if enabled:
		write_json(directory.path_join("status.json"), {"running": false, "app_id": app_id})
	if owns_lock:
		DirAccess.remove_absolute(directory.path_join("owner.lock/owner.json"))
		DirAccess.remove_absolute(directory.path_join("owner.lock"))

func write_json(path: String, data: Dictionary) -> bool:
	var temporary := path + ".tmp"
	var file := FileAccess.open(temporary, FileAccess.WRITE)
	if file == null:
		return false
	file.store_string(JSON.stringify(workspace.json_value(data), "  "))
	file.flush()
	var error := file.get_error()
	file.close()
	return error == OK and DirAccess.rename_absolute(temporary, path) == OK

func publish_status(_change: Dictionary) -> void:
	if enabled:
		var summary: Dictionary = workspace.document.get_document_state()
		write_json(directory.path_join("status.json"), {"running": true, "app_id": app_id, "pid": OS.get_process_id(),
			"generation": summary.generation, "revision": summary.revision, "path": summary.path, "busy": busy,
			"engine": Engine.get_version_info(), "directory": directory})

func _process(delta: float) -> void:
	elapsed += delta
	if not enabled or busy or elapsed < 0.1:
		return
	elapsed = 0
	var names := DirAccess.get_files_at(directory.path_join("requests"))
	names.sort()
	for name in names:
		if name.ends_with(".json"):
			busy = true
			publish_status({})
			await consume(name)
			busy = false
			publish_status({})
			break

func consume(name: String) -> void:
	var path := directory.path_join("requests").path_join(name)
	var id := name.trim_suffix(".json")
	var output := directory.path_join("results").path_join(name)
	if FileAccess.file_exists(output):
		DirAccess.remove_absolute(path)
		return
	var before: Dictionary = workspace.document.get_document_state()
	var result := {"ok": false, "executed": false, "phase": "request", "code": "INVALID_REQUEST",
		"id": id, "app_id": app_id, "generation": before.generation, "start_revision": before.revision, "end_revision": before.revision}
	var request: Variant = JSON.parse_string(FileAccess.get_file_as_string(path))
	var claim := directory.path_join("claims").path_join(name)
	if FileAccess.file_exists(claim):
		result.code = "EXECUTION_OUTCOME_UNKNOWN"
		result.executed = null
		result.message = "A durable claim exists without a result. It will not be replayed; inspect the document and logs."
	elif request is Dictionary and request.get("id") == id and request.get("script_path") is String and request.get("generation") is float and request.get("app_id") is String:
		if request.app_id != app_id or request.generation != before.generation:
			result.code = "STALE_DOCUMENT"
		elif not write_json(claim, {"id": id, "app_id": app_id, "generation": before.generation, "start_revision": before.revision}):
			result.code = "CLAIM_WRITE_FAILED"
		else:
			result = host.execute(request.script_path, int(request.generation))
			result.id = id
			result.app_id = app_id
			if request.get("observe", false) == true and result.ok:
				var observation_path := directory.path_join("observations").path_join(id + ".png")
				var object_id: String = request.get("object_id", "") if request.get("object_id", "") is String else ""
				result.observation = await canvas.observe(observation_path, object_id)
				if not result.observation.ok:
					result.ok = false
					result.phase = "observation"
					result.code = result.observation.code
	if write_json(output, result):
		DirAccess.remove_absolute(path)
	else:
		result.ok = false
		result.code = "RESULT_WRITE_FAILED"
	completed.emit(result)
