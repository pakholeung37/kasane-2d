# kasane-animation

Deterministic CPU animation state and evaluation for motions, expressions, poses, and physics.

`MotionPreview::evaluate_drawables` keeps a per-preview geometry workspace and
reuses parameter UUID keys and evaluator scratch buffers. Returned frames remain
independently owned; repeated evaluation does not advance playback. Concurrent
reads of one preview serialize access to scratch storage, while different
previews have separate workspaces. Seeking may replace the workspace without
changing simulation semantics. Hidden-geometry observation retains its existing
separate evaluation path.

Motion and physics updates reuse existing parameter-map nodes when the key set
is unchanged. A changed key set replaces the map, removing obsolete entries.
Core geometry evaluation samples each unique binding-axis grid once per frame
using prepared parameter slots, and applies parent deformers to vertex batches.
Prepared grids are invalidated with document structure/parameter/binding edits;
vertex-only edits continue to read current keyforms.
