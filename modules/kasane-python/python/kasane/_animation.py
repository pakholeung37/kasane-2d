"""Detached CPU Motion, Expression and Physics previews."""
from __future__ import annotations

from collections.abc import Callable
import json

from ._types import DrawableSample, Evaluation, ExpressionSnapshot, MotionSnapshot, ParameterSample, SeekCacheStats


class ExpressionPreview:
    """Replay expressions without modifying the authoring session.

    The preview captures the document when created. Create a new preview after
    editing expression assets or model parameters.
    """

    def __init__(self, native) -> None:
        self._native = native

    @property
    def document_revision(self) -> int:
        """Return the revision of the captured document, not the live session."""
        return self._native.document_revision()

    def snapshot(self) -> ExpressionSnapshot:
        """Return detached current values without advancing playback; keys are parameter UUIDs."""
        time, values, active = self._native.snapshot()
        return ExpressionSnapshot(time, dict(values), active)

    def set_base_parameter(self, parameter_id: str, value: float) -> None:
        """Set a finite parameter UUID baseline, clamped to its range, then reset.

        The activation schedule is retained. Missing UUIDs or nonfinite values raise
        SdkFailure with EVALUATION_FAILED without changing playback.
        """
        self._native.set_base_parameter(parameter_id, value)

    def schedule_expression(self, expression_id: str, time: float) -> None:
        """Schedule an expression UUID at an absolute preview time in seconds.

        Time must be finite, nonnegative and no earlier than the current time.
        Equal-time activations preserve call order and start on the first update
        at or after their scheduled time. SdkFailure codes: INVALID_TIME,
        PAST_ACTIVATION, MISSING_EXPRESSION, UNRESOLVED_PARAMETER.
        Failure leaves the schedule and playback unchanged.
        """
        self._native.schedule_expression(expression_id, time)

    def reset(self) -> None:
        """Restart time and active state while retaining the activation schedule."""
        self._native.reset()

    def advance(self, dt: float) -> ExpressionSnapshot:
        """Advance by finite, nonnegative seconds and return an ExpressionSnapshot.

        Zero is allowed and evaluates activations due now. Invalid dt or clock
        overflow raises SdkFailure with INVALID_TIME without changing state.
        """
        time, values, active = self._native.advance(dt)
        return ExpressionSnapshot(time, dict(values), active)

    def seek(self, time: float) -> ExpressionSnapshot:
        """Replay from the baseline on the absolute 60 Hz grid to time in seconds.

        Replay begins with a zero-length step at time zero, then includes grid
        steps and a partial final step when needed.
        Schedule and baseline are retained. Nonfinite/negative time raises
        INVALID_TIME; time * 60 above one million raises SEEK_LIMIT. Validation
        occurs before resetting playback. This standalone preview has no cache.
        """
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
        """Return the revision of the captured document, not the live session."""
        return self._native.document_revision()

    @staticmethod
    def _snapshot(raw) -> MotionSnapshot:
        time, parameters, parts, part_opacities, opacity, active, expressions, events, coverage = raw
        return MotionSnapshot(time, dict(parameters), dict(parts), dict(part_opacities), opacity, active, expressions, events, coverage)

    def snapshot(self) -> MotionSnapshot:
        """Return detached current values and last-update events without advancing playback."""
        return self._snapshot(self._native.snapshot())

    @property
    def operation(self) -> dict:
        """Last successful operation identity; this is not a replay recipe."""
        return json.loads(self._native.operation_json())

    def set_base_parameter(self, parameter_id: str, value: float) -> None:
        """Set a finite parameter UUID baseline, clamped to its range, then reset.

        Retains activation/input schedules; clears seek checkpoints and statistics.
        Missing UUIDs or nonfinite values raise EVALUATION_FAILED without mutation.
        """
        self._native.set_base_parameter(parameter_id, value)

    def schedule_motion_entry(self, group: str, index: int, time: float) -> None:
        """Schedule a model3 entry at an absolute preview time in seconds.

        ``group`` is an exact name; ``index`` is a nonnegative, zero-based entry
        index. ``time`` must be finite, nonnegative and at least the current
        preview time. Parameter-curve fades override registration fades, which
        override clip fades (default one second). Sound is not played.

        Success clears seek checkpoints/statistics without resetting playback.
        SdkFailure codes are MISSING_MOTION_ENTRY for unknown groups/indices,
        INVALID_TIME for invalid time and PAST_ACTIVATION for past time.
        Unresolved imported tracks are skipped and reported in coverage. Failures preserve
        playback, schedules and cache. Negative or oversized indices raise
        OverflowError during conversion to the native unsigned integer.
        """
        self._native.schedule_motion_entry(group, index, time)

    def set_seek_cache_budget(self, size_bytes: int) -> None:
        """Set the retained checkpoint budget in bytes (default 16 MiB).

        Every call clears checkpoints and seek statistics, even for an unchanged
        budget; zero disables caching. Playback and schedules are preserved.
        Conservative estimates exclude shared document/curve data and temporary
        seek state. Oldest inserted checkpoints are evicted first; oversized
        checkpoints are skipped. Negative or oversized sizes raise OverflowError
        during conversion to the native unsigned integer, without changing state.
        """
        self._native.set_seek_cache_budget(size_bytes)

    def clear_seek_cache(self) -> None:
        """Discard checkpoints and zero seek statistics, retaining the budget.

        Playback, baseline values and schedules are unchanged.
        """
        self._native.clear_seek_cache()

    def seek_cache_stats(self) -> SeekCacheStats:
        """Return immutable cache usage and last successful seek statistics.

        Reading statistics does not mutate playback or the cache. See
        SeekCacheStats for field units and reset semantics.
        """
        return SeekCacheStats(*self._native.seek_cache_stats())

    def schedule_motion(self, motion_id: str, time: float) -> None:
        """Schedule a clip UUID at an absolute preview time in seconds.

        Uses clip fades and parameter-curve overrides; use schedule_motion_entry
        for model3 registration fades. Time must be finite, nonnegative and no
        earlier than the current time. Equal-time activations keep call order and
        start on the first update at or after their scheduled time.
        SdkFailure codes: INVALID_TIME, PAST_ACTIVATION, MISSING_MOTION.
        Unresolved imported tracks are skipped and reported in coverage.
        Success clears seek checkpoints/statistics;
        failure preserves playback, schedules and cache.
        """
        self._native.schedule_motion(motion_id, time)

    def schedule_expression(self, expression_id: str, time: float) -> None:
        """Schedule an expression after Motion and before Physics at time in seconds.

        Time must be finite, nonnegative and at least the current time; equal-time
        activations keep call order. SdkFailure codes: INVALID_TIME,
        PAST_ACTIVATION, MISSING_EXPRESSION, UNRESOLVED_PARAMETER. Success clears
        seek checkpoints/statistics; failure preserves playback and cache.
        """
        self._native.schedule_expression(expression_id, time)

    def schedule_parameter_input(self, parameter_id: str, time: float, value: float) -> None:
        """Schedule a parameter UUID override at an absolute time in seconds.

        Finite values are clamped to the parameter range. Time must be finite,
        nonnegative and at least the current time; equal-time inputs keep call
        order and are applied before Motion on the first update at or after time.
        Reset and seek retain the input history. Success clears checkpoints and
        statistics. Invalid time/value raises INVALID_TIME, past time raises
        PAST_ACTIVATION, and a missing UUID raises EVALUATION_FAILED.
        """
        self._native.schedule_parameter_input(parameter_id, time, value)

    def stabilize_physics(self) -> MotionSnapshot:
        """Settle Physics particles at the current parameter values."""
        return self._snapshot(self._native.stabilize_physics())

    def reset(self) -> None:
        """Restore time zero, baseline values and initial Motion/Expression/Physics/Pose state.

        Retains activation/input schedules and cache budget, clears checkpoints,
        seek statistics and fired events. Does not evaluate time-zero activations;
        advance(0) or seek(0) does that.
        """
        self._native.reset()

    def advance(self, dt: float) -> MotionSnapshot:
        """Advance Motion, Expression, Physics and Pose by finite, nonnegative seconds.

        Returns a detached MotionSnapshot. Zero evaluates activations due now.
        INVALID_TIME rejects invalid dt or clock overflow; EVENT_LIMIT rejects an
        estimated batch over one million events. These errors preserve playback.
        Arbitrary advance calls do not populate canonical seek checkpoints.
        """
        return self._snapshot(self._native.advance(dt))

    def seek(self, time: float) -> MotionSnapshot:
        """Replay to absolute time in seconds from a canonical checkpoint or baseline.

        Uses the absolute 60 Hz grid with a partial final step; repeated seeks with
        the same baseline/schedules return identical snapshots. Cold replay begins
        with a zero-length step at time zero. INVALID_TIME rejects nonfinite/negative time;
        SEEK_LIMIT rejects time * 60 above one million, even with a warm cache.
        Failed seeks preserve playback and cache. Events are returned as data;
        seeking does not dispatch audio or other event side effects.
        """
        return self._snapshot(self._native.seek(time))

    def seek_with_progress(self, time: float,
                           progress: Callable[[int, int], bool]) -> MotionSnapshot:
        """Report remaining replay steps; exact cache hits report (0, 0).

        False or an exception cancels without changing playback or cache state.
        """
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
        """Return the revision of the captured document, not the live session."""
        return self._native.document_revision()

    def parameters(self) -> dict[str, float]:
        """Return a detached parameter-UUID to current-value mapping without advancing physics."""
        return dict(self._native.parameters())

    def diagnostics(self) -> list[str]:
        """Return a copy of coverage messages from the last advance; reset clears them."""
        return self._native.diagnostics()

    def set_parameter(self, parameter_id: str, value: float) -> None:
        """Set a parameter UUID value, clamped to its range, without resetting particles.

        A missing UUID raises EVALUATION_FAILED; a nonfinite value raises
        INVALID_TIME. Failure leaves state unchanged. Reset restores document
        defaults, so this input is not retained as a baseline or scheduled input.
        """
        self._native.set_parameter(parameter_id, value)

    def advance(self, dt: float) -> dict[str, float]:
        """Advance physics by finite, nonnegative seconds and return parameter values.

        Uses the physics asset's fixed FPS when present; otherwise uses the supplied
        delta. Refreshes diagnostics. Invalid dt raises INVALID_TIME without
        changing state. No physics asset leaves parameter values unchanged.
        """
        return dict(self._native.advance(dt))

    def stabilize(self) -> dict[str, float]:
        """Initialize particles and physics outputs from current parameter values.

        Returns a detached parameter mapping without advancing a preview clock.
        This does not run Motion, Expression or Pose, and does not refresh diagnostics.
        """
        return dict(self._native.stabilize())

    def reset(self) -> None:
        """Restore document-default parameters and initial particles/caches; clear diagnostics.

        Previously set parameter inputs are discarded; the captured document remains unchanged.
        """
        self._native.reset()
