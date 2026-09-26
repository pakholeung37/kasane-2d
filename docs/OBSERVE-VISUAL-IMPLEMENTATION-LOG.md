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
| O3 comparison and report | complete | Same-snapshot inline baseline, compatible packet/external-reference comparison, target and non-target metrics, side-by-side/onion/outline/heatmap, explicit two-axis grid, paged sheets, complete/failed v2 reports and CPU reader | V04 normal/error paths passed with isolated wheels; animation playback runs remain O7 |
| O4 geometry diagnostics | complete | Same-pass evaluation trace, geometry overlays and canvas-space deformation diagnostics | V05 passed; query remains O6 |
| O5 isolation and mask diagnostics | complete | DiagnosticPlan, isolated/X-ray render, actual mask attachment views, hidden-geometry capture and report resources | V06 composition and recovery gates passed; per-object query remains O6 |
| O6 geometry and coverage queries | complete | Triangle point/region query, CPU analysis reopen, per-candidate GPU alpha, support matrix and frontmost pick rule | V07 gates passed; interpolation selection remains explicitly unavailable |
| O7 joint acceptance | implementation verified; agent ablation not run | Explicit animation replay, independent inspection wheel gate, combined regression, resource/performance evidence, API/validation docs | Independent agent comparison and legacy `--full` gate still need their external inputs |

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

## O3 comparison and report (2026-09-26)

The optional `inspection` extra pins Pillow 12.3.0 for external image reads
and contact-sheet headers. The base wheel still imports without Pillow; O2
numeric labels use their own bitmap glyphs. Font origin and license are in
`modules/kasane-python/FONT-PROVENANCE.md`. Static `inspect_run` freezes one
sample batch, resolves a fixed union ROI for baseline comparisons, renders
sequentially, saves per-sample report-profile packets, and publishes
`report.json` atomically after each sample. Input order, requested/actual
values and clamp/repeat differences are explicit. Two-axis grids use declared
parameter IDs/values, reject duplicate actual-value cells, and keep missing
cells. Sheets split at the 16-million-pixel page budget; each cell records a
sheet-to-view transform and keeps label space out of scene coordinates.

`compare_observations` checks view/presentation policy and requires canvas
registration across documents. Target meshes across documents require an
object map. External references are read once and record a source hash plus
declared alpha/color interpretation; unregistered references produce only a
side-by-side artifact and unavailable numeric metrics. Registered comparisons
produce side-by-side, onion skin, threshold outline and fixed 0–255 abs-diff
heatmap artifacts, change bounds, RGB/raw/alpha metrics when compatible, and
separate target/non-target domains. A mesh target ROI unions current and
reference evaluated bounds, so displacement is not hidden by a recentered
view. Without a declared target, only whole-view metrics are returned.

The v2 reader validates report version, artifact paths, hashes and dimensions
on a CPU-only wheel. A failed run retains a readable report and attaches its
directory to the exception. `Observer.inspect(..., baseline_values=...)`
captures both states from one document snapshot and stores its comparison in
all packet profiles. The report records render/readback counts, pixel/byte
totals, elapsed time and a CPU image memory estimate; it does not claim a
measured process peak. Animation playback runs, partial/cancelled reports and
diagnostic/query channels remain later-stage work.

| O3 evidence | Result |
| --- | --- |
| Source and fixtures | Started from `8420055`; fixture manifest remains `a971cfc4a93a55c3a335ae35df8f425968f4e5490dbe3ce1c333a0eacf530331` |
| `cargo fmt --check`, `cargo test --workspace --locked -q`, `cargo clippy --workspace --all-targets --locked -- -D warnings` | passed |
| release observe wheel plus `Pillow==12.3.0` in isolated CPython 3.14 | 16 `test_observe.py` and 6 `test_observe_o3.py` cases passed; incompatible policy, unregistered reference, target geometry union, grid duplicates, failed report, hash tampering and pagination covered |
| release CPU-only wheel without Pillow in isolated CPython 3.14 | 52 `test_cpu.py` cases passed; opened the GPU-produced v2 run and saved inline comparison with four artifacts |
| legacy raw baseline | all five current Framework-filtering raw SHA-256 values reproduced on Apple M4 / Metal |
| visual QA | inspected the two-cell contact sheet and fixed-range heatmap under `/tmp/kasane-o3-qa/inspection-4f172031c8f741c68406e8e41371b3ea/` |

The built wheels are in `/tmp/kasane-observe-o3-wheel/` and
`/tmp/kasane-observe-o3-cpu-wheel/`. These paths are local evidence, not
portable golden artifacts.

## O4 evaluated geometry and deformer diagnostics (2026-09-26)

`FrameEvaluator::evaluate_with_trace` adds a same-pass, opt-in Rust
`EvaluationTrace`; ordinary evaluation leaves the trace buffer unallocated.
The trace pairs final drawable positions with source vertex IDs, validates
triangle/index correspondence after the renderer's winding reversal, and
records ordered triangle IDs plus a topology SHA-256. Final positions include
BlendShape, transform, and glue evaluation. Each deformer records enabled
state, parent chain, evaluated warp controls and local control indices.
Rotation axes are sampled at 17 points through the actual parent, so a
multi-column warp can bend the displayed axis. Animation capture can request
the trace using its actual preview parameter state and Part opacity frame.
Scene bundles and analysis packets retain the optional trace with their
existing content/hash checks; report packets retain overlays and numeric
diagnostics without carrying the full trace.

New context channels are `wireframe`, sparse `vertices`, and `deformers`.
`displacement` and `distortion` require a baseline sample; they add arrows and
abnormal triangle outlines without changing the clean render. Each overlay
records segment and label omissions under a 20,000-segment density limit.
`Observer.inspect(..., baseline_values=...)` and `inspect_run` with
`baseline_index` report canvas-space triangle `F=C*inverse(B)`, determinant,
two singular values, degeneration, orientation reversal, and threshold flags.
The defaults are min stretch below 0.5 and max stretch above 2.0. Twice-area
epsilon is `max(1e-12 px², 1e-8 * baseline maximum edge length² in px²)`;
an uninvertible baseline has no finite score. Topology or vertex identity
changes report `TOPOLOGY_MISMATCH`. A known rotation reflection toggle is
separate from a numeric orientation reversal. Displacement represents the
whole evaluated result, including glue and BlendShape, not a keyform delta.

| O4 evidence | Result |
| --- | --- |
| `cargo test --workspace --locked -q`, `cargo fmt --check`, `cargo clippy --workspace --all-targets --locked -- -D warnings` | passed |
| release observe wheel, CPython 3.14 | 27 Observe tests passed, including 5 O4 cases for parent-warp axis sampling, animation trace, BlendShape/glue final positions, rigid/reflect/stretch/degenerate numbers, camera independence, topology mismatch, scene round trip, and run reopening |
| release CPU-only wheel, CPython 3.14 | 52 CPU tests passed; reopened O4 analysis packet and v2 report with deformation data and overlay views |
| visual QA | inspected wireframe, vertex, displacement and deformer views under `/tmp/kasane-o4-qa/` |
| default raw path | all five SHA-256 values match the O3 Framework-filtering baseline in `/tmp/kasane-observe-o3-baseline.json` |

Built wheels are in `/tmp/kasane-observe-o4-wheel/` and
`/tmp/kasane-observe-o4-cpu-wheel/`. These local paths are test evidence.

## O5 isolation, X-ray and mask diagnostics (2026-09-26)

`DiagnosticPlan` selects color meshes separately from mask-only sources and
keeps ancestor offscreen targets in render-plan order. The derived frame
retains target-local commands and mask inputs without editing the captured
scene or live session. The normal renderer applies destination reads to the
isolated background and lower draws; the view reports
`destination_context="isolated"`. `XraySpec` records explicit mask, opacity,
and disabled overrides, and the output pixels carry an XRAY marker. Opt-in
hidden-geometry capture evaluates disabled mesh positions for that view,
including animation preview captures.

The mask channel reads the renderer's raw mask attachments after each source
pass and combined pass. It exports source alpha with evaluated geometry,
combined alpha, post-inversion consumer alpha, and isolated consumer
composite alpha. Each view records target dimensions, source IDs, origin,
scale, and both mask/canvas sampling matrices. Composite alpha is labeled as
an aid rather than a color-contribution estimate. Report resources count the
main-target readback incurred by every mask attachment pass. Scene/profile
reopen retains diagnostic views and hidden-geometry capture identity.

| O5 evidence | Result |
| --- | --- |
| `cargo test --workspace --locked -q`, `cargo fmt --check`, `cargo clippy --workspace --all-targets --locked -- -D warnings` | passed |
| release observe wheel, CPython 3.14 | 35 Observe tests passed, including 8 O5 cases for mask-only dependencies, inversion, nested masked offscreen targets with destination read, disabled and animation X-ray, diagnostic cache recovery, budgets, and report resources |
| release CPU-only wheel, CPython 3.14 | 52 CPU tests passed; reopened the O5 analysis packet and v2 run with six diagnostic views |
| visual QA | inspected seven-view strip at `/tmp/kasane-o5-qa.png` |
| default raw path | all five raw and legacy input SHA-256 values match `/tmp/kasane-o4-baseline.json` |

Built wheels are in `/tmp/kasane-observe-o5-wheel/` and
`/tmp/kasane-observe-o5-cpu-wheel/`. These local paths are test evidence.

## O6 image-coordinate geometry and coverage queries (2026-09-26)

`InspectionPacket.query` resolves a view ID and checks evaluated triangles in
image coordinates. Point hits return all triangles at the continuous point,
including shared-edge ambiguity, with vertex IDs, barycentric weights, UV,
canvas/runtime coordinates, composition path and source revision. Region hits
use positive-area triangle/half-open-rectangle clipping. AABB intersection
alone is never a hit. Analysis packet queries and `object_details` work in a
CPU-only wheel. The pixel probe reads only captured raw/clean buffers with
the same mapping, explicitly reporting missing buffers. `max_hits` reports
total and truncation instead of silently dropping ambiguity.

Coverage uses a per-mesh isolated GPU pass on the point's one-pixel cell or
a bounded region. The diagnostic normalizes additive/multiplicative color
blending to source alpha while keeping texture sampling, UV, culling, mesh
masks/inversion, opacity and ancestor offscreen gates. The renderer frame is
derived from the frozen capture; normal clean results remain unchanged.
Extended raw mesh blending or a nonzero ancestor offscreen blend reports
`unsupported_composition` and keeps geometry candidates. Region results give
per-triangle covered pixel count, half-open bounds and maximum alpha. The
frontmost rule uses captured render-plan order and does not claim unique color
contribution; it gives no answer for unsupported or truncated candidates.
Coverage regions are capped at 262,144 pixels and candidate readbacks at
8 million pixels before diagnostic GPU work. `ObjectDetails` identifies
source bindings and reports interpolation selection as `not_computed`;
geometry with glue/deformers is not presented as an editable point.

| O6 evidence | Result |
| --- | --- |
| `cargo test --workspace --locked -q`, `cargo fmt --check`, `cargo clippy --workspace --all-targets --locked -- -D warnings` | passed |
| release observe wheel, CPython 3.14 | 44 Observe tests passed, including 9 O6 cases for shared triangles, bbox false positives, offline analysis, transparent texels, normal/additive/multiplicative alpha, mesh mask inversion, offscreen gate and unsupported composition, frontmost rule, region counts and limits |
| release CPU-only wheel, CPython 3.14 | 52 CPU tests passed; reopened an O6 analysis packet and queried geometry/object details without GPU |
| default raw path | all five raw and legacy input SHA-256 values match `/tmp/kasane-o5-baseline.json` |

Built wheels are in `/tmp/kasane-observe-o6-wheel/` and
`/tmp/kasane-observe-o6-cpu-wheel/`. These local paths are test evidence.

## O7 combined inspection acceptance (2026-09-26)

`PlaybackRecipe` records ordered base-parameter, motion, registered motion,
expression and timed-parameter actions. A detached Motion preview is built
from one session snapshot, applies the recipe, and seeks each requested
absolute time before the shared texture-union capture. Source metadata stores
the complete recipe with `history_status=recipe_recorded`; direct capture of
an existing preview continues to state `not_recorded` because its earlier
history is unknown. `inspect_animation_run` writes the same recoverable v2
report as a parameter run, including a `playback` record. Tests compare recipe
samples against an independently operated Motion preview, check nonmonotonic
seeks, baseline comparison, CPU-only reopening, a failed recipe report and
observer recovery after close.

The `--require-inspection` wheel validator installs the wheel and pinned
`inspection` extra in a fresh environment outside the repository. It requires
GPU Observe and runs the base GPU, O3–O7 and CPU suites. A CPU-only wheel was
also used as a negative control: the release gate failed with
`Inspection release requires a GPU Observe wheel`. `--full` remains a
separate legacy acceptance profile, and can be combined with `--inspection`
when its historical recipe scripts and Core probes are available.

| O7 evidence | Result |
| --- | --- |
| Source base / fixture | Started at `c325543`; fixture manifest SHA-256 `a971cfc4a93a55c3a335ae35df8f425968f4e5490dbe3ce1c333a0eacf530331` |
| `cargo fmt --check`, `cargo test --workspace --locked -q`, `cargo clippy --workspace --all-targets --locked -- -D warnings` | passed |
| release Observe wheel plus `inspection` extra, CPython 3.14, Apple M4/Metal | `--require-inspection` passed: 16 base GPU, 32 O3–O7, 52 CPU tests; wheel SHA-256 `63e4be33bf9ea889ef453271cd7d1cf2847616a31d3aa741a08963c627ea3dce`; report `/tmp/kasane-o7-sdk-validation/41e285c6991c41e68f51c1f844eb7f61/report.json` |
| CPU-only wheel | Reopened a three-sample animation v2 report with two views and two comparisons, without Pillow or a GPU; persisted report under `/tmp/kasane-o7-persist/` |
| raw compatibility | All five raw RGBA and legacy input SHA-256 values match `/tmp/kasane-o6-baseline.json`; current record `/tmp/kasane-o7-baseline.json` |
| resource/performance | Five 128×128 repetitions: legacy raw median 2.054 ms, capture including decode 0.389 ms, raw rerender/readback 1.701 ms, clean inspect 2.163 ms, clean plus labels 2.733 ms, report-profile save 1.104 ms. Twenty further packet closes raised process RSS high-water mark by 81,920 bytes; see `/tmp/kasane-o7-benchmark.json`. This high-water mark does not prove absence of Rust/GPU leaks. Decode and upload have no separate timer and are recorded as unavailable. |

| Trace | Implementation and combined evidence |
| --- | --- |
| V01 alpha/color | O2 light/dark/checker/transparent tests plus unchanged raw baseline |
| V02 ROI/focus | O2 subpixel, overscan and fixed/follow tests; O7 animation fixed-union report |
| V03 marks | O2 stable dense labels and object table; packet/report reopening |
| V04 comparison | O3 reference/registration/grid/pagination tests; O7 animation baseline comparisons |
| V05 evaluated geometry | O4 deformation/trace tests; O7 wireframe animation report |
| V06 isolation/mask | O5 nested/inverted/disabled tests and post-diagnostic clean recovery |
| V07 geometry/coverage query | O6 CPU geometry and GPU coverage tests, including unsupported composition and limits |

The independent agent ablation in the plan is **not_run**. It needs frozen
tasks, an untuned asset and separate participant sessions for the four tool
conditions; no outcome or effectiveness claim is inferred from visual QA or
unit tests. The legacy `--full` validator is also **not_run** in this checkout:
its `examples/sdk/` recipe scripts are absent, and official Core/image
reference probes were not supplied. The new inspection gate does not claim
their results. Known capability limits remain those in
`docs/OBSERVE-VISUAL-CONTRACT.md`, including unsupported special-composition
coverage and unavailable edit interpolation for warp/glue geometry.

### Agent experiment preflight after O7 commit

The existing S4-A (`visual-locate`), S4-B v2 (`visual-parent`) and
Shirousagi `shirousagi-repair` tasks were frozen with the final O7 wheel in
`/tmp/kasane-observe-o7-ablation-preflight-v2/`. Each preparation passed its
positive control and rejected its no-edit, wrong-target or wrong-amount
controls; the Shirousagi task also rejected a wrong keyform. This is a
**harness preflight**, not an agent trial. Its model field is
`PREP_ONLY_NO_AGENT` and no `run`, `assess`, or `review` result exists.

The first Shirousagi preflight found that its task references use Cubism
runtime parameter IDs such as `ParamAngleX`, while the current SDK accepts
UUIDs or unique display names for parameter samples; this local model's
display names are Japanese. The experiment worker now resolves runtime IDs to
the imported parameter UUID before rendering or evaluating, while retaining
the original reference labels for participants. The corrected frozen
preflight passed. The requested white-rabbit compression/occlusion variant and
a separate untuned fourth asset still need task-specific oracles and
positive/negative controls before an ablation can be run. The three existing
tasks alone are not presented as a four-condition effectiveness result.
