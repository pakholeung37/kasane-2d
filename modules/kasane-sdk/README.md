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

Editor visibility and protection use `EditorState` and
`EditSession::replace_editor_state`. The sparse map stores explicit
`hidden_in_editor` and `locked` flags for Mesh, Transform, Part, Offscreen and Glue.
Editors resolve Part inheritance and composite visibility; deformation parentage
does not propagate editor flags. SDK model writes ignore editor locks. Metadata
participates in modified state, history budgets, deletion cleanup and undo/redo
without invalidating runtime previews. Writers emit project v10. The old
`ObjectLocks` and `DeformerDisplay` APIs and wire fields are removed, with no
migration adapters.

## Alpha mesh generation

`alpha_mesh_geometry(AlphaMask { width, height, alpha }, &AlphaMeshOptions)`
is a deterministic, CPU-only geometry operation. `alpha` contains one byte per
pixel, in top-down rows; retained pixels above `alpha_threshold` are included.
It returns `MeshGeometry`, without changing any model.

The seven main controls are outside/inside spacing, outside/inside margin,
minimum margin, minimum boundary points and the alpha threshold. `standard()`,
`deformation_small()` and `deformation_large()` provide modeling presets. Their
outside/inside spacing is respectively 85/85, 160/160 and 40/40 pixels, targeting
roughly half the earlier vertex counts while retaining margins, corner coverage
and thin-feature support. The exact reduction depends on shape and size; these
are not equivalent Cubism settings. Custom spacing keeps its literal meaning.
**All distances are source-image pixels.** There is no implicit 1024-pixel,
canvas, UV-atlas or DPI scaling. `preserve_holes` defaults to false: the outer
domain spans source holes, while interior support follows the artwork instead
of filling large transparent voids with lattice vertices. Holes narrower than
the inside spacing receive no separate support ring. Foreground islands inside
large holes still receive support. Set `preserve_holes: true` to retain holes in
the mesh domain. Positive outside margins can merge nearby components and alter
holes. `remove_faint_islands` defaults to false. When enabled it discards isolated
4-connected components whose peak alpha is at most 64 and total alpha is at most
1020 (four fully opaque pixels), only if the image also has alpha >= 128. This
explicit policy retains opaque dots, connected faint edges, larger translucent
regions and fully translucent source images; it is not a global alpha threshold.

The generator unions foreground pixel squares (merging horizontal/vertical
runs), offsets their polygons, and simplifies only when the required padded
foreground is still covered. Failed shortcuts protect nearby raster corners;
other smooth spans keep their simplification tolerance. Offsets are simplified
before clipping to the image bounds, so a tightly cropped diagonal does not
force dense sampling along the entire boundary. Exterior and retained hole
boundaries become CDT constraints. Smooth convex contours are redistributed by
arc length when preserving every simplified segment would substantially oversample
the curve. Required padding, valid topology and minimum ring sizes are checked;
sharp/concave contours and rings with holes retain their existing corners.
Other segments use subdivision allowing at most 10% spacing slack.
Simplified inner offset rings are also constrained, preventing
outer edges from skipping the ring and connecting directly to deep interior
vertices. Face inclusion uses the actual sampled f32 outline, independently of
the inner constraints, so internal rings do not become holes.

Collapsed or very narrow inset components use chordal-axis support chains
derived from the boundary triangulation. Chains are clipped to foreground away
from surviving cores; short spurs are removed. This is a heuristic for thin
features, not an exact medial axis or a reconstruction of Cubism internals.
A staggered triangular lattice fills the cores with clearance from ring edges.
When holes are spanned, every foreground component gets at least one interior
support vertex, including tiny islands that have no surviving inset.
There is no global angle refinement that could disturb the lattice. Boundary
and interior spacing remain targets; corners and support chains can add points.
There is no guaranteed minimum triangle angle or guarantee of deformation
quality. Pixel alpha is never modified; alpha thresholding is explicit.

Output coordinates use the bottom-left image corner as `(0, 0)`, +Y up; UVs are
`(x / width, y / height)`. `clip_to_image` defaults to true, stopping margins at
image edges and keeping UVs in `[0, 1]`. Set it to false to keep complete margins
outside the source rectangle. Those UVs can leave `[0, 1]`; callers must add
transparent texture padding and remap UVs before rendering. Retain the image
pixels and their pixel-to-model transform to regenerate
after editing; do not derive a new source mask from the previous mesh.
Triangles are CCW and use local sequential vertex IDs. The generator does not
transfer keyforms, BlendShapes or glue: applying its output to existing artwork
requires an explicit `TopologyReplacement` and dependency policy.

Empty masks, invalid options, geometry failures, and exceeded budgets return
structured `SdkError`s (`EMPTY_ALPHA_MASK`, `INVALID_ALPHA_MASK`,
`INVALID_ALPHA_MESH_OPTIONS`, `ALPHA_MESH_GEOMETRY`, `ALPHA_MESH_LIMIT`). No partial
geometry is returned. Defaults cap output at 65536 vertices; preprocessing is
also bounded (16384 per image axis, 64 Mi pixels, 262144 merged rectangles,
1048576 sampling and support checks). Highly fragmented masks may reach those limits.

```sh
cargo test -p kasane-sdk --test alpha_mesh --locked
```

Tests cover whole-pixel coverage, winding, closed boundaries, UV orientation,
holes, disconnected islands, tiny/diagonal features, thresholding, density,
limits and repeatability.
