extends SceneTree

var failures: Array[String] = []
var checks := 0
var doc
var preview
var viewport: SubViewport
var output: String
var samples: Array = []

func id(n: int) -> String:
	return "%08x-1111-4111-8111-111111111111" % n

func check(condition: bool, label: String) -> void:
	checks += 1
	if not condition:
		failures.append(label)
		push_error(label)

func ok(result: Dictionary, label: String) -> void:
	check(result.get("ok", false), label + ": " + str(result))

func part(n: int, parent: int, order: float, enabled := true) -> Dictionary:
	return {"id":id(n),"runtime_id":"Part%d"%n,"name":"Part%d"%n,"parent_id":id(parent) if parent else "","enabled":enabled,"draw_order":order}

func surface(n: int, owner: int) -> Dictionary:
	return {"id":id(n),"runtime_id":"Surface%d"%n,"name":"Surface%d"%n,"part_id":id(owner),"blend_mode":0,"flags":4,"masks":[],"part_keyform_indices":[],"keyforms":[{"opacity":1.0,"multiply":[1,1,1],"screen":[0,0,0]}]}

func mesh(n: int, owner: int, rect: Rect2, color: Color, order: float, empty := false) -> void:
	var points := PackedVector2Array([rect.position, Vector2(rect.end.x,rect.position.y), rect.end, Vector2(rect.position.x,rect.end.y)])
	ok(doc.create_mesh({"id":id(n),"runtime_id":"Mesh%d"%n,"name":"Mesh%d"%n,"texture_asset_id":id(2),
		"vertex_ids":PackedInt64Array([1,2,3,4]),"base_positions":points,"uvs":PackedVector2Array([Vector2.ZERO,Vector2.RIGHT,Vector2.ONE,Vector2.DOWN]),
		"triangles":PackedInt64Array([] if empty else [1,2,3,1,3,4]),
		"properties":{"part_id":id(owner),"deformer_id":"","blend_mode":0,"enabled":true,"double_sided":true,"inverted_mask":false,"draw_order":order,"masks":[],
		"appearance":{"opacity":color.a,"multiply":[color.r,color.g,color.b],"screen":[0,0,0]}}}),"create mesh")

func draw() -> Image:
	for unused in 4:
		await process_frame
		await RenderingServer.frame_post_draw
	samples.append(preview.get_render_stats())
	return viewport.get_texture().get_image()

func pixel(image: Image, point: Vector2i, expected: Color, label: String) -> void:
	var value := image.get_pixelv(point)
	check(maxf(absf(value.r-expected.r),maxf(absf(value.g-expected.g),maxf(absf(value.b-expected.b),absf(value.a-expected.a))))<0.025,label+": "+str(value))

func _initialize() -> void:
	run.call_deferred()

func run() -> void:
	output=OS.get_cmdline_user_args()[0]
	DirAccess.make_dir_recursive_absolute(output)
	doc=ClassDB.instantiate("KasaneDocumentBridge")
	ok(doc.initialize(id(1),Vector2(128,128),Vector2(64,64),100),"initialize")
	var white:=Image.create(1,1,false,Image.FORMAT_RGBA8)
	white.fill(Color.WHITE)
	white.save_png(output.path_join("white.png"))
	ok(doc.add_image_asset(id(2),"white",output.path_join("white.png"),1,1),"asset")
	for p in [part(10,0,0),part(11,10,1),part(12,10,2),part(13,10,3)]:ok(doc.write_part(p),"part")
	for s in [surface(20,10),surface(21,11),surface(22,12),surface(23,13)]:ok(doc.write_offscreen(s),"surface")
	mesh(30,10,Rect2(16,16,96,96),Color.RED,0)
	mesh(31,11,Rect2(16,16,48,96),Color.BLUE,0)
	mesh(32,12,Rect2(64,16,48,96),Color.GREEN,0)
	mesh(33,10,Rect2(16,16,32,96),Color(1,1,1,0),-1)
	mesh(34,10,Rect2(16,16,32,96),Color(1,1,1,0),-2,true)
	var textures=ClassDB.instantiate("KasaneTextureStore")
	ok(textures.set_texture(id(2),ImageTexture.create_from_image(white)),"texture")
	viewport=SubViewport.new();viewport.size=Vector2i(128,128);viewport.transparent_bg=true;viewport.disable_3d=true;viewport.render_target_update_mode=SubViewport.UPDATE_ALWAYS;root.add_child(viewport)
	preview=ClassDB.instantiate("KasaneDocumentPreview");viewport.add_child(preview);preview.set_texture_store(textures);preview.set_document(doc)
	ok(preview.get_last_result(),"first frame")
	var baseline:Image=await draw()
	pixel(baseline,Vector2i(32,64),Color.BLUE,"nested child blue")
	pixel(baseline,Vector2i(96,64),Color.GREEN,"nested child green")
	check(preview.get_offscreen_texture(id(23)).get_size()==Vector2(2,2),"empty surface releases full allocation")
	var nodes_before:Dictionary=preview.get_render_stats()
	var s:Dictionary=doc.get_offscreen_snapshot(id(21))
	s.masks=[id(33)];ok(doc.write_offscreen(s,true),"offscreen mask")
	var image:Image=await draw()
	pixel(image,Vector2i(32,64),Color.BLUE,"normal mask inside")
	pixel(image,Vector2i(56,64),Color.RED,"normal mask outside")
	preview.scale=Vector2(0.5,0.5);preview.position=Vector2(8.25,0.75);preview.rotation=0.2
	image=await draw()
	pixel(image,Vector2i(preview.transform*Vector2(32,64)),Color.BLUE,"scaled rotated offscreen mask inside")
	pixel(image,Vector2i(preview.transform*Vector2(56,64)),Color.RED,"scaled rotated offscreen mask outside")
	preview.scale=Vector2.ONE;preview.position=Vector2.ZERO;preview.rotation=0
	var mask_geometry:Dictionary=doc.get_mesh_snapshot(id(33))
	var mask_faces=mask_geometry.triangles
	mask_geometry.triangles=PackedInt64Array()
	ok(doc.replace_mesh(mask_geometry),"remove existing mask faces")
	image=await draw();pixel(image,Vector2i(32,64),Color.RED,"removed faces clear cached mask geometry")
	mask_geometry.triangles=mask_faces
	ok(doc.replace_mesh(mask_geometry),"restore existing mask faces")
	image=await draw();pixel(image,Vector2i(32,64),Color.BLUE,"restored mask faces render again")
	s.flags=12;ok(doc.write_offscreen(s,true),"inverse offscreen mask");image=await draw()
	pixel(image,Vector2i(32,64),Color.RED,"inverse mask inside")
	pixel(image,Vector2i(56,64),Color.BLUE,"inverse mask outside")
	s.flags=4;s.masks=[id(34)];ok(doc.write_offscreen(s,true),"zero triangle mask");image=await draw()
	pixel(image,Vector2i(32,64),Color.RED,"zero triangle mask gives zero coverage")
	s.flags=12;ok(doc.write_offscreen(s,true),"inverse zero triangle mask");image=await draw()
	pixel(image,Vector2i(32,64),Color.BLUE,"inverse zero triangle mask gives full coverage")
	s.flags=4;s.masks=[];ok(doc.write_offscreen(s,true),"restore offscreen mask")
	var props:Dictionary=doc.get_mesh_snapshot(id(31)).properties
	props.masks=[id(33)];props.raw_blend_mode=3;props.appearance.opacity=0.5;ok(doc.set_mesh_properties(id(31),props),"extended mesh masked opacity")
	image=await draw();pixel(image,Vector2i(32,64),Color(0.5,0,0.5,1),"mesh alpha only modulation")
	pixel(image,Vector2i(56,64),Color.RED,"mesh mask outside")
	mask_geometry.triangles=PackedInt64Array()
	ok(doc.replace_mesh(mask_geometry),"remove mesh mask faces")
	image=await draw();pixel(image,Vector2i(32,64),Color.RED,"mesh mask clears cached geometry")
	mask_geometry.triangles=mask_faces
	ok(doc.replace_mesh(mask_geometry),"restore mesh mask faces")
	image=await draw();pixel(image,Vector2i(32,64),Color(0.5,0,0.5,1),"mesh mask recovers geometry")
	props.masks=[];props.raw_blend_mode=null;props.appearance.opacity=1;ok(doc.set_mesh_properties(id(31),props),"restore mesh")
	ok(doc.write_part(part(11,10,1,false),true),"disable subtree");image=await draw()
	pixel(image,Vector2i(32,64),Color.RED,"disabled subtree absent")
	check(preview.get_offscreen_texture(id(21)).get_size()==Vector2(2,2),"disabled subtree releases allocation")
	ok(doc.write_part(part(11,10,1),true),"reenable subtree");image=await draw()
	check(image.get_data()==baseline.get_data(),"disable-reenable restores pixels")
	# Reparent an overlapping child; moving its owner order must change the plan.
	var green:Dictionary=doc.get_mesh_snapshot(id(32))
	var shifted:=PackedVector2Array([Vector2(16,16),Vector2(64,16),Vector2(64,112),Vector2(16,112)])
	ok(doc.set_vertex_positions(id(32),green.vertex_ids,shifted),"overlap siblings")
	image=await draw();pixel(image,Vector2i(32,64),Color.GREEN,"higher order wins")
	ok(doc.write_part(part(11,10,4),true),"change draw order");image=await draw();pixel(image,Vector2i(32,64),Color.BLUE,"dynamic order wins")
	ok(doc.write_part(part(11,12,4),true),"reparent offscreen owner");image=await draw();pixel(image,Vector2i(32,64),Color.BLUE,"reparented child still composes")
	ok(doc.write_part(part(11,10,1),true),"restore owner")
	ok(doc.set_vertex_positions(id(32),green.vertex_ids,green.base_positions),"restore green vertices")
	image=await draw();check(image.get_data()==baseline.get_data(),"hierarchy A-B-A")
	# Delete a live surface while its meshes survive; queued deletion must not
	# accidentally destroy the reparented MeshView handles on the next frame.
	ok(doc.erase_object(id(21)),"remove offscreen");image=await draw();pixel(image,Vector2i(32,64),Color.BLUE,"removed surface keeps mesh")
	ok(doc.write_offscreen(surface(21,11)),"recreate surface");image=await draw();check(image.get_data()==baseline.get_data(),"surface remove-recreate")
	for index in 20:
		ok(preview.refresh_geometry(),"stable refresh")
		await draw()
	var tail:Array=samples.slice(-20)
	for stats in tail:
		check(stats.offscreen_creations==tail[0].offscreen_creations and stats.offscreen_resizes==tail[0].offscreen_resizes and stats.gpu_texture_bytes==tail[0].gpu_texture_bytes,"stable GPU allocation")
	preview.position=Vector2(0.25,0.75);preview.rotation=0.2;await draw()
	preview.position=Vector2.ZERO;preview.rotation=0;image=await draw();check(image.get_data()==baseline.get_data(),"camera transform A-B-A")
	var other:=SubViewport.new();other.size=Vector2i(128,128);other.transparent_bg=true;other.render_target_update_mode=SubViewport.UPDATE_ALWAYS;root.add_child(other)
	preview.reparent(other,false);await draw()
	check(other.get_texture().get_image().get_data()==baseline.get_data(),"cross viewport reparent")
	preview.reparent(viewport,false);image=await draw();check(image.get_data()==baseline.get_data(),"preview viewport A-B-A")
	image.save_png(output.path_join("lifecycle.png"))
	var report:={"status":"passed" if failures.is_empty() else "failed","checks":checks,"failures":failures,"samples":samples,"initial_resources":nodes_before}
	var file:=FileAccess.open(output.path_join("report.json"),FileAccess.WRITE);file.store_string(JSON.stringify(report,"  "));file.close()
	quit(0 if failures.is_empty() else 1)
