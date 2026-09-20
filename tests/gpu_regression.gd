extends SceneTree

var failures = []
var renderer_checks = []

func check_renderer(condition, label):
    renderer_checks.append({"name": label, "passed": condition})
    if not condition:
        failures.append(label)

func draw_frames(count):
    for i in count:
        await process_frame
        await RenderingServer.frame_post_draw

func require_result(result, label):
    if not result.ok:
        failures.append(label + ": " + str(result))
        push_error(failures[-1])

func _initialize():
    run.call_deferred()

func run():
    var files = OS.get_cmdline_user_args()[0]
    ProjectSettings.set_setting("gd_cubism/rendering/batching", false)
    var doc = ClassDB.instantiate("KasaneDocumentBridge")
    var io = ClassDB.instantiate("KasaneProjectIO")
    require_result(io.open_project(doc, (files + "/gpu-source.json")), "open fixture")
    var textures = ClassDB.instantiate("KasaneTextureStore")
    var source = JSON.parse_string(FileAccess.get_file_as_string((files + "/gpu-source.json"))).document
    for asset in source.assets:
        var image = Image.load_from_file(files + "/" + asset.source)
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
    var baseline_stats = preview.get_render_stats()
    var mesh_instances = {}
    for drawable in doc.get_frame().drawables:
        mesh_instances[drawable.id] = preview.get_mesh_view(drawable.id).get_instance_id()
    preview.hide()
    require_result(preview.refresh(), "hidden preview refresh")
    if preview.get_observation_state().ready:
        failures.append("hidden preview reported a ready screenshot")
    preview.show()
    var last_submission = preview.get_last_result().submission_id
    var states = [{"value":0.0,"scale":1.0,"offset":[0,0]},
                  {"value":1.0,"scale":0.9,"offset":[25,20]},
                  {"value":-1.0,"scale":1.05,"offset":[-10,-8]}]
    var roundtrip = ClassDB.instantiate("KasaneDocumentBridge")
    require_result(io.save_project(doc, (files + "/roundtrip.json")), "save")
    require_result(io.open_project(roundtrip, (files + "/roundtrip.json")), "reopen")
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
        var observation = preview.get_observation_state()
        if observation.submission_id <= last_submission:
            failures.append("parameter update did not create a new render submission")
        last_submission = observation.submission_id
        var stats = preview.get_render_stats()
        if stats.creations != baseline_stats.creations or stats.mesh_views != baseline_stats.mesh_views or stats.mask_viewports != baseline_stats.mask_viewports:
            failures.append("fixed-topology refresh rebuilt render resources")
        for mesh_id in mesh_instances:
            if preview.get_mesh_view(mesh_id).get_instance_id() != mesh_instances[mesh_id]:
                failures.append("mesh view identity changed: " + mesh_id)
        require_result(roundtrip.set_preview_values(values), "roundtrip parameter")
        if doc.get_frame().drawables != roundtrip.get_frame().drawables:
            failures.append("source roundtrip differs")
        for frame in 12:
            model.advance(0)
            await process_frame
        await RenderingServer.frame_post_draw
        if not preview.get_observation_state().ready:
            failures.append("preview did not report rendered revision ready")
        for j in 2:
            var image = viewports[j].get_texture().get_image()
            image.save_png("res://%s-%d.png" % ["reference" if j == 0 else "actual", i])
    # Exercise lifecycle and observation on the real GPU, including unchanged revision.
    var viewport = viewports[1]
    var original_submission = preview.get_last_result().submission_id
    viewport.remove_child(preview)
    viewport.add_child(preview)
    check_renderer(not preview.get_observation_state().ready, "reentry invalidates previous viewport completion")
    require_result(preview.refresh(), "reentered preview")
    await draw_frames(3)
    check_renderer(preview.get_observation_state().ready, "reentered preview completes")
    check_renderer(preview.get_last_result().submission_id > original_submission, "same revision uses new submission")

    viewport.render_target_update_mode = SubViewport.UPDATE_DISABLED
    var frozen_pixels = viewport.get_texture().get_image().get_data()
    preview.position += Vector2(30, 0)
    require_result(preview.refresh(), "disabled viewport submission")
    await draw_frames(3)
    check_renderer(not preview.get_observation_state().ready, "disabled viewport cannot complete submission")
    check_renderer(viewport.get_texture().get_image().get_data() == frozen_pixels, "disabled viewport retains old pixels")

    viewport.render_target_update_mode = SubViewport.UPDATE_WHEN_VISIBLE
    await draw_frames(3)
    check_renderer(not preview.get_observation_state().ready, "conditional viewport requires explicit observation draw")

    # This fixture has masks: one render is insufficient, and ONCE becomes DISABLED.
    viewport.render_target_update_mode = SubViewport.UPDATE_ONCE
    await draw_frames(1)
    check_renderer(not preview.get_observation_state().ready, "masked submission waits for second viewport render")
    await draw_frames(2)
    check_renderer(not preview.get_observation_state().ready, "global draws do not consume disabled viewport wait")
    viewport.render_target_update_mode = SubViewport.UPDATE_ONCE
    await draw_frames(1)
    check_renderer(preview.get_observation_state().ready, "two explicit draws complete masked submission")
    viewport.render_target_update_mode = SubViewport.UPDATE_ALWAYS
    preview.position -= Vector2(30, 0)
    require_result(preview.refresh(), "restore preview")
    await draw_frames(3)

    # A refresh must not move opaque model geometry above the selection overlay.
    var clean_pixels = viewport.get_texture().get_image().get_data()
    var overlay = ClassDB.instantiate("KasaneSelectionOverlay")
    preview.add_child(overlay)
    overlay.set_preview(preview)
    overlay.select(doc.get_frame().drawables[0].id)
    overlay.set_show_vertices(true)
    await draw_frames(3)
    var selected_pixels = viewport.get_texture().get_image().get_data()
    check_renderer(selected_pixels != clean_pixels, "selection overlay appears in GPU output")
    require_result(preview.refresh(), "refresh with selection")
    await draw_frames(3)
    check_renderer(viewport.get_texture().get_image().get_data() == selected_pixels, "refresh preserves overlay pixels and ordering")
    overlay.queue_free()
    await draw_frames(3)
    check_renderer(viewport.get_texture().get_image().get_data() == clean_pixels, "removing overlay restores clean model pixels")

    var report = {"status":"passed" if failures.is_empty() else "failed", "failures":failures,
        "renderer_checks": renderer_checks,
        "states":states,"resolution":[640,480],"background":"transparent", "filter":"linear mipmap",
        "renderer":RenderingServer.get_current_rendering_method(), "adapter":RenderingServer.get_video_adapter_name(),
        "godot":Engine.get_version_info(),"reference":"gd-cubism official Core", "actual":"Document / KasaneDocumentPreview"}
    var file = FileAccess.open("res://capture.json", FileAccess.WRITE)
    file.store_string(JSON.stringify(report, "  "))
    file.close()
    quit(0 if failures.is_empty() else 1)
