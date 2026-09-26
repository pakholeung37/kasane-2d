"""Measure user-visible Observe phases on the installed wheel and a fixed fixture.

Run this in an isolated environment with an Observe wheel and its inspection
extra installed. Timings include Python/native boundary overhead. Rust/GPU
allocation and decode/upload subphases are not separately instrumented.
"""

from __future__ import annotations

import argparse
import gc
import hashlib
import json
from pathlib import Path
import platform
import resource
from statistics import median
from tempfile import TemporaryDirectory
from time import perf_counter_ns
import tracemalloc

import kasane
from kasane import _native


ROOT = Path(__file__).resolve().parents[1]
PROJECT = ROOT / "tests/fixtures/observe_inspection/projects/pixel-binding.kasane.json"


def ms(fn):
    started = perf_counter_ns()
    value = fn()
    return value, round((perf_counter_ns() - started) / 1_000_000, 3)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--repetitions", type=int, default=5)
    args = parser.parse_args()
    if not args.output.is_absolute() or not 3 <= args.repetitions <= 20:
        parser.error("Use an absolute output path and 3–20 repetitions")
    session = kasane.open_project(PROJECT.resolve())
    values = {"Shift": 0.25}
    clean_request = kasane.InspectionRequest(
        view=kasane.ViewSpec(resolution=(128, 128)), channels=("clean",),
    )
    labels_request = kasane.InspectionRequest(
        view=kasane.ViewSpec(resolution=(128, 128)), channels=("clean", "labels"),
    )
    samples = {name: [] for name in (
        "legacy_observe", "capture_including_decode", "raw_render_readback",
        "clean_inspect", "clean_plus_labels_inspect", "report_profile_save",
    )}
    tracemalloc.start()
    start_rss = resource.getrusage(resource.RUSAGE_SELF).ru_maxrss
    with TemporaryDirectory(prefix="kasane-o7-benchmark-") as temporary:
        with kasane.Observer(128, 128, 128) as observer:
            # Warm GPU pipeline creation and Python import paths.
            observer.observe(session, values)
            for index in range(args.repetitions):
                _, elapsed = ms(lambda: observer.observe(session, values))
                samples["legacy_observe"].append(elapsed)
                scene, elapsed = ms(lambda: observer.capture_scene(session, values))
                samples["capture_including_decode"].append(elapsed)
                _, elapsed = ms(lambda: observer.render_scene(
                    scene, roi=(0, 0, 100, 100), resolution=(128, 128)))
                samples["raw_render_readback"].append(elapsed)
                clean, elapsed = ms(lambda: observer.inspect_scene(
                    scene, request=clean_request))
                samples["clean_inspect"].append(elapsed)
                annotated, elapsed = ms(lambda: observer.inspect_scene(
                    scene, request=labels_request))
                samples["clean_plus_labels_inspect"].append(elapsed)
                _, elapsed = ms(lambda: annotated.save(
                    Path(temporary) / f"packet-{index}", profile="report"))
                samples["report_profile_save"].append(elapsed)
                clean.close()
                annotated.close()
            gc.collect()
            after_close_rss = resource.getrusage(resource.RUSAGE_SELF).ru_maxrss
            # Exercise repeated capture and packet teardown after the measured run.
            for _ in range(20):
                packet = observer.inspect(session, values, request=clean_request)
                packet.close()
            gc.collect()
            after_repeated_close_rss = resource.getrusage(
                resource.RUSAGE_SELF).ru_maxrss
    python_current, python_peak = tracemalloc.get_traced_memory()
    tracemalloc.stop()
    report = {
        "schema_version": 1,
        "platform": platform.platform(),
        "native_sha256": hashlib.sha256(Path(_native.__file__).read_bytes()).hexdigest(),
        "fixture_sha256": hashlib.sha256(PROJECT.read_bytes()).hexdigest(),
        "resolution": [128, 128],
        "repetitions": args.repetitions,
        "timings_ms": {name: {"runs": runs, "median": median(runs)}
                       for name, runs in samples.items()},
        "decode_upload_ms": None,
        "decode_upload_reason": "Capture includes texture decode; upload is inside renderer and has no separate timer",
        "memory": {
            "peak_process_rss_before_bytes": start_rss,
            "peak_process_rss_after_close_bytes": after_close_rss,
            "peak_process_rss_after_20_more_closes_bytes": after_repeated_close_rss,
            "python_tracemalloc_current_bytes": python_current,
            "python_tracemalloc_peak_bytes": python_peak,
            "rss_note": "ru_maxrss is a process high-water mark, not live GPU or Rust allocation",
        },
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2, allow_nan=False) + "\n",
                           encoding="utf-8")
    print(args.output)


if __name__ == "__main__":
    main()
