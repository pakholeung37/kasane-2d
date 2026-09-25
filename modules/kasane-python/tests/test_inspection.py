"""CPU-only validation of raw packet readers (also works with a CPU wheel)."""
from dataclasses import replace
import json
from pathlib import Path
from tempfile import TemporaryDirectory
import unittest
from unittest.mock import patch

from kasane._inspection import InspectionPacket, InspectionView, _parse_json, open_inspection_packet
from kasane._observe import _encode_rgba_png


class PacketValidationTests(unittest.TestCase):
    def packet(self):
        rgba = bytes([0, 0, 0, 0])
        view = InspectionView(
            "view", 0, 1, 1, _encode_rgba_png(1, 1, rgba), rgba,
            (0, 0, 1, 1), (0, 0, 1, 1), (0, 0, 1, 1),
            1.0, (0, 0), "render", "unused", "adapter", "backend",
        )
        return InspectionPacket(
            "capture", "scene", {"capture_id": "capture", "scene_digest": "scene"},
            {"source_kind": "parameters"}, (), (view,), "analysis", {}, {},
        )

    def test_rejects_nonfinite_json_including_exponent_overflow(self):
        for value in (b'NaN', b'Infinity', b'-Infinity', b'1e999', b'-1e999'):
            with self.subTest(value=value), self.assertRaises(ValueError):
                _parse_json(b'{"nested": [' + value + b']}')

    def test_rejects_invalid_saved_mapping(self):
        with TemporaryDirectory() as temp:
            directory = Path(temp) / "packet"
            self.packet().save(directory, profile="report")
            path = directory / "packet.json"
            original = json.loads(path.read_text())
            for key, value in (("view_scale", 0), ("view_scale", -1),
                               ("view_offset", [0]), ("requested_roi", [0, 0, 0, 1])):
                with self.subTest(key=key, value=value):
                    manifest = json.loads(json.dumps(original))
                    manifest["views"][0][key] = value
                    path.write_text(json.dumps(manifest))
                    with self.assertRaises(ValueError):
                        open_inspection_packet(directory)

    def test_checks_remaining_budget_before_reading_next_member(self):
        with TemporaryDirectory() as temp:
            directory = Path(temp) / "packet"
            packet = self.packet()
            replace(packet, views=(packet.views[0], packet.views[0])).save(directory, profile="report")
            real_read = Path.read_bytes
            reads = []

            def track(path):
                reads.append(path.name)
                return real_read(path)

            with patch("kasane._inspection.MAX_PACKET_BYTES", len(packet.views[0].png)), \
                    patch.object(Path, "read_bytes", track), self.assertRaises(ValueError):
                open_inspection_packet(directory)
            self.assertNotIn("001.png", reads)


if __name__ == "__main__":
    unittest.main()
