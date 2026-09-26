"""O4 trace, visual, and canvas-space deformation checks against an observe wheel."""
from dataclasses import replace
import copy
from pathlib import Path
from tempfile import TemporaryDirectory
import unittest

import kasane
from kasane._deformation import _triangle_metrics, diagnose_deformation


FIXTURES = Path(__file__).resolve().parents[3] / "tests/fixtures/observe_inspection/projects"
PARAMETER = "10000000-0000-4000-8000-00000000001f"


class TraceTests(unittest.TestCase):
    def test_parent_warp_curves_rotation_axis_and_trace_includes_blend_glue(self):
        def uid(number):
            return f"50000000-0000-4000-8000-{number:012d}"

        texture = (Path(__file__).resolve().parent / "fixtures/asymmetric-2x2.png").resolve()
        model = kasane.Session(uid(1), 100, 100, (50, 50), 10)
        with model.edit("warped axis and final positions") as edit:
            edit.add_png_asset(uid(2), "texture", texture)
            edit.create_rectangle(uid(3), "a", uid(2), (20, 20), (40, 40))
            edit.create_rectangle(uid(4), "b", uid(2), (60, 60), (80, 80))
            edit.create_warp_transform(uid(9), "parent", kasane.WarpData(
                1, 2, True, [(0, 0), (50, 20), (100, 0),
                             (0, 100), (50, 120), (100, 100)],
            ))
            edit.create_rotation_transform(uid(10), "child", kasane.RotationData(
                0, kasane.RotationPose((0, 0)),
            ))
            edit.set_transform_parent(uid(10), uid(9))
            edit.set_deform_parent(uid(3), uid(10))
            edit.create_glue(kasane.GlueSpec(
                uid(5), "seam", uid(3), uid(4),
                [kasane.GlueVertexPair(0, 0, 0.5, 0.5)], 1,
            ))
            edit.create_parameter(uid(6), "shape", 0, 1, 0, kind="blend_shape")
            edit.create_blend_key_table(kasane.BlendKeyTableSpec(uid(7), uid(6), [0, 1], 0))
            edit.create_blend_binding(kasane.BlendBindingSpec(
                uid(8), uid(3), "mesh", uid(7), [],
                [kasane.BlendMeshDelta([(0, 0)] * 4),
                 kasane.BlendMeshDelta([(10, 0)] * 4)],
            ))
        with kasane.Observer(128, 128, 128) as observer:
            baseline, changed = observer.capture_scenes(
                model, [{uid(6): 0}, {uid(6): 1}], with_trace=True,
            )
            for scene in (baseline, changed):
                for mesh in scene.evaluation_trace["meshes"]:
                    drawable = next(row for row in scene.evaluated_frame["drawables"]
                                    if row["id"] == mesh["id"])
                    self.assertEqual(mesh["positions"], drawable["positions"])
                    self.assertTrue(mesh["includes_blendshape_and_glue"])
            self.assertNotEqual(baseline.evaluation_trace["meshes"][0]["positions"],
                                changed.evaluation_trace["meshes"][0]["positions"])
            axis = next(item["rotation_axis_samples"] for item in
                        baseline.evaluation_trace["transforms"] if item["id"] == uid(10))
            self.assertEqual(len(axis), 17)
            self.assertNotAlmostEqual(axis[4]["y"], axis[12]["y"])

    def test_optional_same_pass_trace_overlay_and_scene_roundtrip(self):
        model = kasane.open_project((FIXTURES / "rotated-warp.kasane.json").resolve())
        request = kasane.InspectionRequest(
            view=kasane.ViewSpec(roi=(0, 0, 100, 100), resolution=(128, 128)),
            channels=("clean", "wireframe", "vertices", "deformers"),
        )
        with TemporaryDirectory() as temporary, kasane.Observer(128, 128, 128) as observer:
            without = observer.capture_scene(model)
            self.assertIsNone(without.evaluation_trace)
            packet = observer.inspect(model, request=request)
            self.assertEqual([view.kind for view in packet.views], list(request.channels))
            trace = packet.evaluation_trace
            self.assertEqual(len(trace["meshes"]), 1)
            self.assertEqual(len(trace["transforms"]), 2)
            self.assertEqual(len(trace["transforms"][0]["rotation_axis_samples"]), 17)
            self.assertEqual(trace["transforms"][1]["control_point_identity"],
                             "deformer_topology_local_index")
            self.assertEqual(trace["meshes"][0]["vertex_ids"], [0, 1, 2, 3])
            self.assertEqual(trace["meshes"][0]["triangles"], [[0, 1, 2], [0, 2, 3]])
            self.assertEqual(trace["meshes"][0]["topology_hash"], next(
                row["topology_hash"] for row in packet.objects
                if row["kind"] == "mesh" and row["id"] == trace["meshes"][0]["id"]
            ))
            folder = Path(temporary).resolve() / "packet"
            packet.save(folder, profile="scene")
            opened = observer.open(folder)
            self.assertEqual(opened.evaluation_trace, trace)
            self.assertEqual(opened._scene.evaluation_trace, trace)
            self.assertEqual([view.kind for view in opened.views], list(request.channels))

    def test_animation_trace_uses_preview_frame(self):
        model = kasane.open_project((FIXTURES / "pixel-binding.kasane.json").resolve())
        preview = model.motion_preview()
        request = kasane.InspectionRequest(
            view=kasane.ViewSpec(roi=(0, 0, 100, 100), resolution=(64, 64)),
            channels=("clean", "wireframe"),
        )
        with kasane.Observer(64, 64, 64) as observer:
            packet = observer.inspect_animation(model, preview, request=request)
            self.assertEqual(packet.source_kind, "animation")
            self.assertIsNotNone(packet.evaluation_trace)
            self.assertEqual([view.kind for view in packet.views], ["clean", "wireframe"])
            for mesh in packet.evaluation_trace["meshes"]:
                drawable = next(item for item in packet.evaluated_frame["drawables"]
                                if item["id"] == mesh["id"])
                self.assertEqual(mesh["positions"], drawable["positions"])

    def test_rigid_rotation_reflection_scaling_and_degenerate_baseline(self):
        base = [(0, 0), (10, 0), (0, 10)]
        rotated = [(0, 0), (0, 10), (-10, 0)]
        rigid = _triangle_metrics(base, rotated, 0.5, 2.0)
        self.assertAlmostEqual(rigid["determinant"], 1)
        self.assertAlmostEqual(rigid["singular_values"][0], 1)
        self.assertAlmostEqual(rigid["singular_values"][1], 1)
        self.assertEqual(rigid["flags"], [])
        reflected = _triangle_metrics(base, [(0, 0), (-10, 0), (0, 10)], 0.5, 2.0)
        self.assertEqual(reflected["determinant"], -1)
        self.assertIn("orientation_reversal", reflected["flags"])
        stretched = _triangle_metrics(base, [(0, 0), (30, 0), (0, 2)], 0.5, 2.0)
        self.assertAlmostEqual(stretched["singular_values"][0], 0.2)
        self.assertAlmostEqual(stretched["singular_values"][1], 3)
        self.assertIn("compression", stretched["flags"])
        self.assertIn("stretch", stretched["flags"])
        degenerate = _triangle_metrics([(0, 0), (10, 0), (20, 0)], rotated, 0.5, 2.0)
        self.assertEqual(degenerate["status"], "baseline_degenerate")
        self.assertIsNone(degenerate["singular_values"])

    def test_canvas_invariance_topology_mismatch_and_run_report(self):
        model = kasane.open_project((FIXTURES / "pixel-binding.kasane.json").resolve())
        with TemporaryDirectory() as temporary, kasane.Observer(192, 128, 192) as observer:
            packets = []
            for resolution in ((96, 64), (192, 128)):
                request = kasane.InspectionRequest(
                    view=kasane.ViewSpec(roi=(0, 0, 100, 100), resolution=resolution),
                    channels=("clean", "displacement", "distortion"),
                )
                packet = observer.inspect(model, {PARAMETER: 1}, request=request,
                                          baseline_values={PARAMETER: 0})
                self.assertEqual([view.kind for view in packet.views], list(request.channels))
                self.assertEqual(packet.deformation["summary"]["abnormal"], 0)
                self.assertGreater(packet.deformation["meshes"][0]
                                   ["vertex_displacement_px"]["maximum"], 0)
                packets.append(packet)
            self.assertEqual(packets[0].deformation, packets[1].deformation)
            baseline = observer.inspect(model, {PARAMETER: 0}, request=kasane.InspectionRequest(
                view=kasane.ViewSpec(roi=(0, 0, 100, 100), resolution=(96, 64)),
                channels=("clean", "wireframe"),
            ))
            changed = copy.deepcopy(baseline.evaluation_trace)
            changed["meshes"][0]["topology_hash"] = "0" * 64
            mismatch = diagnose_deformation(replace(baseline, evaluation_trace=changed),
                                            baseline, request)
            self.assertEqual(mismatch["status"], "TOPOLOGY_MISMATCH")
            self.assertEqual(mismatch["summary"]["topology_mismatch"], 1)
            run = observer.inspect_run(
                model, [{PARAMETER: 0}, {PARAMETER: 1}], request=request,
                output=Path(temporary).resolve(), baseline_index=0,
            )
            reopened = kasane.open_inspection_run(run.run_directory)
            self.assertEqual(reopened.status, "complete")
            self.assertEqual(reopened.report["diagnostics"][0]["deformation"]["status"],
                             "comparable")


if __name__ == "__main__":
    unittest.main()
