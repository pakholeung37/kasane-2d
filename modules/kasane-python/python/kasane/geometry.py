"""Python adapters for the Rust SDK geometry builders."""
from __future__ import annotations
from typing import TYPE_CHECKING
from . import _native
if TYPE_CHECKING:
    from ._types import MeshGeometryData


def _grid_dimensions(columns: int, rows: int) -> None:
    # Python-specific type validation before conversion to Rust usize.
    if type(columns) is not int or type(rows) is not int or columns < 1 or rows < 1:
        raise ValueError("columns and rows must be positive integers")
    if columns > 65535 or rows > 65535:
        raise ValueError("rectangle grid exceeds 65536 vertices")


def rectangle_grid_geometry(source: MeshGeometryData, columns: int, rows: int) -> MeshGeometryData:
    """Subdivide a canonical rectangle, retaining corner IDs and its diagonal."""
    from ._types import MeshGeometryData
    _grid_dimensions(columns, rows)
    return MeshGeometryData(*_native.rectangle_grid_geometry(tuple(source), columns, rows))
