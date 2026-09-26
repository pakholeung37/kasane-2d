"""V06 isolation, X-ray, and renderer mask attachment integration checks."""
from dataclasses import replace
from pathlib import Path
from tempfile import TemporaryDirectory
import unittest

import kasane


PROJECT = (Path(__file__).resolve().parents[3] /
           "tests/fixtures/observe_inspection/projects/masked-offscreen.kasane.json")
MASK = "10000000-0000-4000-8000-000000000019"
TARGET = "10000000-0000-4000-8000-00000000001a"
FRONT = "10000000-0000-4000-8000-00000000001b"
OFFSCREEN = "10000000-0000-4000-8000-000000000047"
TEXTURE = (Path(__file__).resolve().parent / "fixtures/asymmetric-2x2.png").resolve()


def request(*, mode="context", channels=("clean", "mask"), xray=None):
    return kasane.InspectionRequest(
        focus=kasane.Focus(mesh_ids=(TARGET,)),
        view=kasane.ViewSpec(roi=(0, 0, 100, 100), resolution=(96, 96)),
        channels=channels, mode=mode, xray=xray or kasane.XraySpec(),
    )


def set_properties(model, mesh_id, *, inverted=None, enabled=None, blend=None):
    previous = model.mesh_properties(mesh_id)
    with model.edit("diagnostic fixture property") as edit:
        edit.update_mesh_properties(mesh_id, kasane.MeshProperties(
            previous.texture_asset_id, previous.appearance, previous.draw_order,
            previous.blend_mode if blend is None else blend,
            previous.enabled if enabled is None else enabled,
            previous.double_sided,
            previous.inverted_mask if inverted is None else inverted,
            previous.masks,
        ))


class DiagnosticViewTests(unittest.TestCase):
    def test_isolation_preserves_mask_dependency_and_clean_cache(self):
        model = kasane.open_project(PROJECT.resolve())
        with kasane.Observer(96, 96, 96) as observer:
            scene = observer.capture_scene(model)
            clean_before = observer.inspect_scene(scene, request=replace(
                request(), channels=("clean",))).views[0].rgba
            packet = observer.inspect_scene(scene, request=request(mode="isolated"))
            by_kind = {view.kind: view for view in packet.views}
            self.assertEqual(set(by_kind), {"clean", "isolated", "mask_source",
                                            "mask_combined", "mask_consumer", "mask_coverage"})
            plan = by_kind["isolated"].presentation["diagnostic_plan"]
            self.assertEqual(plan["color_mesh_ids"], [TARGET])
            self.assertEqual(plan["mask_only_mesh_ids"], [MASK])
            self.assertEqual(plan["ancestor_target_ids"], [OFFSCREEN])
            self.assertEqual(plan["destination_context"], "isolated")
            self.assertNotEqual(by_kind["clean"].rgba, by_kind["isolated"].rgba)
            self.assertEqual(by_kind["mask_source"].rgba,
                             by_kind["mask_combined"].rgba)
            self.assertEqual(by_kind["mask_combined"].rgba,
                             by_kind["mask_consumer"].rgba)
            self.assertEqual(by_kind["mask_combined"].presentation["source_ids"], [MASK])
            self.assertEqual(len(by_kind["mask_source"].presentation[
                "source_geometry"]["positions"]), 4)
            self.assertGreater(by_kind["mask_combined"].width, 0)
            self.assertEqual(observer.inspect_scene(scene, request=replace(
                request(), channels=("clean",))).views[0].rgba, clean_before)

    def test_inverted_mask_consumer_and_scene_roundtrip(self):
        model = kasane.open_project(PROJECT.resolve())
        set_properties(model, TARGET, inverted=True)
        with TemporaryDirectory() as temporary, kasane.Observer(96, 96, 96) as observer:
            scene = observer.capture_scene(model)
            packet = observer.inspect_scene(scene, request=request())
            views = {view.kind: view for view in packet.views}
            source = views["mask_combined"].rgba
            consumer = views["mask_consumer"].rgba
            for index in range(0, len(source), 4):
                self.assertEqual(consumer[index], 255 - source[index])
                self.assertEqual(consumer[index + 3], 255)
            self.assertTrue(views["mask_consumer"].presentation["inversion_applied"])
            directory = Path(temporary).resolve() / "scene"
            packet.save(directory, profile="scene")
            reopened = observer.open(directory)
            self.assertEqual([view.kind for view in reopened.views],
                             [view.kind for view in packet.views])
            self.assertEqual(reopened.views[-1].presentation,
                             packet.views[-1].presentation)
            self.assertEqual(observer.inspect_scene(reopened._scene, request=request()).views[0].rgba,
                             packet.views[0].rgba)

    def test_xray_disabled_requires_explicit_hidden_capture(self):
        model = kasane.open_project(PROJECT.resolve())
        set_properties(model, TARGET, enabled=False)
        spec = kasane.XraySpec(ignore_masks=True, ignore_opacity=True,
                               include_disabled=True)
        with kasane.Observer(96, 96, 96) as observer:
            ordinary = observer.capture_scene(model, with_trace=True)
            with self.assertRaises(kasane.ObservationFailure) as failure:
                observer.inspect_scene(ordinary, request=request(
                    mode="xray", channels=("clean",), xray=spec))
            self.assertEqual(failure.exception.code, "CAPTURE_NOT_AVAILABLE")
            packet = observer.inspect(model, request=request(
                mode="xray", channels=("clean",), xray=spec))
            self.assertTrue(packet.metadata["hidden_geometry_captured"])
            self.assertEqual([view.kind for view in packet.views], ["clean", "xray"])
            xray = packet.views[1]
            self.assertEqual(xray.mode, "xray")
            self.assertGreater(xray.presentation["covered_pixels"], 0)
            self.assertEqual(xray.presentation["overrides"], {
                "ignore_masks": True, "ignore_opacity": True,
                "include_disabled": True,
            })
            self.assertNotEqual(packet.views[0].rgba, xray.rgba)

    def test_destination_read_uses_isolated_background(self):
        model = kasane.open_project(PROJECT.resolve())
        set_properties(model, TARGET, blend="multiplicative")
        with kasane.Observer(96, 96, 96) as observer:
            scene = observer.capture_scene(model)
            packet = observer.inspect_scene(scene, request=request(
                mode="isolated", channels=("clean",)))
            self.assertEqual(packet.views[1].presentation["destination_context"], "isolated")
            self.assertNotEqual(packet.views[0].rgba, packet.views[1].rgba)
            again = observer.inspect_scene(scene, request=request(channels=("clean",)))
            self.assertEqual(again.views[0].rgba, packet.views[0].rgba)

    def test_nested_offscreen_mask_paths_and_inversion(self):
        def uid(number):
            return f"60000000-0000-4000-8000-{number:012d}"
        model = kasane.Session(uid(1), 100, 100, (50, 50), 10)
        with model.edit("nested mask targets") as edit:
            edit.add_png_asset(uid(2), "texture", TEXTURE)
            edit.create_part(uid(3), "outer")
            edit.create_part(uid(4), "inner", uid(3))
            edit.create_rectangle(uid(5), "outer mask", uid(2), (25, 25), (75, 75))
            edit.create_rectangle(uid(6), "inner mask", uid(2), (40, 40), (60, 60))
            edit.create_rectangle(uid(7), "target", uid(2), (20, 20), (80, 80))
            edit.set_mesh_part(uid(7), uid(4))
            edit.create_offscreen(kasane.OffscreenSpec(
                uid(8), "outer", uid(3), blend_mode=1, flags=4, masks=[uid(5)],
            ))
            edit.create_offscreen(kasane.OffscreenSpec(
                uid(9), "inner", uid(4), flags=12, masks=[uid(6)],
            ))
        with kasane.Observer(96, 96, 96) as observer:
            scene = observer.capture_scene(model)
            packet = observer.inspect_scene(scene, request=kasane.InspectionRequest(
                focus=kasane.Focus(mesh_ids=(uid(7),)),
                view=kasane.ViewSpec(roi=(0, 0, 100, 100), resolution=(96, 96)),
                channels=("clean", "mask"), mode="isolated",
            ))
            plan = next(view for view in packet.views if view.kind == "isolated").presentation[
                "diagnostic_plan"]
            self.assertEqual(plan["ancestor_target_ids"], [uid(8), uid(9)])
            self.assertEqual(set(plan["mask_only_mesh_ids"]), {uid(5), uid(6)})
            combined = [view for view in packet.views if view.kind == "mask_combined"]
            consumers = [view for view in packet.views if view.kind == "mask_consumer"]
            self.assertEqual([view.presentation["consumer_id"] for view in combined],
                             [uid(8), uid(9)])
            self.assertEqual(consumers[0].rgba, combined[0].rgba)
            for index in range(0, len(combined[1].rgba), 4):
                self.assertEqual(consumers[1].rgba[index],
                                 255 - combined[1].rgba[index])

    def test_animation_xray_hidden_capture(self):
        model = kasane.open_project(PROJECT.resolve())
        set_properties(model, TARGET, enabled=False)
        preview = model.motion_preview()
        with kasane.Observer(96, 96, 96) as observer:
            packet = observer.inspect_animation(
                model, preview, request=request(mode="xray", channels=("clean",),
                    xray=kasane.XraySpec(include_disabled=True)))
            self.assertEqual(packet.source_kind, "animation")
            self.assertTrue(packet.metadata["hidden_geometry_captured"])
            self.assertGreater(packet.views[1].presentation["covered_pixels"], 0)

    def test_samples_report_counts_diagnostic_readbacks(self):
        model = kasane.open_project(PROJECT.resolve())
        with TemporaryDirectory() as temporary, kasane.Observer(96, 96, 96) as observer:
            first, second = observer.inspect_samples(model, [{}, {}], request=request(mode="xray", channels=("clean",)))
            self.assertEqual([view.kind for view in first.views], ["clean", "xray"])
            self.assertEqual(second.views[1].sample_index, 1)
            run = observer.inspect_run(model, [{}], request=request(mode="isolated"),
                                       output=Path(temporary).resolve())
            self.assertEqual(run.report["status"], "complete")
            self.assertEqual(run.report["resources"]["render_count"], 5)
            self.assertEqual(run.report["resources"]["readback_count"], 7)
            self.assertGreater(run.report["resources"]["readback_bytes"], 0)
            offline = kasane.open_inspection_run(run.run_directory)
            self.assertEqual([view.kind for view in offline.packet(0).views],
                             ["clean", "isolated", "mask_source", "mask_combined",
                              "mask_consumer", "mask_coverage"])

    def test_mask_budget_rejects_before_attachment_passes(self):
        model = kasane.open_project(PROJECT.resolve())
        bounded = replace(request(), limits=kasane.InspectionLimits(
            max_artifact_pixels=96 * 96 * 2))
        with kasane.Observer(96, 96, 96) as observer:
            scene = observer.capture_scene(model)
            before = observer.inspect_scene(scene, request=replace(
                request(), channels=("clean",))).views[0].rgba
            with self.assertRaises(kasane.ObservationFailure) as failure:
                observer.inspect_scene(scene, request=bounded)
            self.assertEqual(failure.exception.code, "OBSERVATION_BUDGET_EXCEEDED")
            after = observer.inspect_scene(scene, request=replace(
                request(), channels=("clean",))).views[0].rgba
            self.assertEqual(before, after)


if __name__ == "__main__":
    unittest.main()
