# kasane-sdk

Engine-independent Rust authoring SDK for session editing, history, project
operations, and CPU geometry queries.

`resolve_mesh_ids`, `object_bounds`, and `hit_test_geometry` operate on one
evaluated `DrawableFrame` plus its frozen `Mesh` and `Part` records. They do not
need a renderer or textures. The observer's Python binding calls them on a
captured scene; the analysis-packet reader reuses them after reopening saved
geometry, including in wheels built without GPU observation.
