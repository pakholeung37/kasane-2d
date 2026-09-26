# Observe visual implementation log

2026-09-25. Implementation started from base commit
`768f129506baf357c81fef9e98cb6a0099cbb994`. The user's preexisting
staged research/plan changes were preserved. Fixture manifest SHA-256:
`a971cfc4a93a55c3a335ae35df8f425968f4e5490dbe3ce1c333a0eacf530331`.

| Stage | Status | Delivered here | Remaining gate |
| --- | --- | --- | --- |
| O0 contract and baseline | complete | Typed target contract, three generated/verified synthetic projects, raw pixel/timing/RSS baseline, derived light/dark arithmetic reference, edit task definition | Renderer-native background results belong to V01 implementation |
| O1 frozen capture and ROI | complete | Detached document snapshot; 1–64 samples with one texture union decode; full frozen authoring source records; static/actual animation frame capture with last successful operation identity; explicit ROI; scene and raw packet round trips; report/analysis/scene payload profiles; canonical capture/scene/render IDs; independent-process reopen and legacy pixel regression | O1 acceptance gate passed. Full playback recipe/runner, query-capable analysis profile, presentation, complete report v2 and full `InspectionRequest` belong to O2/O3 |
| O2 display and location | complete | Main-target light/dark/checker background, conservative straight alpha and raw-derived alpha view, mesh/Part focus, fixed-union/follow sample views, 3×3/runtime mappings, numeric marks/highlights, object table, omitted-label reasons, packet profile round trips | V01/V02/V03 gates passed with the isolated GPU wheel; comparison/report and geometry/query channels belong to O3–O6 |
| O3–O7 | not started | — | V04–V07 comparison, geometry, diagnostic composition and query; joint acceptance and playback work |

The Python O1 API includes `capture_scene`, `capture_scenes`,
`capture_animation_scene`, `render_scene`, `save_scene`, `open_scene`,
`inspect`, `inspect_animation`, `render(packet)`, `packet.save`, and
`open_inspection_packet`; it is documented in
`modules/kasane-python/API.md`. The saved scene has no resumable animation
runtime state. `CapturedScene.source` records the current animation snapshot,
host Model opacity policy, last successful operation identity, and
`history_status=not_recorded`; events in that
snapshot cover only the last update. `CapturedScene.authoring` is frozen
metadata and source topology, not selected-keyform provenance. Packet profile
capabilities explicitly report when GPU rerender, raw bytes and evaluated
geometry are present. CPU geometry/coverage query is not yet implemented.

## Verification record

| Command/evidence | Result |
| --- | --- |
| `build_fixtures.py --verify` with final GPU wheel | passed, 9 files and 3 projects |
| `cargo test -p kasane-sdk-observe --locked` | passed: 4 unit + 6 integration tests; multi-sample snapshot, canonical digest normalization/nonfinite rejection, cross-process scene reopen, corruption/version/path rejection |
| `cargo test --workspace --locked` | passed after O1 capture/operation work |
| `cargo test -p kasane-moc3-psd --locked` | passed after those test lint edits |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | passed after O1 capture/operation work |
| `cargo fmt --check`, `git diff --check` | passed |
| release GPU wheel, isolated CPython 3.14 environment | passed 8 `test_observe.py` tests, including Pose opacity, batch name ambiguity, profile capability gates, packet corruption and independent-process scene packet reopen |
| release CPU-only wheel, isolated CPython 3.14 environment | passed 51 `test_cpu.py` tests; separately opened report/analysis profiles without GPU |
| raw baseline on final GPU wheel | all five self-authored case raw and legacy input hashes matched the pre-change wheel on Apple M4/Metal |

Baseline values and reproducible commands are in
`docs/OBSERVE-VISUAL-BASELINE.md`; contract and capability limits are in
`docs/OBSERVE-VISUAL-CONTRACT.md`. The original `observe()` return tuple,
`observe_run()` report v1 and raw pixel hashes were preserved. The ROI path
still emits a provisional legacy `input_sha256` extension for compatibility;
the separate canonical `scene_digest` and `render_digest` are versioned input
identifiers. They do not promise byte-identical output across GPU adapters.
The scene bundle writer now emits schema v2 with a persisted capture ID and
validated scene digest; the reader also accepts v1 and assigns it a new ID.
The raw packet manifest is schema 2 and only claims the channels currently
captured. Analysis reopening preserves raw bytes and evaluated geometry for O2;
it does not yet expose geometry/PixelProbe queries. Animation operation identity
distinguishes reset/advance(0)/seek(0), but the unrecorded live history is not
a replay recipe or resumable runtime state. Operation identity is excluded from
the scene digest: separate previews of an identical frozen scene have the same
scene digest and distinct capture IDs.

## Mao real-model integration

`tools/run_mao_observe_o1_integration.py` exercises the optional local Mao
model at `models/local/mao/runtime/mao_pro.model3.json`. It imports 260
drawables, captures one frozen scene, renders the full 5800×8400 canvas and a
head ROI `(1300, 500, 4500, 3100)` at 1280×1040, and checks ROI coordinate
mapping. It saves both a scene bundle and a scene-profile inspection packet,
then requires exact RGBA equality after reopening each. The packet is also
reopened and rerendered in a separate Python process. On Apple M4/Metal, the
head PNG SHA-256 was
`aba4b82868e0c90bfded1eb16fc562d24eb07c7ee24b384f1f562e0a84c32b80`.
The script writes a new output directory under `target/mao-observe-o1` for
each run. This local model is not part of the checked-in fixture suite.

## Accepted default texture filtering (2026-09-26)

After visual review, the default Observe GPU profile follows the local Cubism
Native OpenGL sample's source texture settings: generated mipmaps, linear
pixel/mip-level filtering, repeating UVs, no anisotropic filtering, and one
framebuffer sample. It does not add geometric edge antialiasing. Mao renders
from the accepted build are under `target/mao-observe-o1/run-df37da6db111`;
the 1024×1024 full PNG SHA-256 is
`a748ab78f964bdde4c84fb33606157f4260b6d3f0ab008003bc9cb1064d81133`
and the 1280×1040 head PNG SHA-256 is
`11ac1cdd4bfd39474f09bc23c251e11dd95e177dc0cb017570d0b020c19c67b1`.
Saved scene and packet replay remained pixel-identical, including a separate
Python process.

The O0 raw measurements remain historical. The new default profile's
five-case measurement is in `docs/OBSERVE-VISUAL-BASELINE.md`; four raw hashes
changed, while `masked-offscreen` stayed identical. A no-default-features GPU
wheel reproduced all five O0 raw hashes. The rectangle grid remesh test now
checks both any changed pixels (at most 128 of 65,536) and changes greater than
one channel level (at most two). On Apple M4/Metal, the accepted profile had
112 one-level differences at parameter 0.5, and 111 changed pixels including
one larger edge difference at parameter 1. All eight `test_observe.py` cases,
targeted Rust tests and targeted Clippy passed with the accepted default.

## O2 display and location (2026-09-26)

The new `InspectionRequest` supports context `clean`, `labels`, and `alpha`
views. An opaque light/dark color or checkerboard initializes the actual main
scene target before destination reads; mask and nested Offscreen targets remain
transparent. An independent transparent raw pass supplies the alpha view.
Normal-blend transparent presentation uses round-half-up unpremultiplication
and zeros RGB at alpha=0. Additive, multiply, extended drawable and special
Offscreen blends return `UNREPRESENTABLE_TRANSPARENT_OUTPUT` for straight
alpha rather than silently saving an invalid PNG. The old raw entry and its
hashes are unchanged.

Mesh/Part focus resolves against frozen authoring and evaluated geometry.
`inspect_samples` shares one capture and texture union; fixed-union uses one
ROI across samples, while follow uses each sample's own ROI. Views expose
source-canvas/image matrices, runtime/canvas mapping, content rectangle,
requested/padded/visible ROIs, and explicit `focus_status`. The object table
stores stable UUID-sorted marks, Part paths, deformer parent, render path,
topology hash, evaluated bounds and unknown coverage status. The label view
keeps clean pixels separate, draws numeric labels and selected geometry boxes,
and records omitted labels. Its 3×5 bitmap digits use only the standard
library, so the CPU wheel gains no presentation dependency.

| O2 evidence | Result |
| --- | --- |
| `cargo test --workspace --locked -q` and `cargo clippy --workspace --all-targets --locked -- -D warnings` | passed, including main-target extended destination-read and transparent blend rejection tests |
| release GPU wheel, isolated CPython 3.14 | 16 `test_observe.py` cases passed: masked/nested background values, straight-alpha rounding, special-blend rejection/recovery, rotated focus, subpixel/overscan mapping, high-resolution rerender, fixed/follow samples, 13-object dense labels, empty/hidden focus, profile and independent-process reopen |
| release CPU wheel, isolated CPython 3.14 | 52 `test_cpu.py` cases passed; CPU-only wheel reopened O2 report/analysis profiles, and correctly rejected scene-profile GPU rerender |
| default raw baseline | all five current Framework-filtering raw SHA-256 values in `docs/OBSERVE-VISUAL-BASELINE.md` reproduced on Apple M4/Metal |

O2 does not expose coverage or geometry queries, diagnostic modes, comparison,
contact sheets, report v2, or a playback recipe. Those capabilities remain
false in the packet. Presentation is a renderer-native numeric view; it does
not add an sRGB conversion or claim GPU pixel identity across adapters.
