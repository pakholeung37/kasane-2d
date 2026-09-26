# Observe visual inspection contract

Date: 2026-09-25. Contract version: `inspection-v2-draft-1`. This fixes the O0
interface and semantics for implementation. The basic Rust/Python frozen
capture, ROI rerender, scene bundle, canonical digest, raw packet, and bounded
save-profile slice is available. O2 adds presentation, evaluated-geometry
focus, fixed/follow sample views, numeric labels, and an object table. O3 adds
registered image comparison, explicit two-axis layouts, paged contact sheets,
and a recoverable report v2. O4 adds optional Rust evaluation traces,
geometry overlays, and canvas-space deformation diagnostics. O5 adds
diagnostic isolation, X-ray and renderer mask attachment views. Queries remain
a delivery target. The existing
`Observer.observe`, `observe_run`, raw bytes, and report v1 remain unchanged.

## Public types and calls

All records are frozen dataclasses in Python and typed Rust records at the
boundary. Unknown enum values and nonfinite numbers are errors. IDs are UUIDs,
not display names; where names are accepted, they must resolve uniquely within
the captured document.

```python
@dataclass(frozen=True)
class Focus:
    mesh_ids: tuple[str, ...] = ()
    part_ids: tuple[str, ...] = ()

@dataclass(frozen=True)
class ViewSpec:
    roi: tuple[float, float, float, float] | None = None  # source canvas
    resolution: tuple[int, int] = (1024, 1024)
    padding_canvas: float = 0.0
    framing: Literal["fixed_union", "follow"] = "fixed_union"
    aspect: Literal["contain"] = "contain"

@dataclass(frozen=True)
class PresentationSpec:
    background: Literal["light", "dark", "checker", "transparent"] = "light"
    alpha: Literal["opaque", "straight"] = "opaque"
    color_policy: Literal["renderer_native_v1"] = "renderer_native_v1"

@dataclass(frozen=True)
class OverlaySpec:
    max_labels: int = 12
    vertex_ids: tuple[int, ...] = ()
    show_leaders: bool = True

@dataclass(frozen=True)
class DiagnosticSpec:
    alpha_threshold: float = 1 / 255
    min_stretch: float = 0.5
    max_stretch: float = 2.0
    trace_fields: tuple[str, ...] = ()

@dataclass(frozen=True)
class InspectionLimits:
    max_view_side: int = 4096
    max_page_pixels: int = 16_000_000
    max_artifact_pixels: int = 64_000_000
    max_samples: int = 64
    max_labels: int = 12
    max_vertex_labels: int = 64
    max_query_candidates: int = 256
    max_cpu_retained_bytes: int = 268_435_456

@dataclass(frozen=True)
class InspectionRequest:
    focus: Focus = Focus()
    view: ViewSpec = ViewSpec()
    presentation: PresentationSpec = PresentationSpec()
    overlay: OverlaySpec = OverlaySpec()
    diagnostics: DiagnosticSpec = DiagnosticSpec()
    channels: tuple[str, ...] = ("clean", "labels")
    mode: Literal["context", "isolated", "xray"] = "context"
    xray: XraySpec = XraySpec()
    limits: InspectionLimits = InspectionLimits()
    allow_partial: bool = False
```

Unknown fields in a major version are rejected. Defaults do not request
expensive wireframe, mask, coverage, or animation traces.

Result records have these required fields; IDs are stable inside one packet,
and all absent measurements have an explicit status/reason:

| Type | Required fields |
| --- | --- |
| `InspectionPacket` | `capture_id`, `scene_digest`, `source_kind`, `document_id`, `version`, `evaluation_revision`, `objects`, `views`, `capabilities`, `profile`, `closed` |
| `InspectionView` | `view_id`, `sample_index`, `kind`, `mode`, `width`, `height`, `rgba/png or artifact_path`, `artifact_sha256`, `requested_roi`, `padded_roi`, `visible_roi`, `canvas_to_image`, `image_to_canvas`, `presentation`, `status` |
| `ObjectDetails` | `object_id`, `mark`, `authored_metadata`, `evaluated_geometry`, `topology_hash`, `binding_provenance`, `edit_mapping_status`, `diagnostics`, `capabilities` |
| `QueryHit` | `object_id`, `triangle_key`, `vertex_ids`, `barycentric`, `image_point`, `canvas_point`, `runtime_point`, `uv`, `coverage_status/value`, `composition_path`, `source_revision`, `binding_provenance_ref` |
| `QueryResult` | `view_id`, `mode`, `requested_point/region`, `sample_point`, `status`, `hits`, `total`, `truncated`, `pixel_probe` |
| `PixelProbe` | `sample_pixel`, `raw_rgba/status`, `presentation_rgba/status`, `view/policy IDs`, optional neighbourhood artifact |
| `ComparisonResult` | `current/reference view IDs`, `registration_status`, `view_compatibility`, `artifact IDs`, `target/non_target metrics`, `change_bounds`, `diagnostics` |
| `InspectionRun` | `run_directory`, `report_path`, `status`, `packet/view references`, `artifacts`, `resources` |
| `SaveReceipt` | `absolute_directory`, `profile`, `manifest_sha256`, `artifact hashes`, `saved_bytes`, `elapsed_ms` |

```python
observer.inspect(session, values=None, *, request, baseline_values=None) -> InspectionPacket
observer.inspect_run(session, samples, *, request, output, baseline_index=None, layout=None) -> InspectionRun
observer.inspect_animation(session, preview, *, request, apply_model_opacity=False) -> InspectionPacket
observer.inspect_animation_run(session, *, playback, times, request, output) -> InspectionRun
observer.render(packet, *, request) -> InspectionPacket
observer.object_details(packet, *, object_id) -> ObjectDetails
observer.query(packet, *, view_id, point=None, region=None,
               mode="coverage", alpha_threshold=1/255) -> QueryResult
compare_observations(current, reference, *, view_id, reference_view_id,
                     options) -> ComparisonResult
packet.save(absolute_directory, *, profile="analysis") -> SaveReceipt
observer.open(absolute_directory) -> InspectionPacket
open_inspection_run(absolute_directory) -> InspectionRun
packet.close() -> None
```

`point` and `region` are mutually exclusive. `render` returns a new packet
value with an appended view and the same capture ID; it never rereads a session
or source asset. A closed packet rejects new rendering/query work. `report`
profile reopens for artifact reading, `analysis` adds CPU geometry/probe data,
and `scene` adds frozen render scene and decoded textures for new GPU views.
Uncaptured capability returns `CAPTURE_NOT_AVAILABLE`, never a fresh snapshot.

The O1 raw subset uses `RawInspectionRequest(roi, resolution,
padding_canvas)`. O2 `InspectionRequest` supports `context` mode with clean,
labels, and alpha channels. O4 adds wireframe, vertices, deformers,
displacement, and distortion channels; the last two require an explicit
baseline. O5 implements isolated and X-ray modes plus a mask channel for one
focused mesh with a mesh or ancestor offscreen mask. X-ray uses an isolated
alpha pass and evaluated outlines; explicit mask, opacity, and disabled
overrides are recorded. Disabled X-ray requires an opt-in hidden-geometry
capture. Mask source, combined, post-inversion consumer, and isolated
composite-alpha views preserve source IDs and sampling transforms; composite
alpha is not a final color-contribution measurement.
`inspect_samples` freezes a batch once and applies fixed-union or follow framing.
The packet manifest remains `kasane-inspection-packet` schema 2 with per-view
presentation policy, object marks, focus status and explicit capability flags.
O3 adds `compare_observations`, `ExternalReference`, `CompareOptions`,
`SequenceLayout`/`GridLayout`, `Observer.inspect_run`, and
`open_inspection_run`. `Observer.inspect(..., baseline_values=...)` freezes
both parameter states from one snapshot and attaches a comparison that survives
packet save/reopen. A registered comparison uses compatible view/pixel
policies and an explicit target ROI or evaluated mesh union; an unregistered
external reference is side-by-side only. A failed run retains a readable v2
report. CPU geometry queries remain later work. A live preview capture keeps
the last successful operation identity while reporting history as
`not_recorded`; it does not claim a playback recipe or resumable runtime.

## Capture identity and evidence

One capture freezes document version, evaluated frame, object metadata,
topology, requested/actual parameters, and all required texture bytes. It
records a content hash per texture; a source file change after capture cannot
affect a derived view. File reads are individually validated, not an atomic
filesystem transaction. An animation capture uses the already evaluated
Motion/Expression/Physics/Pose frame and retains its host Model opacity policy;
it must never reconstruct the frame from final parameter values.

`capture_id` identifies an acquisition. `scene_digest` hashes canonical scene,
metadata, and texture content, excluding live animation operation identity;
`render_digest` adds view, mode, background,
sampling and renderer policy; `artifact_sha256` hashes actual output bytes.
The current scene/render digest v1 uses UTF-8 canonical JSON with sorted object
keys, array order preserved, negative zero normalized, and NaN/Inf rejected.
Future binary array payloads must declare dtype, shape, and little-endian byte
order in their own format version. GPU pixel bytes may differ between adapters.
Default Observe builds follow the local Cubism Native Framework sample's
source texture policy: mipmaps with linear pixel and mip-level filtering,
repeating source UVs, no anisotropic filtering, and one framebuffer sample.
Mip texels use an area-weighted box filter (`area_box_v2`), including the
full footprint for odd dimensions; the generation policy is part of render
identity. Bundle texture descriptors always record the resolved content hash,
even if the source descriptor had no expected hash.
Mesh edges receive no MSAA or postprocess. A
`--no-default-features` build retains the earlier source texture policy.
`input_sha256` includes the enabled texture sampling policy and
is not the new canonical digest. Report records adapter/backend and SDK build.

Evidence tags are `rendered_pixels`, `evaluated_geometry`, `runtime_trace`,
`derived_measurement`, `authored_metadata`, and `user_annotation`. Names and
Part paths are authoring labels, not proof of visual meaning. `enabled` and
`visible` are drawing conditions, not final pixel visibility. Unmeasured
coverage is `unknown`, never zero.

## Coordinates and queries

Canvas input uses source pixels with origin top left and y downward. Continuous
ROI is `(x0,y0,x1,y1)` with positive extent. Image pixels are addressed at
centres `(i+0.5,j+0.5)`; integer region bounds are half open. ROI may exceed
the canvas. `contain` computes `s=min(W/rw,H/rh)` after padding and
`offset=((W-s*rw)/2-s*x0, (H-s*rh)/2-s*y0)`. The whole viewport is rendered;
areas outside the requested ROI can contain real scene. Each view reports
requested/padded/visible ROI and both inverse 3×3 matrices. Runtime→canvas
uses canvas origin and pixels per unit with a y flip. A transform inverse gives
an evaluated local point, not an editable keyform coordinate.

`geometry` point hits require triangle containment after AABB prefilter and
return every matching triangle with topology key and barycentric weights.
Region queries use actual triangle/region intersection. `coverage` adds
texture alpha, drawable opacity, masks and composition gates; it is local
coverage before final occlusion, not color contribution. `frontmost_covered`
is a defined pick rule over covered candidates and ScenePlan ordering. For
unsupported composition, keep geometry hits and return
`coverage_status="unsupported_composition"`. `PixelProbe` reads original raw
and presentation buffers at a documented sample pixel, never a labeled image.
Out-of-image requests return `outside_image`; image pixels outside requested
ROI remain queryable. Candidate truncation reports total and limit.

Binding provenance lists selected binding ID, axis IDs/values, selected
keyform coordinates/indexes and interpolation weights, effective BlendShape,
parent chain, glue, and source revision. It names `set_mesh_keyform` for a
selected form or `update_positions` for base geometry as distinct edit paths.
It does not calculate a required edit delta; default status is
`requires_target_selection`. Zero weight, incompatible topology, or nonlinear
parents are explicit limitations.

## Pixel and blend support matrix

`raw` means the existing RGBA8Unorm readback and PNG path with no post
conversion. `opaque` means a light/dark/checker background initialized in the
**main scene target before drawing**, including destination reads. Mask and
child offscreen targets still clear transparent. `straight` means portable
source-over transparent PNG, only when equivalence has been established; it
unpremultiplies with a recorded rounding rule and sets RGB=0 where alpha=0.
The current numeric color policy is `renderer_native_v1`, without a new gamma
conversion.

| Rendering path | raw | opaque background | straight transparent | coverage |
| --- | --- | --- | --- | --- |
| Normal source-over, ordinary alpha | existing | O2 implemented | O2 implemented with round-half-up unpremultiply | O6 local alpha × gates |
| Additive | existing | O2 implemented | O2 rejects | O6 local coverage only |
| Multiply | existing | O2 implemented | O2 rejects | O6 local coverage only |
| Extended/destination-read | existing | O2 implemented | O2 rejects | O6 per-mode coverage or unsupported |
| Mesh mask/inverted mask | existing | O2 implemented | O2 normal-blend path | O6 mask-aware |
| Nested offscreen | existing | O2 implemented | O2 normal-blend path | O6 ancestor gates |

O2 rejects any special drawable or Offscreen blend for straight output. This
is conservative because one transparent RGBA image cannot promise the same
result on every destination. If a requested straight output cannot be
established, return
`UNREPRESENTABLE_TRANSPARENT_OUTPUT`; do not silently save premultiplied PNG.
The alpha channel image comes from a transparent raw pass, not opaque output.
Pixel probes and comparison metrics retain the policy and background used.

## Report v2 and failure rules

`report.json` contains `schema_version: 2`, status (`running`, `complete`,
`failed` in O3; `partial` and `cancelled` remain reserved), run/build/platform/adapter, capture identity,
samples, animation source when applicable, object/mark table, views,
comparisons, diagnostics, provenance, capabilities, and resource accounting.
Each view has ID, sample, kind, mode, image path/hash, dimensions, view
matrices/ROI, color/alpha/background policy, dependencies and overrides.
Static samples use `source_kind="parameters"`; later animation runs use
`source_kind="animation"` with actual f32 time, operation identity, recipe or
step source, host opacity policy, and stage availability. Missing history is
`not_recorded`; a replay is explicitly `replayed` and must match the target
snapshot. A zero-time reset and an advance(0) have distinct operation IDs.

Artifacts are written to unique run directories using internal IDs and
temporary-file rename. The report is updated as work progresses. A failed
run keeps completed evidence and attaches `run_directory` to the exception.
Unknown major schema versions are rejected. O3 run views and comparisons are
PNG artifacts; report-profile packets contain no raw arrays. The reader
verifies hashes, dimensions, paths and read budgets before exposing a run.
`complete` requires every requested channel. O2 still rejects `allow_partial`;
partial/cancelled runs and animation playback recipes remain later work. GPU/IO
failure is `failed`, with completed evidence retained in the report.

## Implementation decisions relative to the plan

- O1 uses `ResolvedObservation` and `RenderRequest` for frozen scene bundles,
  explicit ROI rerender, and bounded packet profiles.
- O2 initializes the main target before destination reads and adds presentation,
  focus, object metadata, and fixed/follow views. The Rust ROI limit remains
  4096 per side and the device limit.
- O3 uses a separately installed `inspection` extra for Pillow based external
  image reads and contact sheets. It leaves baseline views in fixed canvas
  coordinates, checks aggregate artifact pixels before GPU rendering, and
  reports CPU image memory as an estimate rather than a process peak.
