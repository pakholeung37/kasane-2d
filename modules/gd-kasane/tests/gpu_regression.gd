extends SceneTree

var failures = []
func require_result(result, label):
    if not result.ok:
        failures.append(label + ": " + str(result))
        push_error(failures[-1])

func _initialize():
    run.call_deferred()

func run():
    ProjectSettings.set_setting("gd_cubism/rendering/batching", false)
    var doc = ClassDB.instantiate("KasaneDocumentBridge")
    var io = ClassDB.instantiate("KasaneProjectIO")
    require_result(io.open_project(doc, "res://gpu-source.json"), "open fixture")
    var textures = ClassDB.instantiate("KasaneTextureStore")
    var source = JSON.parse_string(FileAccess.get_file_as_string("res://gpu-source.json")).document
    for asset in source.assets:
        var image = Image.load_from_file("res://package/" + asset.source)
        image.generate_mipmaps()
        require_result(textures.set_texture(asset.id, ImageTexture.create_from_image(image)), "texture")
    var viewports = []
    for i in 2:
        var viewport = SubViewport.new()
        viewport.size = Vector2i(640, 480)
        viewport.disable_3d = true
        viewport.transparent_bg = true
        viewport.render_target_update_mode = SubViewport.UPDATE_ALWAYS
        root.add_child(viewport)
        viewports.append(viewport)
    var model = ClassDB.instantiate("GDCubismUserModel")
    viewports[0].add_child(model)
    model.assets = "res://package/model.model3.json"
    model.physics_evaluate = false
    model.pose_update = false
    model.playback_process_mode = 2 # MANUAL
    var preview = ClassDB.instantiate("KasaneDocumentPreview")
    viewports[1].add_child(preview)
    preview.set_texture_store(textures)
    preview.set_document(doc)
    require_result(preview.get_last_result(), "preview")
    var states = [{"value":0.0,"scale":1.0,"offset":[0,0]},
                  {"value":1.0,"scale":0.9,"offset":[25,20]},
                  {"value":-1.0,"scale":1.05,"offset":[-10,-8]}]
    var roundtrip = ClassDB.instantiate("KasaneDocumentBridge")
    require_result(io.save_project(doc, "res://roundtrip.json"), "save")
    require_result(io.open_project(roundtrip, "res://roundtrip.json"), "reopen")
    for i in states.size():
        var state = states[i]
        var offset = Vector2(state.offset[0], state.offset[1])
        model.scale = Vector2.ONE * state.scale
        model.position = offset + Vector2(271,193) * state.scale
        preview.scale = Vector2.ONE * state.scale
        preview.position = offset
        for param in model.get_parameters():
            param.value = state.value
            param.hold = true
        var values = {source.parameters[0].id:state.value}
        require_result(doc.set_preview_values(values), "parameter")
        require_result(roundtrip.set_preview_values(values), "roundtrip parameter")
        if doc.get_frame().drawables != roundtrip.get_frame().drawables:
            failures.append("source roundtrip differs")
        for frame in 12:
            model.advance(0)
            await process_frame
        await RenderingServer.frame_post_draw
        for j in 2:
            var image = viewports[j].get_texture().get_image()
            image.save_png("res://%s-%d.png" % ["reference" if j == 0 else "actual", i])
    var report = {"status":"passed" if failures.is_empty() else "failed", "failures":failures,
        "states":states,"resolution":[640,480],"background":"transparent", "filter":"linear mipmap",
        "renderer":RenderingServer.get_current_rendering_method(), "adapter":RenderingServer.get_video_adapter_name(),
        "godot":Engine.get_version_info(),"reference":"gd-cubism official Core", "actual":"Document / KasaneDocumentPreview"}
    var file = FileAccess.open("res://capture.json", FileAccess.WRITE)
    file.store_string(JSON.stringify(report, "  "))
    file.close()
    quit(0 if failures.is_empty() else 1)
