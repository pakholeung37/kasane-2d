extends SceneTree

func _initialize():
    call_deferred("run")

func run():
    var args = OS.get_cmdline_user_args()
    var doc = ClassDB.instantiate("KasaneDocumentBridge")
    var io = ClassDB.instantiate("KasaneProjectIO")
    var opened = io.open_project(doc,args[0])
    if not opened.ok or not opened.resources_complete:
        push_error(str(opened)); quit(1); return
    var textures = ClassDB.instantiate("KasaneTextureStore")
    var viewport = SubViewport.new()
    viewport.size = Vector2i(640,480)
    viewport.transparent_bg = true
    viewport.render_target_update_mode = SubViewport.UPDATE_ALWAYS
    root.add_child(viewport)
    var preview = ClassDB.instantiate("KasaneDocumentPreview")
    viewport.add_child(preview)
    preview.set_texture_store(textures)
    preview.set_document(doc)
    var states = [{"value":-1.0,"scale":2.0,"offset":[160,120]},
                  {"value":0.0,"scale":2.5,"offset":[150,100]},
                  {"value":1.0,"scale":1.8,"offset":[170,130]}]
    for i in states.size():
        var state = states[i]
        doc.set_preview_values({"11111111-1111-4111-8111-000000000010":state.value,
            "11111111-1111-4111-8111-000000000011":state.value,
            "11111111-1111-4111-8111-000000000012":state.value})
        preview.position = Vector2(state.offset[0],state.offset[1])
        preview.scale = Vector2.ONE*state.scale
        if not preview.refresh().ok:
            push_error(str(preview.get_last_result())); quit(1); return
        for frame in 12: await process_frame
        await RenderingServer.frame_post_draw
        viewport.get_texture().get_image().save_png(args[1]+"-%d.png" % i)
    var file = FileAccess.open(args[1]+".json",FileAccess.WRITE)
    file.store_string(JSON.stringify({"states":states,"resolution":[640,480],"renderer":RenderingServer.get_current_rendering_method(),
        "adapter":RenderingServer.get_video_adapter_name(),"background":"transparent","filter":"linear mipmap"}))
    file.close()
    quit(0)
