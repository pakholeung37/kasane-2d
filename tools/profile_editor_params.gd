extends SceneTree
## Run in a staged apps/editor project with a selected native library.
## User args: absolute repository root, absolute output JSON path.
## Measures main-thread calls separately from process-frame intervals.
## It imports fixture models into its own editor instance; do not use an
## existing user session. No screenshot capture or disk writes are timed.

func _initialize():
	run.call_deferred()

func stats(values: Array) -> Dictionary:
	values.sort()
	var total := 0.0
	for v in values: total += v
	return {"mean_ms":total / values.size(),"p50_ms":values[values.size()/2],"p95_ms":values[int(values.size()*0.95)]}

func run():
	var app = load("res://main.tscn").instantiate()
	root.add_child(app)
	for i in 10: await process_frame
	var results := []
	var repo: String = OS.get_cmdline_user_args()[0]
	for model in ["Mao", "Ren"]:
		var path: String = repo + "/third_party/CubismSdkForNative-5-r.5/Samples/Resources/" + model + "/" + model + ".model3.json"
		var imported: Dictionary = app.workspace.import_model(path)
		if not imported.ok:
			push_error(str(imported)); quit(1); return
		app.workspace.fit_view()
		for i in 15: await process_frame
		var doc = app.workspace.document
		var preview = app.workspace.surface.get_ref().preview
		var summary: Dictionary = doc.get_document_summary()
		var pid := ""
		for p in summary.parameters:
			if p.runtime_id == "ParamAngleX": pid = p.id
		var report := {"model":model,"render_stats":preview.get_render_stats(),"window_size":root.size,"viewport_size":app.canvas.viewport.size}
		for mode in ["idle", "get_frame", "set_without_signals", "refresh_geometry", "set_preview_values", "slider_event"]:
			var cpu := []
			var wall := []
			doc.set_block_signals(mode == "set_without_signals")
			for i in 25:
				var start := Time.get_ticks_usec()
				match mode:
					"get_frame": doc.get_frame()
					"set_without_signals", "set_preview_values": doc.set_preview_values({pid:float(i % 21)-10.0})
					"refresh_geometry": preview.refresh_geometry()
					"slider_event": app.parameter_dock._on_row_value_committed(pid,float(i % 21)-10.0)
				cpu.append((Time.get_ticks_usec()-start)/1000.0)
				await process_frame
				wall.append((Time.get_ticks_usec()-start)/1000.0)
			doc.set_block_signals(false)
			report[mode] = {"cpu":stats(cpu),"frame_interval":stats(wall)}
			print(model, " ", mode, " ", JSON.stringify(report[mode]))
		report["final_stats"] = preview.get_render_stats()
		results.append(report)
	var output := FileAccess.open(OS.get_cmdline_user_args()[1],FileAccess.WRITE)
	output.store_string(JSON.stringify(results,"\t"))
	quit()
