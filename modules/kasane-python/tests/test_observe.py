"""Run against a wheel built with `maturin build --features observe`."""

from pathlib import Path
import json
import subprocess
import struct
import shutil
import sys
from tempfile import TemporaryDirectory
import unittest
import zlib

import kasane


FIXTURES = Path(__file__).resolve().parent / "fixtures"
INSPECTION_FIXTURES = Path(__file__).resolve().parents[3] / "tests/fixtures/observe_inspection/projects"
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
PART_B = "00000000-0000-4000-8000-000000000008"
POSE = "00000000-0000-4000-8000-000000000009"
PARAMETER = "00000000-0000-4000-8000-000000000010"


class GpuWheelTests(unittest.TestCase):
    def test_o2_background_composes_mask_and_nested_offscreen(self):
        nested = kasane.Session(DOCUMENT, 100, 100, (50, 50), 10)
        with nested.edit("nested surfaces") as edit:
            edit.add_png_asset(ASSET, "texture", TEXTURE)
            edit.create_part(PART, "outer")
            edit.create_part(PART_B, "inner", PART)
            edit.create_rectangle(MESH, "target", ASSET, (30, 30), (70, 70))
            edit.set_mesh_part(MESH, PART_B)
            edit.create_offscreen(kasane.OffscreenSpec(
                OFFSCREEN, "outer surface", PART,
                keyforms=[kasane.OffscreenKeyform(0.75)],
            ))
            edit.create_offscreen(kasane.OffscreenSpec(
                POSE, "inner surface", PART_B,
                keyforms=[kasane.OffscreenKeyform(0.5)],
            ))
        masked = kasane.open_project(
            (INSPECTION_FIXTURES / "masked-offscreen.kasane.json").resolve(),
        )
        with kasane.Observer(128, 128, 128) as observer:
            for model in (nested, masked):
                roi = (0, 0, 100, 100)
                raw = observer.inspect(model, request=kasane.RawInspectionRequest(
                    roi, (128, 128),
                )).views[0].rgba
                self.assertTrue(any(0 < value < 255 for value in raw[3::4]))
                for background, channel in (("light", 238), ("dark", 32)):
                    packet = observer.inspect(model, request=kasane.InspectionRequest(
                        view=kasane.ViewSpec(roi=roi, resolution=(128, 128)),
                        presentation=kasane.PresentationSpec(background=background),
                        channels=("clean", "alpha"),
                    ))
                    clean, alpha_view = packet.views
                    self.assertTrue(all(value == 255 for value in clean.rgba[3::4]))
                    for offset in range(0, len(raw), 4):
                        alpha = raw[offset + 3]
                        self.assertEqual(alpha_view.rgba[offset], alpha)
                        for color in range(3):
                            expected = min(255, raw[offset + color] +
                                           (channel * (255 - alpha) + 127) // 255)
                            self.assertLessEqual(abs(clean.rgba[offset + color] - expected), 2)
            focused = observer.inspect(nested, request=kasane.InspectionRequest(
                focus=kasane.Focus(mesh_ids=(MESH,)),
                view=kasane.ViewSpec(resolution=(64, 64)), channels=("clean",),
            ))
            self.assertEqual(focused.objects[0]["composition_path"],
                             (OFFSCREEN, POSE))

    def test_o2_rotated_focus_and_high_resolution_rerender(self):
        rotated = kasane.open_project((INSPECTION_FIXTURES / "rotated-warp.kasane.json").resolve())
        warped_id = "10000000-0000-4000-8000-000000000017"
        line_id = "10000000-0000-4000-8000-000000000016"
        with kasane.Observer(64, 64, 64) as observer:
            packet = observer.inspect(rotated, request=kasane.InspectionRequest(
                focus=kasane.Focus(mesh_ids=(warped_id,)),
                view=kasane.ViewSpec(resolution=(192, 96), padding_canvas=2),
                channels=("clean",),
            ))
            roi = packet.views[0].requested_roi
            self.assertNotEqual(roi, (20.0, 20.0, 60.0, 60.0))
            self.assertEqual(packet.views[0].runtime_to_canvas((0, 0)), (37.0, 62.0))
            self.assertEqual(packet.focus_status, "visible")

            model = kasane.open_project((INSPECTION_FIXTURES / "pixel-binding.kasane.json").resolve())
            scene = observer.capture_scene(model)
            low = observer.render_scene(scene, roi=(60, 20, 85, 45),
                                        resolution=(24, 24)).frame.rgba
            high = observer.render_scene(scene, roi=(60, 20, 85, 45),
                                         resolution=(192, 192)).frame.rgba
            nearest = bytearray(len(high))
            for y in range(192):
                for x in range(192):
                    nearest[(y * 192 + x) * 4:(y * 192 + x + 1) * 4] = low[
                        ((y // 8) * 24 + x // 8) * 4:
                        ((y // 8) * 24 + x // 8 + 1) * 4]
            self.assertNotEqual(high, bytes(nearest))
            self.assertIn(line_id, {row["id"] for row in
                                   observer.inspect_scene(scene, request=kasane.InspectionRequest(
                                       view=kasane.ViewSpec(roi=(60, 20, 85, 45), resolution=(96, 96)),
                                       channels=("clean",),
                                   )).objects})

    def test_o2_straight_alpha_rounding_and_empty_focus(self):
        model = kasane.Session(DOCUMENT, 100, 100, (13, 71), 7)
        with model.edit("transparent texture") as edit:
            edit.add_png_asset(ASSET, "texture", TEXTURE)
            edit.create_rectangle(MESH, "edge", ASSET, (35, 35), (65, 65))
            edit.create_part(PART, "empty")
        with kasane.Observer(64, 64, 64) as observer:
            raw = observer.inspect(model, request=kasane.RawInspectionRequest(
                (0, 0, 100, 100), (64, 64),
            )).views[0].rgba
            straight = observer.inspect(model, request=kasane.InspectionRequest(
                view=kasane.ViewSpec(roi=(0, 0, 100, 100), resolution=(64, 64)),
                presentation=kasane.PresentationSpec(
                    background="transparent", alpha="straight",
                ), channels=("clean",),
            )).views[0]
            self.assertTrue(any(0 < alpha < 255 for alpha in raw[3::4]))
            for offset in range(0, len(raw), 4):
                alpha = raw[offset + 3]
                self.assertEqual(straight.rgba[offset + 3], alpha)
                for channel in range(3):
                    expected = min(255, (raw[offset + channel] * 255 + alpha // 2) // alpha) if alpha else 0
                    self.assertEqual(straight.rgba[offset + channel], expected)
            empty = observer.inspect(model, request=kasane.InspectionRequest(
                focus=kasane.Focus(part_ids=(PART,)),
                view=kasane.ViewSpec(roi=(-10, -10, 10, 10), resolution=(48, 24)),
                channels=("clean",),
            ))
            self.assertEqual(empty.focus_status, "empty")
            self.assertEqual(empty.views[0].runtime_to_canvas((0, 0)), (13, 71))
            with self.assertRaises(kasane.ObservationFailure) as unknown:
                observer.inspect(model, request=kasane.InspectionRequest(
                    focus=kasane.Focus(mesh_ids=(MISSING,)),
                    view=kasane.ViewSpec(resolution=(32, 32)),
                    channels=("clean",),
                ))
            self.assertEqual(unknown.exception.code, "UNKNOWN_FOCUS")
            with self.assertRaises(kasane.ObservationFailure) as auto_empty:
                observer.inspect(model, request=kasane.InspectionRequest(
                    focus=kasane.Focus(part_ids=(PART,)),
                    view=kasane.ViewSpec(resolution=(32, 32)),
                    channels=("clean",),
                ))
            self.assertEqual(auto_empty.exception.code, "EMPTY_FOCUS")

    def test_o2_fixed_union_and_follow_use_frozen_sample_geometry(self):
        model = kasane.Session(DOCUMENT, 100, 100, (50, 50), 10)
        with model.edit("moving mesh") as edit:
            edit.add_png_asset(ASSET, "texture", TEXTURE)
            edit.create_rectangle(MESH, "moving", ASSET, (40, 40), (60, 60))
        base = model.mesh(MESH).positions
        with model.edit("bind movement") as edit:
            edit.create_parameter(PARAMETER, "shift", 0, 1, 0)
            edit.create_mesh_binding(
                POSE, MESH, [kasane.Axis(PARAMETER, [0, 1])],
                [kasane.MeshKeyform([0], base),
                 kasane.MeshKeyform([1], [(x + 10, y) for x, y in base])],
            )
        with kasane.Observer(64, 64, 64) as observer:
            common = dict(focus=kasane.Focus(mesh_ids=(MESH,)),
                          presentation=kasane.PresentationSpec(background="dark"),
                          channels=("clean",))
            fixed = observer.inspect_samples(model, [{PARAMETER: 0}, {PARAMETER: 1}],
                request=kasane.InspectionRequest(
                    view=kasane.ViewSpec(resolution=(120, 80), framing="fixed_union"),
                    **common,
                ))
            self.assertEqual(fixed[0].capture_id, fixed[1].capture_id)
            self.assertEqual(fixed[0].views[0].requested_roi,
                             fixed[1].views[0].requested_roi)
            self.assertEqual(fixed[0].views[0].requested_roi, (40, 40, 70, 60))
            self.assertEqual(fixed[0].views[0].canvas_to_image_matrix,
                             fixed[1].views[0].canvas_to_image_matrix)
            self.assertEqual(fixed[0].objects[0]["mark"],
                             fixed[1].objects[0]["mark"])
            self.assertEqual(fixed[1].views[0].sample_index, 1)
            follow = observer.inspect_samples(model, [{PARAMETER: 0}, {PARAMETER: 1}],
                request=kasane.InspectionRequest(
                    view=kasane.ViewSpec(resolution=(120, 80), framing="follow"),
                    **common,
                ))
            self.assertEqual(follow[0].views[0].requested_roi, (40, 40, 60, 60))
            self.assertEqual(follow[1].views[0].requested_roi, (50, 40, 70, 60))
            self.assertNotEqual(follow[0].views[0].canvas_to_image_matrix,
                                follow[1].views[0].canvas_to_image_matrix)

    def test_o2_presentation_focus_labels_alpha_and_profile_reopen(self):
        model = kasane.Session(DOCUMENT, 100, 100, (50, 50), 10)
        with model.edit("presentation fixture") as edit:
            edit.add_png_asset(ASSET, "texture", TEXTURE)
            edit.create_part(PART, "面部")
            edit.create_rectangle(MESH, "同名", ASSET, (40, 40), (60, 60))
            edit.create_rectangle(BACKGROUND, "同名", ASSET, (70, 70), (80, 80))
            edit.set_mesh_part(MESH, PART)
        request = kasane.InspectionRequest(
            focus=kasane.Focus(part_ids=(PART,)),
            view=kasane.ViewSpec(resolution=(160, 128), padding_canvas=8),
            channels=("clean", "labels", "alpha"),
        )
        with kasane.Observer(64, 64, 64) as observer:
            packet = observer.inspect(model, request=request)
            self.assertEqual([view.kind for view in packet.views],
                             ["clean", "labels", "alpha"])
            self.assertEqual(packet.views[0].requested_roi, (40, 40, 60, 60))
            self.assertTrue(packet.capabilities["presentation"])
            self.assertFalse(packet.capabilities["raw_view"])
            self.assertTrue(all(packet.views[0].rgba[i] == 255 for i in
                                range(3, len(packet.views[0].rgba), 4)))
            self.assertNotEqual(packet.views[0].png, packet.views[1].png)
            self.assertEqual(packet.views[2].presentation["alpha_source"],
                             "transparent_raw_pass")
            self.assertTrue(any(packet.views[2].rgba[i] < 255 for i in
                                range(0, len(packet.views[2].rgba), 4)))
            meshes = {row["id"]: row for row in packet.objects if row["kind"] == "mesh"}
            self.assertEqual(meshes[MESH]["name"], meshes[BACKGROUND]["name"])
            self.assertNotEqual(meshes[MESH]["mark"], meshes[BACKGROUND]["mark"])
            self.assertEqual(meshes[MESH]["part_path"], (PART,))
            self.assertEqual(meshes[MESH]["coverage_status"], "unknown")
            point = (47.25, 51.75)
            mapped = packet.views[0].canvas_to_image(point)
            restored = packet.views[0].image_to_canvas(mapped)
            self.assertAlmostEqual(restored[0], point[0], places=4)
            self.assertAlmostEqual(restored[1], point[1], places=4)
            self.assertEqual(packet.views[0].runtime_to_canvas((-1.0, 1.0)),
                             (40.0, 40.0))
            self.assertEqual(packet.views[0].canvas_to_runtime((40.0, 40.0)),
                             (-1.0, 1.0))
            matrix = packet.views[0].canvas_to_image_matrix
            self.assertAlmostEqual(matrix[0][0] * point[0] + matrix[0][2], mapped[0])
            with TemporaryDirectory() as temporary:
                directory = Path(temporary).resolve() / "packet"
                packet.save(directory, profile="analysis")
                reopened = observer.open(directory)
                self.assertEqual([view.kind for view in reopened.views],
                                 ["clean", "labels", "alpha"])
                self.assertEqual(reopened.views[1].png, packet.views[1].png)
                self.assertEqual(reopened.focus_status, "visible")
                self.assertEqual(reopened.views[0].canvas, packet.views[0].canvas)
                self.assertEqual(reopened.objects, tuple(json.loads(
                    json.dumps(packet.objects))))
                scene_directory = Path(temporary).resolve() / "scene-packet"
                packet.save(scene_directory, profile="scene")
                child = subprocess.run(
                    [sys.executable, "-c", """
import kasane, sys
from pathlib import Path
with kasane.Observer(64, 64, 64) as observer:
    packet = observer.open(Path(sys.argv[1]))
    assert packet.capabilities['presentation']
    again = observer.render(packet, request=kasane.InspectionRequest(
        view=kasane.ViewSpec(roi=(40, 40, 60, 60), resolution=(80, 64)),
        channels=('clean',),
    ))
    assert again.capture_id == packet.capture_id
    assert again.views[-1].kind == 'clean'
print('O2_SCENE_REOPEN_OK')
""", str(scene_directory)], capture_output=True, text=True,
                )
                self.assertEqual(child.returncode, 0, child.stderr)
                self.assertIn("O2_SCENE_REOPEN_OK", child.stdout)

    def test_o2_checker_and_special_transparent_failure_recovery(self):
        model = kasane.Session(DOCUMENT, 4, 4, (2, 2), 1)
        with kasane.Observer(4, 4, 4) as observer:
            raw = observer.observe(model).rgba
            request = kasane.InspectionRequest(
                view=kasane.ViewSpec(resolution=(4, 4)),
                presentation=kasane.PresentationSpec(
                    background="checker", light_rgb=(240, 240, 240),
                    dark_rgb=(16, 16, 16), checker_tile_px=2,
                ),
                channels=("clean",),
            )
            packet = observer.inspect(model, request=request)
            pixels = packet.views[0].rgba
            self.assertEqual(pixels[:4], bytes((240, 240, 240, 255)))
            self.assertEqual(pixels[8:12], bytes((16, 16, 16, 255)))
            self.assertEqual(observer.observe(model).rgba, raw)
            with self.assertRaises(ValueError):
                kasane.PresentationSpec(background="transparent")

        layered = kasane.Session(DOCUMENT, 100, 100, (50, 50), 10)
        with layered.edit("special blend") as edit:
            edit.add_png_asset(ASSET, "texture", TEXTURE)
            edit.create_rectangle(MESH, "additive", ASSET, (30, 30), (70, 70))
        properties = layered.mesh_properties(MESH)
        with layered.edit("set additive") as edit:
            edit.update_mesh_properties(MESH, kasane.MeshProperties(
                properties.texture_asset_id, properties.appearance,
                properties.draw_order, "additive", properties.enabled,
                properties.double_sided, properties.inverted_mask,
                properties.masks,
            ))
        with kasane.Observer(64, 64, 64) as observer:
            transparent = kasane.InspectionRequest(
                view=kasane.ViewSpec(resolution=(64, 64)),
                presentation=kasane.PresentationSpec(
                    background="transparent", alpha="straight",
                ), channels=("clean",),
            )
            with self.assertRaises(kasane.ObservationFailure) as failure:
                observer.inspect(layered, request=transparent)
            self.assertEqual(failure.exception.code,
                             "UNREPRESENTABLE_TRANSPARENT_OUTPUT")
            light = observer.inspect(layered, request=kasane.InspectionRequest(
                view=kasane.ViewSpec(resolution=(64, 64)), channels=("clean",),
            ))
            dark = observer.inspect(layered, request=kasane.InspectionRequest(
                view=kasane.ViewSpec(resolution=(64, 64)),
                presentation=kasane.PresentationSpec(background="dark"),
                channels=("clean",),
            ))
            self.assertNotEqual(light.views[0].rgba, dark.views[0].rgba)

    def test_o2_label_limit_records_omissions_without_changing_clean(self):
        model = kasane.Session(DOCUMENT, 100, 100, (50, 50), 10)
        with model.edit("two labels") as edit:
            edit.add_png_asset(ASSET, "texture", TEXTURE)
            edit.create_rectangle(MESH, "左", ASSET, (20, 20), (30, 30))
            edit.create_rectangle(BACKGROUND, "右", ASSET, (60, 60), (70, 70))
        with kasane.Observer(64, 64, 64) as observer:
            request = kasane.InspectionRequest(
                view=kasane.ViewSpec(resolution=(256, 256)),
                overlay=kasane.OverlaySpec(max_labels=1),
                channels=("clean", "labels"),
            )
            packet = observer.inspect(model, request=request)
            labels = packet.views[1]
            self.assertEqual(len(labels.presentation["labels"]), 1)
            self.assertTrue(any(item["reason"] == "label_limit" for item in
                                labels.omitted_labels))
            self.assertNotEqual(labels.rgba, packet.views[0].rgba)
            second = observer.inspect(model, request=request)
            self.assertEqual([row["mark"] for row in packet.objects if row["kind"] == "mesh"],
                             [row["mark"] for row in second.objects if row["kind"] == "mesh"])
            properties = model.mesh_properties(BACKGROUND)
            with model.edit("hide second mesh") as edit:
                edit.update_mesh_properties(BACKGROUND, kasane.MeshProperties(
                    properties.texture_asset_id, properties.appearance,
                    properties.draw_order, properties.blend_mode, False,
                    properties.double_sided, properties.inverted_mask,
                    properties.masks,
                ))
            hidden = observer.inspect(model, request=kasane.InspectionRequest(
                focus=kasane.Focus(mesh_ids=(BACKGROUND,)),
                view=kasane.ViewSpec(roi=(0, 0, 100, 100), resolution=(256, 256)),
                channels=("clean", "labels"),
            ))
            rows = {row["id"]: row for row in hidden.objects if row["kind"] == "mesh"}
            self.assertEqual(rows[BACKGROUND]["mark"],
                             {row["id"]: row["mark"] for row in packet.objects
                              if row["kind"] == "mesh"}[BACKGROUND])
            self.assertFalse(rows[BACKGROUND]["enabled"])
            self.assertIn(hidden.focus_status, ("not_visible", "empty"))
            self.assertTrue(any(item["object_id"] == BACKGROUND for item in
                                hidden.views[1].omitted_labels))

    def test_o2_dense_marks_and_subpixel_overscan_mapping(self):
        model = kasane.Session(DOCUMENT, 100, 100, (13, 71), 7)
        with model.edit("dense marks") as edit:
            edit.add_png_asset(ASSET, "texture", TEXTURE)
            for index in range(13):
                object_id = f"00000000-0000-4000-8000-{index + 100:012x}"
                x, y = 8 + (index % 4) * 22, 8 + (index // 4) * 22
                edit.create_rectangle(object_id, "重名", ASSET,
                                      (x, y), (x + 5, y + 5))
        with kasane.Observer(64, 64, 64) as observer:
            request = kasane.InspectionRequest(
                view=kasane.ViewSpec(
                    roi=(-4.25, 3.5, 103.75, 98.5),
                    resolution=(512, 384), padding_canvas=1.25,
                ), channels=("clean", "labels"),
            )
            packet = observer.inspect(model, request=request)
            clean, labels = packet.views
            marks = [row["mark"] for row in packet.objects if row["kind"] == "mesh"]
            self.assertEqual(marks, list(range(1, 14)))
            self.assertEqual(len(labels.presentation["labels"]), 12)
            self.assertEqual(len(labels.omitted_labels), 1)
            self.assertEqual(labels.omitted_labels[0]["reason"], "label_limit")
            self.assertEqual(clean.requested_roi, (-4.25, 3.5, 103.75, 98.5))
            self.assertEqual(clean.padded_roi, (-5.5, 2.25, 105.0, 99.75))
            self.assertEqual(clean.runtime_to_canvas((0, 0)), (13, 71))
            for point in ((0.125, 0.875), (50.375, 40.625), (100.25, 99.75)):
                image_point = clean.canvas_to_image(point)
                canvas_point = clean.image_to_canvas(image_point)
                self.assertAlmostEqual(canvas_point[0], point[0], places=6)
                self.assertAlmostEqual(canvas_point[1], point[1], places=6)
            x0, y0, x1, y1 = clean.content_rect
            self.assertGreaterEqual(x0, -1e-5)
            self.assertGreaterEqual(y0, -1e-5)
            self.assertLessEqual(x1, 512 + 1e-5)
            self.assertLessEqual(y1, 384 + 1e-5)

    def test_batch_resolves_names_from_one_snapshot_and_rejects_ambiguity(self):
        model = kasane.Session(DOCUMENT, 100, 100, (50, 50), 10)
        with model.edit("duplicate names") as edit:
            edit.create_parameter(PARAMETER, "Same", -1, 1, 0)
            edit.create_parameter(PART, "Same", -1, 1, 0)
        with kasane.Observer(32, 32, 32) as observer:
            with self.assertRaises(ValueError):
                observer.capture_scenes(model, [{}] * 65)
            with self.assertRaises(kasane.ObservationFailure) as failure:
                observer.capture_scenes(model, [{"Same": 0.5}])
            self.assertEqual(failure.exception.code, "AMBIGUOUS_PARAMETER_NAME")
            samples = observer.capture_scenes(model, [{PARAMETER: 0}, {PART: 0.5}])
            self.assertEqual(samples[0].capture_id, samples[1].capture_id)
            self.assertEqual(samples[1].metadata["requested"], {PART: 0.5})
        with self.assertRaises(ValueError):
            kasane.RawInspectionRequest((0, 0, 100, 100), (4096, 4096))

    def test_raw_packet_profiles_reopen_with_explicit_capabilities(self):
        model = kasane.Session(DOCUMENT, 100, 100, (50, 50), 10)
        with model.edit("packet fixture") as edit:
            edit.add_png_asset(ASSET, "texture", TEXTURE)
            edit.create_rectangle(MESH, "face", ASSET, (40, 40), (60, 60))
        request = kasane.RawInspectionRequest((35, 35, 65, 65), (96, 80))
        with kasane.Observer(64, 64, 64) as observer:
            packet = observer.inspect(model, request=request)
            self.assertEqual(packet.source_kind, "parameters")
            self.assertTrue(packet.capabilities["rerender_scene"])
            self.assertEqual(packet.objects[0]["name"], "face")
            extended = observer.render(packet, request=kasane.RawInspectionRequest(
                (38, 38, 62, 62), (128, 96), padding_canvas=1,
            ))
            self.assertEqual(extended.capture_id, packet.capture_id)
            self.assertEqual(extended.scene_digest, packet.scene_digest)
            self.assertEqual(len(extended.views), 2)
            with TemporaryDirectory() as temporary:
                root = Path(temporary).resolve()
                for profile in ("report", "analysis", "scene"):
                    directory = root / profile
                    receipt = extended.save(directory, profile=profile)
                    self.assertEqual(receipt.directory, directory)
                    self.assertEqual(len(receipt.manifest_sha256), 64)
                    reopened = observer.open(directory)
                    self.assertEqual(reopened.capture_id, packet.capture_id)
                    self.assertEqual(reopened.scene_digest, packet.scene_digest)
                    self.assertEqual(reopened.views[0].png, extended.views[0].png)
                    self.assertEqual(reopened.capabilities["rerender_scene"], profile == "scene")
                    self.assertEqual(reopened.capabilities["raw_pixel_data"], profile != "report")
                    if profile == "report":
                        self.assertIsNone(reopened.evaluated_frame)
                    else:
                        self.assertEqual(reopened.evaluated_frame, packet.evaluated_frame)
                        self.assertEqual(reopened.views[0].rgba, packet.views[0].rgba)
                    if profile == "scene":
                        added = observer.render(reopened, request=request)
                        self.assertEqual(added.views[-1].rgba, packet.views[0].rgba)
                        child = subprocess.run(
                            [sys.executable, "-c", """
import sys
from pathlib import Path
import kasane
request = kasane.RawInspectionRequest((35, 35, 65, 65), (96, 80))
with kasane.Observer(64, 64, 64) as observer:
    packet = observer.open(Path(sys.argv[1]))
    assert observer.render(packet, request=request).views[-1].rgba == packet.views[0].rgba
print('PACKET_REOPEN_OK')
""", str(directory)], capture_output=True, text=True,
                        )
                        self.assertEqual(child.returncode, 0, child.stderr)
                        self.assertIn("PACKET_REOPEN_OK", child.stdout)
                    else:
                        with self.assertRaises(kasane.ObservationFailure) as unavailable:
                            observer.render(reopened, request=request)
                        self.assertEqual(unavailable.exception.code, "CAPTURE_NOT_AVAILABLE")
                original_png = (root / "report/views/000.png").read_bytes()
                (root / "report/views/000.png").write_bytes(b"corrupted")
                with self.assertRaises(ValueError):
                    observer.open(root / "report")
                (root / "report/views/000.png").write_bytes(original_png)
                manifest_path = root / "report/packet.json"
                original_manifest = manifest_path.read_bytes()
                tampered = json.loads(original_manifest)
                tampered["files"]["../outside"] = "0" * 64
                manifest_path.write_text(json.dumps(tampered), encoding="utf-8")
                with self.assertRaises(ValueError):
                    observer.open(root / "report")
                manifest_path.write_bytes(original_manifest)
            packet.close()
            self.assertTrue(packet.closed)
            self.assertTrue(extended.capabilities["rerender_scene"])

    def test_animation_capture_preserves_pose_part_opacity(self):
        model = kasane.Session(DOCUMENT, 100, 100, (50, 50), 10)
        with model.edit("two pose parts") as edit:
            edit.add_png_asset(ASSET, "texture", TEXTURE)
            edit.create_part(PART, "first")
            edit.create_part(PART_B, "second")
            edit.create_rectangle(MESH, "left", ASSET, (15, 35), (40, 65))
            edit.create_rectangle(BACKGROUND, "right", ASSET, (60, 35), (85, 65))
            edit.set_mesh_part(MESH, PART)
            edit.set_mesh_part(BACKGROUND, PART_B)
            edit.create_pose(POSE, [[(PART, []), (PART_B, [])]], fade_in=0.5)
        preview = model.motion_preview()
        self.assertEqual(preview.snapshot().part_opacities[PART_B], 0)
        with kasane.Observer(64, 64, 64) as observer:
            static = observer.render_scene(
                observer.capture_scene(model), roi=(0, 0, 100, 100),
                resolution=(128, 128),
            )
            animated = observer.capture_animation_scene(model, preview)
            actual = observer.render_scene(
                animated, roi=(0, 0, 100, 100), resolution=(128, 128),
            )
            self.assertNotEqual(actual.frame.rgba, static.frame.rgba)
            self.assertEqual(animated.source["snapshot"]["part_opacities"][PART_B], 0)
            self.assertEqual(animated.source["operation"]["sequence"], 0)
            self.assertEqual(animated.source["operation"]["kind"]["operation"], "created")
            self.assertEqual(animated.source["operation"], preview.operation)
            self.assertEqual(preview.snapshot().part_opacities[PART_B], 0)

    def test_frozen_scene_roi_reopens_without_live_session(self):
        model = kasane.Session(DOCUMENT, 100, 100, (50, 50), 10)
        with model.edit("base") as edit:
            edit.add_png_asset(ASSET, "texture", TEXTURE)
            edit.create_rectangle(MESH, "face", ASSET, (40, 40), (60, 60))
            edit.create_parameter(PARAMETER, "Shift", -1, 1, 0)
        with kasane.Observer(64, 64, 64) as observer:
            scene = observer.capture_scene(model)
            self.assertGreaterEqual(scene.metadata["snapshot_clone_ns"], 0)
            samples = observer.capture_scenes(model, [{"Shift": 0}, {"Shift": 0.5}])
            self.assertEqual(samples[0].capture_id, samples[1].capture_id)
            self.assertNotEqual(samples[0].scene_digest, samples[1].scene_digest)
            self.assertEqual(samples[0].source, samples[1].source)
            self.assertEqual(len(scene.scene_digest), 64)
            self.assertEqual(scene.scene_digest, observer.capture_scene(model).scene_digest)
            self.assertNotEqual(scene.capture_id, observer.capture_scene(model).capture_id)
            captured_positions = scene.authoring["meshes"][0]["base_positions"]
            preview = model.motion_preview()
            before_preview = preview.snapshot()
            animated = observer.capture_animation_scene(model, preview)
            self.assertEqual(preview.snapshot(), before_preview)
            self.assertEqual(animated.source["source_kind"], "animation")
            self.assertEqual(animated.source["history_status"], "not_recorded")
            self.assertFalse(animated.source["apply_model_opacity"])
            self.assertEqual(animated.source["snapshot"]["time"], before_preview.time)
            view = observer.render_scene(
                scene, roi=(38.25, 39.5, 61.75, 60.5),
                resolution=(128, 96), padding_canvas=2,
            )
            self.assertEqual((view.frame.width, view.frame.height), (128, 96))
            point = (51.125, 52.25)
            restored = view.image_to_canvas(view.canvas_to_image(point))
            self.assertAlmostEqual(restored[0], point[0], places=5)
            self.assertAlmostEqual(restored[1], point[1], places=5)
            with model.edit("move later") as edit:
                edit.update_positions(MESH, [0, 1, 2, 3], [
                    (50, 40), (70, 40), (70, 60), (50, 60),
                ])
                edit.rename_mesh(MESH, "renamed later")
            self.assertEqual(scene.authoring["meshes"][0]["base_positions"],
                             captured_positions)
            self.assertEqual(scene.authoring["meshes"][0]["name"], "face")
            self.assertEqual(model.mesh(MESH).name, "renamed later")
            self.assertNotEqual(model.mesh(MESH).positions[0][0],
                                captured_positions[0]["x"])
            with self.assertRaises(kasane.ObservationFailure) as stale:
                observer.capture_animation_scene(model, preview)
            self.assertEqual(stale.exception.code, "STALE_ANIMATION_PREVIEW")
            self.assertNotEqual(
                view.frame.rgba,
                observer.observe(model).rgba,
            )
            with TemporaryDirectory() as directory:
                bundle = Path(directory).resolve() / "scene"
                scene.save_scene(bundle)
                reopened = observer.open_scene(bundle)
                self.assertEqual(reopened.capture_id, scene.capture_id)
                self.assertEqual(reopened.scene_digest, scene.scene_digest)
                self.assertEqual(reopened.authoring, scene.authoring)
                again = observer.render_scene(
                    reopened, roi=(38.25, 39.5, 61.75, 60.5),
                    resolution=(128, 96), padding_canvas=2,
                )
                self.assertEqual(again.frame.rgba, view.frame.rgba)
                self.assertEqual(again.render_digest, view.render_digest)
                animated_bundle = Path(directory).resolve() / "animated"
                animated.save_scene(animated_bundle)
                self.assertEqual(observer.open_scene(animated_bundle).source, animated.source)
                child = subprocess.run(
                    [sys.executable, "-c", """
import sys
from pathlib import Path
import kasane
with kasane.Observer(64, 64, 64) as observer:
    scene = observer.open_scene(Path(sys.argv[1]))
    frame = observer.render_scene(scene, roi=(38.25, 39.5, 61.75, 60.5),
                                  resolution=(128, 96), padding_canvas=2).frame
    assert any(frame.rgba[3::4])
print('REOPEN_OK')
""", str(bundle)],
                    capture_output=True, text=True, check=True,
                )
                self.assertIn("REOPEN_OK", child.stdout)

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

    def test_rectangle_grid_remesh_keeps_bound_appearance(self):
        parameter_id = "00000000-0000-4000-8000-000000000020"
        binding_id = "00000000-0000-4000-8000-000000000021"
        model = kasane.Session(DOCUMENT, 100, 100, (50, 50), 10)
        with model.edit("bound rectangle") as edit:
            edit.add_png_asset(ASSET, "asymmetric", TEXTURE)
            edit.create_rectangle(MESH, "face", ASSET, (20, 20), (80, 80))
            edit.create_parameter(parameter_id, "Turn", 0, 1, 0)
            edit.create_mesh_binding(binding_id, MESH, [kasane.Axis(parameter_id, [0, 1])], [
                kasane.MeshKeyform([0], [(20, 20), (80, 20), (80, 80), (20, 80)]),
                kasane.MeshKeyform([1], [(20, 20), (76, 18), (67, 82), (21, 77)]),
            ])
        with kasane.Observer(256, 256, 256) as observer:
            before = [observer.observe(model, {"Turn": value}).rgba for value in (0, 0.5, 1)]
            model.remesh_rectangle_grid(MESH, 8, 8)
            after = [observer.observe(model, {"Turn": value}).rgba for value in (0, 0.5, 1)]
        for original, remeshed in zip(before, after):
            pixel_deltas = [
                max(abs(a - b) for a, b in zip(original[i:i + 4], remeshed[i:i + 4]))
                for i in range(0, len(original), 4)
            ]
            # Repeated UVs can move a few raster samples at grid boundaries.
            self.assertLessEqual(sum(delta > 0 for delta in pixel_deltas), 128)
            self.assertLessEqual(sum(delta > 1 for delta in pixel_deltas), 2)


if __name__ == "__main__":
    unittest.main()
