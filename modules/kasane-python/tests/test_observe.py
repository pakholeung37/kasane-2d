"""Run against a wheel built with `maturin build --features observe`."""

from pathlib import Path
import json
import struct
import shutil
from tempfile import TemporaryDirectory
import unittest
import zlib

import kasane


FIXTURES = Path(__file__).resolve().parent / "fixtures"
TEXTURE = FIXTURES / "asymmetric-2x2.png"
SECOND_TEXTURE = FIXTURES / "texture_00.png"
DOCUMENT = "00000000-0000-4000-8000-000000000001"
ASSET = "00000000-0000-4000-8000-000000000002"
MESH = "00000000-0000-4000-8000-000000000003"
MISSING = "00000000-0000-4000-8000-000000000099"
BACKGROUND = "00000000-0000-4000-8000-000000000004"
MASK = "00000000-0000-4000-8000-000000000005"
PART = "00000000-0000-4000-8000-000000000006"
OFFSCREEN = "00000000-0000-4000-8000-000000000007"


class GpuWheelTests(unittest.TestCase):
    def test_oversize_texture_reports_error_and_observer_recovers(self):
        def chunk(kind, data):
            payload = kind + data
            return struct.pack(">I", len(data)) + payload + struct.pack(">I", zlib.crc32(payload))

        width = 32769
        png = (b"\x89PNG\r\n\x1a\n"
               + chunk(b"IHDR", struct.pack(">IIBBBBB", width, 1, 8, 6, 0, 0, 0))
               + chunk(b"IDAT", zlib.compress(b"\x00" + b"\xff\x00\x00\xff" * width))
               + chunk(b"IEND", b""))
        model = kasane.Session(DOCUMENT, 100, 100, (50, 50), 10)
        with model.edit("base") as edit:
            edit.add_png_asset(ASSET, "texture", TEXTURE)
            edit.create_rectangle(MESH, "face", ASSET, (40, 40), (60, 60))
        with kasane.Observer(64, 64, 64) as observer:
            observer.observe(model)
            with TemporaryDirectory() as directory:
                oversize = Path(directory).resolve() / "oversize.png"
                oversize.write_bytes(png)
                with model.edit("oversize") as edit:
                    edit.replace_png_asset(ASSET, "oversize", oversize)
                with self.assertRaises(kasane.ObservationFailure) as failure:
                    observer.observe(model)
                self.assertEqual(failure.exception.code, "TEXTURE_SIZE_LIMIT")
                self.assertEqual(failure.exception.asset_id, ASSET)
                with model.edit("restore") as edit:
                    edit.replace_png_asset(ASSET, "texture", TEXTURE)
                self.assertEqual(observer.observe(model).width, 64)

    def test_focus_crop_keeps_mask_and_offscreen_composition(self):
        model = kasane.Session(DOCUMENT, 100, 100, (50, 50), 10)
        with model.edit("layered scene") as edit:
            edit.add_png_asset(ASSET, "texture", TEXTURE)
            edit.create_part(PART, "group")
            edit.create_rectangle(BACKGROUND, "background", ASSET, (20, 20), (80, 80))
            edit.create_rectangle(MASK, "mask", ASSET, (40, 40), (55, 60))
            edit.create_rectangle(MESH, "target", ASSET, (40, 40), (60, 60))
            edit.set_mesh_part(MESH, PART)
            edit.create_offscreen(kasane.OffscreenSpec(
                OFFSCREEN, "layer", PART, keyforms=[kasane.OffscreenKeyform(0.5)],
            ))
        properties = model.mesh_properties(MESH)
        with model.edit("mask") as edit:
            edit.update_mesh_properties(MESH, kasane.MeshProperties(
                properties.texture_asset_id, properties.appearance,
                properties.draw_order, properties.blend_mode, properties.enabled,
                properties.double_sided, properties.inverted_mask, [MASK],
            ))
        self.assertEqual(model.offscreen_ids(), [OFFSCREEN])
        with kasane.Observer(64, 64, 64) as observer:
            frame = observer.observe(model)
            self.assertEqual(frame.width, 64)
            with TemporaryDirectory() as directory:
                run = observer.observe_run(model, [{}], Path(directory).resolve(), focus=[MESH])
                self.assertEqual(run.output, run.directory)
                report = json.loads(run.report.read_text())
                self.assertEqual(report["status"], "frames_complete")
                self.assertEqual(report["frames"][0]["alpha_convention"],
                                 "premultiplied_no_post_conversion")
                self.assertEqual(report["frames"][0]["background"], "transparent")
                self.assertEqual(len(run.crops), 1)
                self.assertEqual(report["frames"][0]["crops"][0]["object_id"], MESH)

    def test_observer_reuses_device_and_tracks_content(self):
        self.assertTrue(kasane.capabilities()["gpu_observation"])
        model = kasane.Session(DOCUMENT, 100, 100, (50, 50), 10)
        with model.edit("base") as edit:
            edit.add_png_asset(ASSET, "texture", TEXTURE)
            edit.create_rectangle(MESH, "face", ASSET, (40, 40), (60, 60))
        with kasane.Observer(64, 64, 64) as observer:
            first = observer.observe(model)
            self.assertEqual(len(first.rgba), 64 * 64 * 4)
            self.assertTrue(first.png.startswith(b"\x89PNG\r\n\x1a\n"))
            with TemporaryDirectory() as directory:
                destination = Path(directory).resolve() / "first.png"
                first.save_png(destination)
                self.assertEqual(destination.read_bytes(), first.png)
            self.assertTrue(any(first.rgba[i] for i in range(3, len(first.rgba), 4)))
            self.assertEqual(first.version, model.version)
            self.assertEqual(len(first.input_sha256), 64)
            first_revision = first.texture_revisions[0].revision
            with model.edit("move") as edit:
                edit.update_positions(MESH, [0, 1, 2, 3], [
                    (50, 40), (70, 40), (70, 60), (50, 60),
                ])
            second = observer.observe(model)
            self.assertNotEqual(first.rgba, second.rgba)
            self.assertNotEqual(first.input_sha256, second.input_sha256)
            self.assertEqual(second.texture_revisions[0].revision, first_revision)
            with TemporaryDirectory() as directory:
                changed = Path(directory).resolve() / "texture.png"
                shutil.copyfile(SECOND_TEXTURE, changed)
                with model.edit("texture") as edit:
                    edit.replace_png_asset(ASSET, "texture", changed)
                third = observer.observe(model)
                self.assertGreater(third.texture_revisions[0].revision, first_revision)
                self.assertNotEqual(third.texture_revisions[0].sha256,
                                    first.texture_revisions[0].sha256)
                self.assertNotEqual(third.input_sha256, second.input_sha256)
                observer.set_fit_long_side(32)
                fourth = observer.observe(model)
                self.assertNotEqual(third.rgba, fourth.rgba)
                self.assertNotEqual(third.input_sha256, fourth.input_sha256)
                self.assertEqual(third.texture_revisions, fourth.texture_revisions)
                runs = Path(directory).resolve() / "runs"
                run = observer.observe_run(model, [{}, {}], runs, focus=[MESH, MISSING])
                report = json.loads(run.report.read_text(encoding="utf-8"))
                self.assertEqual(report["status"], "frames_complete")
                self.assertEqual(len(report["sdk_binary_sha256"]), 64)
                self.assertEqual(report["frames"][0]["input_sha256"], fourth.input_sha256)
                self.assertEqual(report["frames"][0]["input_sha256"],
                                 report["frames"][1]["input_sha256"])
                self.assertEqual(report["samples"],
                                 json.loads((run.directory / "samples.json").read_text()))
                self.assertEqual(len(report["frames"]), 2)
                self.assertEqual(len(run.frames), 2)
                self.assertEqual(len(run.crops), 2)
                self.assertTrue(run.contact_sheet.read_bytes().startswith(b"\x89PNG"))
                crop = report["frames"][0]["crops"][0]
                self.assertEqual(crop["object_id"], MESH)
                self.assertEqual(crop["bounds"], list(fourth.drawable_bounds[0].bounds))
                crop_png = run.crops[0].read_bytes()
                crop_width, crop_height = struct.unpack_from(">II", crop_png, 16)
                self.assertEqual((crop_width, crop_height), (
                    crop["bounds"][2] - crop["bounds"][0],
                    crop["bounds"][3] - crop["bounds"][1],
                ))
                compressed = bytearray()
                offset = 8
                while offset < len(crop_png):
                    length = struct.unpack_from(">I", crop_png, offset)[0]
                    kind = crop_png[offset + 4:offset + 8]
                    if kind == b"IDAT":
                        compressed.extend(crop_png[offset + 8:offset + 8 + length])
                    offset += 12 + length
                decoded = zlib.decompress(compressed)
                x0, y0, x1, y1 = crop["bounds"]
                expected_rows = b"".join(
                    b"\0" + fourth.rgba[y * fourth.width * 4 + x0 * 4:
                                           y * fourth.width * 4 + x1 * 4]
                    for y in range(y0, y1)
                )
                self.assertEqual(decoded, expected_rows)
                diagnostics = json.loads((run.directory / "diagnostics.json").read_text())
                self.assertEqual(diagnostics[0]["code"], "FOCUS_NOT_FOUND")
                self.assertEqual(len(json.loads((run.directory / "samples.json").read_text())), 2)
                changed.unlink()
                with self.assertRaises(kasane.ObservationFailure) as error:
                    observer.observe(model)
                self.assertEqual(error.exception.code, "PROJECT_IO")
                self.assertEqual(error.exception.asset_id, ASSET)
                with self.assertRaises(kasane.ObservationFailure) as failed_run:
                    observer.observe_run(model, [{}], runs)
                failed_report = json.loads(
                    (failed_run.exception.run_directory / "report.json").read_text()
                )
                self.assertEqual(failed_report["status"], "failed")
                self.assertEqual(failed_report["frames"], [])


if __name__ == "__main__":
    unittest.main()
