# kasane-sdk

Engine-independent Rust authoring SDK for session editing, history, project
operations, and CPU geometry queries.

`resolve_mesh_ids`, `object_bounds`, and `hit_test_geometry` operate on one
evaluated `DrawableFrame` plus its frozen `Mesh` and `Part` records. They do not
need a renderer or textures. The observer's Python binding calls them on a
captured scene; the analysis-packet reader reuses them after reopening saved
geometry, including in wheels built without GPU observation.

Palette sorting uses `EditSession::replace_hierarchy_order(HierarchyOrder)`.
`Document::hierarchy_order()` exposes independent organization and deformation
ID ranks. These metadata edits participate in checkpoints, history budgets and
project IO, preserve evaluation revisions, and are removed when their objects
are erased. Version 6 projects may contain an optional `hierarchy_order` field;
empty ordering is omitted and existing projects retain source ordering.

Parameter and parameter group display order use the ordered CDI collections in
`DisplayInfo`. Replacing those collections preserves runtime parameter storage
order and uses the normal validated metadata transaction.
The editor materializes missing resolved CDI entries when sorting its complete
visible parameter list, while retaining unresolved entries and extensions.

Editor protection metadata uses `ObjectLocks` and `EditSession::replace_object_locks`.
Only explicit Mesh, Transform, Part and Offscreen IDs are stored; the editor derives
inheritance independently along organization and deformation chains. SDK model
writes intentionally ignore editor locks. Lock changes participate in modified
state, history budgets, deletion cleanup and undo/redo without invalidating visual
previews. Current writers save project v7; v1–v6 projects load without locks.
