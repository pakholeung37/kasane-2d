#!/usr/bin/env python3
"""Render Mao's head with O1 capture, then verify saved scene and packet replay.

Run with an installed ``kasane`` wheel built with the ``observe`` feature::

    python tools/run_mao_observe_o1_integration.py

The local Mao model is an optional, untracked test asset. Each run writes a
new directory under ``target/mao-observe-o1`` and leaves earlier runs intact.
"""

from __future__ import annotations

from hashlib import sha256
import json
from pathlib import Path
import subprocess
import sys
from uuid import uuid4

import kasane


ROOT = Path(__file__).resolve().parents[1]
SOURCE = ROOT / "models/local/mao/runtime/mao_pro.model3.json"
OUTPUT = ROOT / "target/mao-observe-o1"
HEAD_ROI = (1300.0, 500.0, 4500.0, 3100.0)
HEAD_RESOLUTION = (1280, 1040)
FULL_RESOLUTION = (1024, 1024)


def digest(data: bytes) -> str:
    return sha256(data).hexdigest()


def main() -> None:
    if not SOURCE.is_file():
        raise FileNotFoundError(f"Mao model not found: {SOURCE}")
    OUTPUT.mkdir(parents=True, exist_ok=True)
    run = OUTPUT / f"run-{uuid4().hex[:12]}"
    run.mkdir()

    session = kasane.Session(str(uuid4()), 64, 64, (32, 32), 1)
    imported = session.import_model3(SOURCE)
    if imported.warnings or session.diagnose_resources():
        raise AssertionError("Mao import reported warnings or missing resources")
    if len(session.mesh_ids()) != 260:
        raise AssertionError("Mao drawable count changed")
    canvas = session.canvas
    full_roi = (0.0, 0.0, canvas.width, canvas.height)
    request = kasane.RawInspectionRequest(HEAD_ROI, HEAD_RESOLUTION)

    with kasane.Observer(*FULL_RESOLUTION, FULL_RESOLUTION[0]) as observer:
        scene = observer.capture_scene(session)
        full = observer.render_scene(scene, roi=full_roi, resolution=FULL_RESOLUTION)
        head = observer.render_scene(scene, roi=HEAD_ROI, resolution=HEAD_RESOLUTION)
        point = (2900.0, 1800.0)
        restored = head.image_to_canvas(head.canvas_to_image(point))
        if any(abs(a - b) > 0.001 for a, b in zip(point, restored)):
            raise AssertionError("Head ROI image/canvas coordinate mapping changed")
        full.frame.save_png(run / "full.png")
        head.frame.save_png(run / "head.png")

        scene_dir = scene.save_scene(run / "scene")
        reopened_scene = observer.open_scene(scene_dir)
        replayed_head = observer.render_scene(
            reopened_scene, roi=HEAD_ROI, resolution=HEAD_RESOLUTION,
        )
        if (reopened_scene.capture_id != scene.capture_id or
                reopened_scene.scene_digest != scene.scene_digest or
                replayed_head.frame.rgba != head.frame.rgba):
            raise AssertionError("Saved scene changed the head render")

        packet = observer.inspect_scene(scene, request=request)
        receipt = packet.save(run / "packet", profile="scene")
        reopened_packet = observer.open(receipt.directory)
        replayed_packet = observer.render(reopened_packet, request=request)
        if (reopened_packet.capture_id != scene.capture_id or
                replayed_packet.views[-1].rgba != head.frame.rgba):
            raise AssertionError("Saved packet changed the head render")

        child = subprocess.run(
            [sys.executable, "-c", """
from hashlib import sha256
from pathlib import Path
import sys
import kasane
request = kasane.RawInspectionRequest((1300.0, 500.0, 4500.0, 3100.0), (1280, 1040))
with kasane.Observer(1024, 1024, 1024) as observer:
    packet = observer.open(Path(sys.argv[1]))
    view = observer.render(packet, request=request).views[-1]
    assert sha256(view.rgba).hexdigest() == sys.argv[2]
print("INDEPENDENT_PACKET_REOPEN_OK")
""", str(receipt.directory), digest(head.frame.rgba)],
            capture_output=True, text=True, timeout=120,
        )
        if child.returncode or "INDEPENDENT_PACKET_REOPEN_OK" not in child.stdout:
            raise AssertionError(f"Independent packet replay failed: {child.stderr}")

        report = {
            "source": str(SOURCE),
            "canvas": canvas._asdict(),
            "mesh_count": len(session.mesh_ids()),
            "capture_id": scene.capture_id,
            "scene_digest": scene.scene_digest,
            "head_render_digest": head.render_digest,
            "full_roi": full_roi,
            "head_roi": HEAD_ROI,
            "full_resolution": FULL_RESOLUTION,
            "head_resolution": HEAD_RESOLUTION,
            "full_png_sha256": digest(full.frame.png),
            "head_png_sha256": digest(head.frame.png),
            "head_rgba_sha256": digest(head.frame.rgba),
            "adapter": {"name": head.frame.adapter_name,
                        "backend": head.frame.adapter_backend},
            "scene_reopen_pixel_equal": True,
            "packet_reopen_pixel_equal": True,
            "independent_packet_reopen_pixel_equal": True,
            "packet_profile": receipt.profile,
            "packet_saved_bytes": receipt.saved_bytes,
        }
        (run / "report.json").write_text(
            json.dumps(report, indent=2, ensure_ascii=False) + "\n", encoding="utf-8",
        )
    print(run)


if __name__ == "__main__":
    main()
