extends Logger
## Godot can invoke Logger from any thread. Never log from these callbacks.
var mutex := Mutex.new()
var active := false
var errors: Array[Dictionary] = []
var messages: Array[Dictionary] = []

func begin() -> void:
	mutex.lock()
	errors.clear()
	messages.clear()
	active = true
	mutex.unlock()

func finish() -> Dictionary:
	mutex.lock()
	active = false
	var result := {"errors": errors.duplicate(true), "logs": messages.duplicate(true)}
	mutex.unlock()
	return result

func _log_error(function: String, file: String, line: int, code: String, rationale: String, _editor_notify: bool, error_type: int, _backtraces: Array[ScriptBacktrace]) -> void:
	mutex.lock()
	if active:
		errors.append({"function": function, "file": file, "line": line, "message": rationale if not rationale.is_empty() else code, "type": error_type})
	mutex.unlock()

func _log_message(message: String, error: bool) -> void:
	mutex.lock()
	if active:
		messages.append({"message": message, "stderr": error})
	mutex.unlock()
