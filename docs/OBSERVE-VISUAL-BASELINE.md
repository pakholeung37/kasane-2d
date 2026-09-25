# Observe visual O0 baseline

Date: 2026-09-25. Source HEAD before implementation:
`768f129506baf357c81fef9e98cb6a0099cbb994`. The working tree already
had user edits to the visual plan/research files; these are excluded from the
baseline claim. The raw measurements below used the installed `kasane 0.1.0`
wheel **before rebuilding it with the new Rust ROI code**. Machine:
macOS 27 arm64, Apple M4 / Metal. The fixture manifest SHA-256 was
`a971cfc4a93a55c3a335ae35df8f425968f4e5490dbe3ce1c333a0eacf530331`.
The installed native binary SHA-256 was
`85689b6a7f1dfbbbb1dc201bdac9b2d87b5eb8e783e4f4ed32290a98aca3cda6`.

## Reproduce

```sh
uv run --locked python tests/fixtures/observe_inspection/build_fixtures.py --replace
uv run --locked python tests/fixtures/observe_inspection/build_fixtures.py --verify
uv run --locked python tools/run_observe_visual_baseline.py \
  --output "$PWD/target/observe-visual-baseline.json"
cargo test -p kasane-sdk-observe --locked
```

`build_fixtures.py` uses only locally generated PNG pixels and public authoring
calls. The checked-in `manifest.json` lists SHA-256 for every generated
project/asset. `pixel-binding` has transparent padding, a one-pixel line,
duplicated display names and a two-keyform mesh binding. `rotated-warp` has a
rotated parent and compressed 1×1 warp in parent-local coordinates.
`masked-offscreen` has overlapping meshes, a mask source and a half-opacity
Offscreen. The fixture content is CC0-1.0. Regeneration is explicit; it
refuses to overwrite projects without `--replace`.

## Existing raw output

The observer rendered at 128×128, `fit_long_side=128`, transparent target,
RGBA8Unorm, without gamma conversion or alpha post-processing. Each case was
run three times through one observer; elapsed figures are the median of the
three in-process calls, including capture, texture resolution, render and
readback. They are descriptive measurements, not release thresholds.

| Case | Raw SHA-256 | Nonzero alpha pixels | Partial alpha pixels | Median ms |
| --- | --- | ---: | ---: | ---: |
| pixel-neutral | `20a45660ca1e7c64013d98310e660d9320b87dc4379fa16ae9c6b69d4b2b8684` | 896 | 412 | 1.948 |
| pixel-quarter | `e304c3e5e097989624a3a58f8a56e82116fc16b151d44266ec33fd7eea512464` | 897 | 391 | 2.056 |
| pixel-full | `69655b342334700518bf6ab381f46b86d47d709ef2b5c3ccf91ef48b1e0475e1` | 897 | 391 | 2.041 |
| rotated-warp | `1f81ad7ef977ff93f4e08808e188d9a7f84791b6f31bcaeb92bac80315902d46` | 133 | 133 | 1.779 |
| masked-offscreen | `7a12c63496544a40b5a0295729bb59629a9b952dfad33ff581b33401e171c6b3` | 1932 | 696 | 1.869 |

Peak process RSS after these cases was 49,692,672 bytes, measured with
`resource.getrusage`; it is whole-process RSS, not isolated Observe memory.
The complete JSON record, including PNG hashes, sampled centre pixels, all
three timings and adapter metadata, is regenerated at
`target/observe-visual-baseline.json` and is intentionally not a portable
golden file. Hashes above are evidence from this adapter/build only.
The newly built observe wheel was also run against the checked-in fixtures;
all five raw hashes and nonzero-alpha counts matched the installed baseline.
Its native binary SHA-256 was
`4ba789a781d18c97047e33fea54f1c69669d3ddf6be686c69a9a628bf79c2aa1`.

The `pixel-quarter` centre raw RGBA was `[59,8,20,65]`. Mathematical
source-over onto opaque white would yield `[249,198,210,255]` in the same
numeric space if that raw sample is premultiplied. Dark RGB 32 would yield
`[83,32,44,255]`. The runner records both with half-up integer rounding.
This is only a postcompose
arithmetic reference. The existing observer cannot initialize a light/dark/
checker main scene background, so renderer-native background measurements are
**not run**. They must be measured after V01 changes, especially for additive,
multiply, destination-read and nested Offscreen. No result here proves a
portable straight-alpha PNG. The raw baseline found zero pixels with nonzero
RGB and zero alpha in these three fixtures; that does not rule such pixels out
for other blend modes.

## Existing test and animation scope

At baseline, `cargo test -p kasane-sdk-observe --locked` passed 3 integration
tests in `tests/capture.rs` in 0.83 s test execution time. The tests cover
frozen evaluated frame values after a session edit, animation frame capture
with stale-preview rejection, and GPU reuse across edits/texture changes.
They do not verify frozen texture bytes across multiple views, explicit ROI,
background composition, save/open, pixel query or animation trace.

Existing `tools/run_animation_gpu_probe.py` renders 18 selected Motion/Pose
frames across plain/single/nested Offscreen with Model opacity on/off, comparing
Kasane and Framework images. It uses supplied CPU animation samples; it does
not implement the planned independent `PlaybackSpec`, interval event journal,
stage trace, or temporal report. It was **not run** in this O0 baseline. The
current Rust `ObservationInput::capture_motion*` already captures the actual
evaluated animation frame with same-session/revision checks. Python `Observer`
does not yet expose that path.

## Edit addressing and minimal task

The first grounding task uses `pixel-binding` mesh
`10000000-0000-4000-8000-000000000015`, binding
`10000000-0000-4000-8000-000000000029`, and parameter
`10000000-0000-4000-8000-00000000001f` (`Shift`). At value 0.25, the two
keyforms have expected interpolation weights 0.75 and 0.25. The target is to
move the right keyform's selected vertex by a known small canvas delta via
`Edit.set_mesh_keyform(binding_id, MeshKeyform(...))`, then capture the same
parameter and view to check the weighted target change and unchanged second
mesh. Updating source geometry through `Edit.update_positions(mesh_id, ...)`
is a separate operation and must not be suggested as an equivalent fix.

The current API can address both edit methods but does not expose the selected
keyform/weights in an Observe result. The O2 test must obtain that provenance
from the new packet rather than hard-coding the values in this document. It
must open a saved frozen scene in another process, query an actual triangle,
edit through the public API, and compare target/non-target regions under the
original view. This task definition does not mutate the checked-in fixture.

## Stage ledger

| Item | Status | Evidence / limitation |
| --- | --- | --- |
| Contract, pixel and coordinate semantics | complete for O0 | `OBSERVE-VISUAL-CONTRACT.md`; implementation remains staged |
| Self-authored inspection fixtures | complete for O0 | generated projects/assets and `manifest.json` |
| Legacy raw and resource baseline | complete for O0 | command and measured values above |
| Renderer-native backgrounds | not_run | no explicit main target background entry yet |
| Animation recipe/time baseline | documented existing scope | direct capture exists; runner/journal absent |
| O1 immutable textures and explicit ROI | in_progress | Rust slice; save/open and Python binding pending |
