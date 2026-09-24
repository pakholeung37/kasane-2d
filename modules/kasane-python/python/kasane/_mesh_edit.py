"""Compatibility adapter for the Rust SDK remeshing operation."""
from __future__ import annotations
from typing import TYPE_CHECKING
from .geometry import _grid_dimensions
if TYPE_CHECKING:
    from ._types import MeshRecordSnapshot, Version
    from ._session import Session


def remesh_rectangle_grid(session: Session, mesh_id: str, columns: int, rows: int,
                          expected_version: Version | None) -> MeshRecordSnapshot:
    _grid_dimensions(columns, rows)
    session._native.remesh_rectangle_grid(mesh_id, columns, rows, expected_version)
    result = session.mesh_record(mesh_id)
    assert result is not None
    return result
