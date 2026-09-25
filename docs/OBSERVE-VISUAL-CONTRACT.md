# Observe visual inspection contract

Date: 2026-09-25. Contract version: `inspection-v2-draft-1`. This fixes the O0
interface and semantics for implementation. The basic Rust/Python frozen
capture, ROI rerender, scene bundle, and canonical digest slice is available;
the inspection packet API and report v2 below remain delivery targets. The existing
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
packet.close() -> None
```

`point` and `region` are mutually exclusive. `render` returns a new packet
value with an appended view and the same capture ID; it never rereads a session
or source asset. A closed packet rejects new rendering/query work. `report`
profile reopens for artifact reading, `analysis` adds CPU geometry/probe data,
and `scene` adds frozen render scene and decoded textures for new GPU views.
Uncaptured capability returns `CAPTURE_NOT_AVAILABLE`, never a fresh snapshot.

## Capture identity and evidence

One capture freezes document version, evaluated frame, object metadata,
topology, requested/actual parameters, and all required texture bytes. It
records a content hash per texture; a source file change after capture cannot
affect a derived view. File reads are individually validated, not an atomic
filesystem transaction. An animation capture uses the already evaluated
Motion/Expression/Physics/Pose frame and retains its host Model opacity policy;
it must never reconstruct the frame from final parameter values.

`capture_id` identifies an acquisition. `scene_digest` hashes canonical scene,
metadata, and texture content; `render_digest` adds view, mode, background,
sampling and renderer policy; `artifact_sha256` hashes actual output bytes.
The current scene/render digest v1 uses UTF-8 canonical JSON with sorted object
keys, array order preserved, negative zero normalized, and NaN/Inf rejected.
Future binary array payloads must declare dtype, shape, and little-endian byte
order in their own format version. GPU pixel bytes may
differ between adapters. The legacy `input_sha256` stays unchanged and is not
the new canonical digest. Report records adapter/backend and SDK build.

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
| Normal source-over, ordinary alpha | existing | required | supported after numeric equivalence test | local alpha × gates |
| Additive | existing | required in main target | conditional on actual output; reject RGB≠0 at alpha=0 | local coverage only |
| Multiply | existing | required in main target | conditional on background independence | local coverage only |
| Extended/destination-read | existing | required in main target | reject when output depends on background or cannot fold to RGBA | per-mode explicit support or unsupported |
| Mesh mask/inverted mask | existing | required | conditional on ordinary RGBA equivalence | mask-aware |
| Nested offscreen | existing | required | conditional on full composition equivalence | ancestor gates required |

The conditional cells are gates, not claims of current support. If a requested
straight output fails or cannot be established, return
`UNREPRESENTABLE_TRANSPARENT_OUTPUT`; do not silently save premultiplied PNG.
The alpha channel image comes from a transparent raw pass, not opaque output.
Pixel probes and comparison metrics retain the policy and background used.

## Report v2 and failure rules

`report.json` contains `schema_version: 2`, status (`running`, `complete`,
`partial`, `failed`, `cancelled`), run/build/platform/adapter, capture identity,
samples, animation source when applicable, object/mark table, views,
comparisons, diagnostics, provenance, capabilities, and resource accounting.
Each view has ID, sample, kind, mode, image path/hash, dimensions, view
matrices/ROI, color/alpha/background policy, dependencies and overrides.
Static samples use `source_kind="parameters"`; animation samples use
`source_kind="animation"` with actual f32 time, operation identity, recipe or
step source, host opacity policy, and stage availability. Missing history is
`not_recorded`; a replay is explicitly `replayed` and must match the target
snapshot. A zero-time reset and an advance(0) have distinct operation IDs.

Artifacts are written to unique run directories using internal IDs and
temporary-file rename. The report is updated as work progresses. A failed
run keeps completed evidence and attaches `run_directory` to the exception.
Unknown major schema versions are rejected; arrays have explicit dtype, shape
and byte order. Reader verifies hashes, lengths, paths and budgets before
exposing capabilities. `complete` requires every requested channel; only an
explicit `allow_partial` may downgrade an unavailable requested channel to
`partial`. GPU/IO failure is `failed`.

## Implementation decisions relative to the plan

- O1's first slice uses `ResolvedObservation` and `RenderRequest` with a
  rectangular ROI. It exposes `capture_scene`, `capture_animation_scene`,
  `render_scene`, `save_scene` and `open_scene` in Python. This is a data-only
  scene bundle; the full inspection packet and report v2 remain to be built.
- The initial Rust ROI limit is 4096 per side and the device limit, matching
  the default contract. Remaining aggregate budgets enter with packet/run.
- Background support waits for main-target renderer work; compositing raw
  pixels in Python would violate the destination-read requirement.
