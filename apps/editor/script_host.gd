extends RefCounted
## Synchronous whole-script execution; no implicit transaction or cancellation.
var logger = preload("res://script_logger.gd").new()
var workspace: RefCounted
var running := false

func _init(owner: RefCounted) -> void:
	workspace = owner
	OS.add_logger(logger)

func close() -> void:
	OS.remove_logger(logger)

func execute(path: String, generation: int) -> Dictionary:
	var before: Dictionary = workspace.document.get_document_state()
	var result := {"ok": false, "executed": false, "phase": "request", "generation": before.generation,
		"start_revision": before.revision, "end_revision": before.revision, "errors": [], "logs": []}
	if running:
		result.code = "BUSY"
		return result
	if generation != before.generation:
		result.code = "STALE_DOCUMENT"
		return result
	var file := FileAccess.open(path, FileAccess.READ)
	if file == null:
		result.code = "SCRIPT_READ_FAILED"
		return result
	var script := GDScript.new()
	var virtual_path: String = path.get_base_dir().path_join(".agent-" + workspace.new_id() + ".gd")
	script.resource_path = virtual_path
	result.script_path = path
	script.source_code = file.get_as_text()
	file.close()
	running = true
	logger.begin()
	var error := script.reload()
	var instance: RefCounted
	result.phase = "compile"
	if error == OK:
		if script.get_instance_base_type() != "RefCounted":
			result.code = "SCRIPT_CONTRACT"
			result.message = "Script must extend RefCounted and implement run(workspace) -> Dictionary."
		else:
			result.phase = "runtime"
			result.executed = true # _init can perform external side effects.
			instance = script.new()
			if instance != null and instance.has_method("run"):
				var business: Variant = instance.call("run", workspace)
				if business is Dictionary and business.get("ok") is bool:
					result.business = business
					result.ok = business.ok
					result.phase = "business"
					result.code = business.get("code", "")
					if str(result.code).is_empty():
						result.code = "OK" if business.ok else "BUSINESS_FAILED"
				else:
					result.code = "SCRIPT_CONTRACT"
					result.message = "run must synchronously return a Dictionary containing ok: bool."
			else:
				result.code = "SCRIPT_CONTRACT"
	else:
		result.code = "COMPILE_ERROR"
		result.compiler_error = error
	var captured: Dictionary = logger.finish()
	result.errors = captured.errors
	result.logs = captured.logs
	for issue in captured.errors:
		if issue.file == virtual_path:
			issue.file = path
		if issue.type != Logger.ERROR_TYPE_WARNING:
			result.ok = false
			if error == OK:
				result.phase = "runtime"
				result.code = "RUNTIME_ERROR"
	var after: Dictionary = workspace.document.get_document_state()
	result.end_revision = after.revision
	result.end_generation = after.generation
	running = false
	return result
