"""Typed Python records for the Rust SDK's frozen-scene spatial queries."""
from __future__ import annotations

from dataclasses import dataclass
import json
import math
from typing import Callable, Literal, Protocol, Sequence


@dataclass(frozen=True)
class ObjectRef:
    """Select one mesh or Part by its stable ID, not its display name."""

    kind: Literal["mesh", "part"]
    id: str

    def __post_init__(self) -> None:
        if self.kind not in ("mesh", "part") or not isinstance(self.id, str) or not self.id:
            raise ValueError("ObjectRef needs a mesh or part kind and a nonempty ID")


@dataclass(frozen=True)
class ObjectBounds:
    """Evaluated geometry bounds in source-canvas pixels for one capture.

    ``mesh_ids`` lists resolved descendants even when a drawing filter removes
    them from the bounds. An empty result has ``canvas_bounds=None`` and an
    ``empty_reason``. Bounds do not measure painted or unoccluded pixels.
    """

    capture_id: str
    scene_digest: str
    targets: tuple[ObjectRef, ...]
    mesh_ids: tuple[str, ...]
    canvas_bounds: tuple[float, float, float, float] | None
    status: Literal["ok", "empty"]
    empty_reason: Literal["no_descendant_mesh", "no_evaluated_positions", "filtered_out"] | None


@dataclass(frozen=True)
class TriangleHit:
    """One containing triangle and the point's interpolation coordinates.

    ``vertex_indices`` address this evaluated drawable's arrays;
    ``vertex_ids`` are stable authoring IDs when available. ``uv`` uses the
    triangle's evaluated barycentric weights.
    """

    triangle_index: int
    vertex_indices: tuple[int, int, int]
    vertex_ids: tuple[int, int, int] | None
    barycentric: tuple[float, float, float]
    uv: tuple[float, float] | None


@dataclass(frozen=True)
class GeometryHit:
    """One mesh candidate, with its ancestor Part path from root to leaf.

    Disabled, transparent, masked, or occluded meshes can still appear.
    ``triangles`` is populated only when query ``details=True``.
    """

    mesh_id: str
    name: str
    part_path: tuple[str, ...]
    part_names: tuple[str, ...]
    enabled: bool
    visible: bool
    opacity: float
    render_order: int
    triangles: tuple[TriangleHit, ...]


@dataclass(frozen=True)
class HitTestResult:
    """All geometry candidates at an image point, subject to the result limit.

    ``total`` counts mesh candidates before truncation. For an out-of-image
    point, ``status='outside_image'`` and ``canvas_point`` is absent.
    """

    capture_id: str
    scene_digest: str
    image_point: tuple[float, float]
    canvas_point: tuple[float, float] | None
    hits: tuple[GeometryHit, ...]
    total: int
    truncated: bool
    status: Literal["hit", "no_hit", "outside_image"]


class SpatialSource(Protocol):
    """Internal bridge shared by live captures and reopened analysis data."""

    def bounds_json(self, targets: list[tuple[str, str]], include_hidden: bool) -> str: ...
    def hit_test_json(self, canvas_point: tuple[float, float], include_hidden: bool,
                      details: bool, max_candidates: int) -> str: ...


def _targets(value: ObjectRef | Sequence[ObjectRef]) -> tuple[ObjectRef, ...]:
    result = (value,) if isinstance(value, ObjectRef) else tuple(value)
    if not result or any(not isinstance(item, ObjectRef) for item in result):
        raise ValueError("Select one or more ObjectRef values")
    return tuple(dict.fromkeys(result))


def object_bounds(
    source: SpatialSource, capture_id: str, scene_digest: str,
    targets: ObjectRef | Sequence[ObjectRef], *, include_hidden: bool = True,
) -> ObjectBounds:
    """Wrap the Rust SDK's bounds result with capture identity and target IDs."""
    requested = _targets(targets)
    raw = json.loads(source.bounds_json(
        [(target.kind, target.id) for target in requested], include_hidden,
    ))
    bounds = raw["canvas_bounds"]
    return ObjectBounds(
        capture_id, scene_digest, requested, tuple(raw["mesh_ids"]),
        tuple(bounds) if bounds is not None else None,
        "ok" if bounds is not None else "empty", raw["empty_reason"],
    )


def hit_test(
    source: SpatialSource, capture_id: str, scene_digest: str,
    image_point: tuple[float, float],
    image_to_canvas: Callable[[tuple[float, float]], tuple[float, float]],
    image_size: tuple[int, int], *, include_hidden: bool = True,
    details: bool = False, max_candidates: int = 256,
) -> HitTestResult:
    """Map a view point to mesh hits; no texture or occlusion test is made."""
    if (len(image_point) != 2 or not all(math.isfinite(v) for v in image_point)
            or type(max_candidates) is not int or max_candidates <= 0):
        raise ValueError("Point must be finite and max_candidates positive")
    if not (0 <= image_point[0] < image_size[0]
            and 0 <= image_point[1] < image_size[1]):
        return HitTestResult(capture_id, scene_digest, image_point, None,
                             (), 0, False, "outside_image")
    canvas_point = image_to_canvas(image_point)
    raw = json.loads(source.hit_test_json(
        canvas_point, include_hidden, details, max_candidates,
    ))
    hits = tuple(GeometryHit(
        item["mesh_id"], item["name"], tuple(item["part_path"]),
        tuple(item["part_names"]), item["enabled"], item["visible"],
        item["opacity"], item["render_order"],
        tuple(TriangleHit(
            tri["triangle_index"], tuple(tri["vertex_indices"]),
            tuple(tri["vertex_ids"]) if tri["vertex_ids"] is not None else None,
            tuple(tri["barycentric"]), tuple(tri["uv"]) if tri["uv"] is not None else None,
        ) for tri in item["triangles"]),
    ) for item in raw["hits"])
    return HitTestResult(capture_id, scene_digest, image_point, canvas_point,
                         hits, raw["total"], raw["truncated"],
                         "hit" if raw["total"] else "no_hit")
