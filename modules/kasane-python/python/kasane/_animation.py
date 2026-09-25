"""Detached CPU Expression preview."""
from __future__ import annotations

from collections.abc import Callable

from ._types import DrawableSample, Evaluation, ExpressionSnapshot, MotionSnapshot, ParameterSample


class ExpressionPreview:
    """Replay expressions without modifying the authoring session.

    The preview captures the document when created. Create a new preview after
    editing expression assets or model parameters.
    """

    def __init__(self, native) -> None:
        self._native = native

    @property
    def document_revision(self) -> int:
        return self._native.document_revision()

    def snapshot(self) -> ExpressionSnapshot:
        time, values, active = self._native.snapshot()
        return ExpressionSnapshot(time, dict(values), active)

    def set_base_parameter(self, parameter_id: str, value: float) -> None:
        """Set a parameter UUID's baseline value and reset playback."""
        self._native.set_base_parameter(parameter_id, value)

    def schedule_expression(self, expression_id: str, time: float) -> None:
        """Activate an expression at a nonnegative animation time."""
        self._native.schedule_expression(expression_id, time)

    def reset(self) -> None:
        """Restart time and active state while retaining the activation schedule."""
        self._native.reset()

    def advance(self, dt: float) -> ExpressionSnapshot:
        """Advance by a nonnegative delta, then return the parameter state."""
        time, values, active = self._native.advance(dt)
        return ExpressionSnapshot(time, dict(values), active)

    def seek(self, time: float) -> ExpressionSnapshot:
        """Replay at 60 Hz from the initial state to the requested time."""
        time, values, active = self._native.seek(time)
        return ExpressionSnapshot(time, dict(values), active)

    def frame(self) -> Evaluation:
        """Evaluate geometry using the current expression parameter values."""
        parameters, drawables = self._native.frame()
        return Evaluation(
            [ParameterSample(*sample) for sample in parameters],
            [DrawableSample(*sample) for sample in drawables],
        )


class MotionPreview:
    """Replay motion clips without changing the authoring session."""

    def __init__(self, native) -> None:
        self._native = native

    @property
    def document_revision(self) -> int:
        return self._native.document_revision()

    @staticmethod
    def _snapshot(raw) -> MotionSnapshot:
        time, parameters, parts, part_opacities, opacity, active, expressions, events, coverage = raw
        return MotionSnapshot(time, dict(parameters), dict(parts), dict(part_opacities), opacity, active, expressions, events, coverage)

    def snapshot(self) -> MotionSnapshot:
        return self._snapshot(self._native.snapshot())

    def set_base_parameter(self, parameter_id: str, value: float) -> None:
        self._native.set_base_parameter(parameter_id, value)

    def schedule_motion(self, motion_id: str, time: float) -> None:
        self._native.schedule_motion(motion_id, time)

    def schedule_expression(self, expression_id: str, time: float) -> None:
        """Activate an Expression after Motion and before Physics in this preview."""
        self._native.schedule_expression(expression_id, time)

    def schedule_parameter_input(self, parameter_id: str, time: float, value: float) -> None:
        """Apply an editor input at time; seek replays the same input history."""
        self._native.schedule_parameter_input(parameter_id, time, value)

    def stabilize_physics(self) -> MotionSnapshot:
        """Settle Physics particles at the current parameter values."""
        return self._snapshot(self._native.stabilize_physics())

    def reset(self) -> None:
        self._native.reset()

    def advance(self, dt: float) -> MotionSnapshot:
        return self._snapshot(self._native.advance(dt))

    def seek(self, time: float) -> MotionSnapshot:
        return self._snapshot(self._native.seek(time))

    def seek_with_progress(self, time: float,
                           progress: Callable[[int, int], bool]) -> MotionSnapshot:
        """Replay with completed/total callbacks; false cancels without changing state."""
        return self._snapshot(self._native.seek_with_progress(time, progress))

    def frame(self) -> Evaluation:
        """Evaluate geometry from real parameter values; virtual Part channels remain separate."""
        parameters, drawables = self._native.frame()
        return Evaluation([ParameterSample(*sample) for sample in parameters],
                          [DrawableSample(*sample) for sample in drawables])


class PhysicsPreview:
    """Detached stateful physics rig preview."""

    def __init__(self, native) -> None:
        self._native = native

    @property
    def document_revision(self) -> int:
        return self._native.document_revision()

    def parameters(self) -> dict[str, float]:
        return dict(self._native.parameters())

    def diagnostics(self) -> list[str]:
        return self._native.diagnostics()

    def set_parameter(self, parameter_id: str, value: float) -> None:
        self._native.set_parameter(parameter_id, value)

    def advance(self, dt: float) -> dict[str, float]:
        return dict(self._native.advance(dt))

    def stabilize(self) -> dict[str, float]:
        return dict(self._native.stabilize())

    def reset(self) -> None:
        self._native.reset()
