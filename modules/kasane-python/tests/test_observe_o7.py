"""Explicit animation replay and v2 report integration checks."""
from pathlib import Path
from tempfile import TemporaryDirectory
import unittest

import kasane


PROJECT = (Path(__file__).resolve().parents[3] /
           "tests/fixtures/observe_inspection/projects/pixel-binding.kasane.json")
PARAMETER = "10000000-0000-4000-8000-00000000001f"
MESH = "10000000-0000-4000-8000-000000000015"
MOTION = "b0000000-0000-4000-8000-000000000001"
TRACK = "b0000000-0000-4000-8000-000000000002"


def motion_model():
    model = kasane.open_project(PROJECT.resolve())
    with model.edit("animated inspection fixture") as edit:
        edit.create_motion(MOTION, "shift", 1, 30, fade_in=0, fade_out=0)
        edit.create_motion_track(MOTION, {
            "id": TRACK,
            "target": {"kind": "parameter", "parameter_id": PARAMETER},
            "initial": {"time": 0, "value": 0},
            "segments": [{"kind": "linear", "end": {"time": 1, "value": 1}}],
            "fade_in": None, "fade_out": None, "extensions": {},
        })
    return model


class AnimationRunTests(unittest.TestCase):
    def test_recipe_batch_matches_preview_and_report_reopens(self):
        model = motion_model()
        preview = model.motion_preview()
        before = preview.snapshot()
        recipe = kasane.PlaybackRecipe((kasane.PlaybackAction(
            "schedule_motion", motion_id=MOTION, time=0),))
        request = kasane.InspectionRequest(
            focus=kasane.Focus(mesh_ids=(MESH,)),
            view=kasane.ViewSpec(roi=(0, 0, 100, 100), resolution=(64, 64)),
            channels=("clean", "wireframe"),
        )
        with TemporaryDirectory() as temporary, kasane.Observer(64, 64, 64) as observer:
            scenes = observer.capture_animation_scenes(
                model, playback=recipe, times=(0, 0.5, 1), with_trace=True)
            self.assertEqual(len({scene.capture_id for scene in scenes}), 1)
            self.assertEqual(len({scene.scene_digest for scene in scenes}), 3)
            for time, scene in zip((0, 0.5, 1), scenes):
                preview.schedule_motion(MOTION, 0) if time == 0 else None
                expected = preview.seek(time)
                self.assertEqual(scene.source["history_status"], "recipe_recorded")
                self.assertEqual(scene.source["playback_recipe"], recipe.record())
                self.assertAlmostEqual(scene.source["snapshot"]["time"], time)
                self.assertAlmostEqual(scene.source["snapshot"]["parameters"][PARAMETER],
                                       expected.parameters[PARAMETER])
                self.assertIsNotNone(scene.evaluation_trace)
            run = observer.inspect_animation_run(
                model, playback=recipe, times=(0, 0.5, 1),
                request=request, output=Path(temporary).resolve(), baseline_index=0)
            self.assertEqual(run.status, "complete")
            self.assertEqual(run.report["playback"]["recipe"], recipe.record())
            self.assertEqual(run.report["playback"]["times"], [0, 0.5, 1])
            self.assertEqual(run.report["resources"]["render_count"], 3)
            self.assertEqual(len(run.report["comparisons"]), 2)
            self.assertEqual([entry["source"]["snapshot"]["time"]
                              for entry in run.report["samples"]], [0, 0.5, 1])
            reopened = kasane.open_inspection_run(run.run_directory)
            self.assertEqual([view.kind for view in reopened.packet(1).views],
                             ["clean", "wireframe"])
            self.assertEqual(before, model.motion_preview().snapshot())

    def test_failed_recipe_keeps_readable_failed_report(self):
        model = motion_model()
        recipe = kasane.PlaybackRecipe((kasane.PlaybackAction(
            "schedule_motion", motion_id="b0000000-0000-4000-8000-000000000099",
            time=0),))
        request = kasane.InspectionRequest(
            view=kasane.ViewSpec(roi=(0, 0, 100, 100), resolution=(32, 32)),
            channels=("clean",),
        )
        with TemporaryDirectory() as temporary, kasane.Observer(32, 32, 32) as observer:
            with self.assertRaises(kasane.ObservationFailure) as failure:
                observer.inspect_animation_run(
                    model, playback=recipe, times=(0, 0.5),
                    request=request, output=Path(temporary).resolve())
            self.assertEqual(failure.exception.code, "MISSING_MOTION")
            failed = kasane.open_inspection_run(failure.exception.run_directory)
            self.assertEqual(failed.status, "failed")
            self.assertEqual(failed.report["diagnostics"][0]["code"], "MISSING_MOTION")

    def test_recipe_validation_and_nonmonotonic_times(self):
        with self.assertRaises(ValueError):
            kasane.PlaybackAction("schedule_motion", motion_id=MOTION, time=float("nan"))
        with self.assertRaises(ValueError):
            kasane.PlaybackAction("unknown")
        model = motion_model()
        recipe = kasane.PlaybackRecipe((kasane.PlaybackAction(
            "schedule_motion", motion_id=MOTION, time=0),))
        with kasane.Observer(32, 32, 32) as observer:
            scenes = observer.capture_animation_scenes(
                model, playback=recipe, times=(1, 0.5, 0))
            self.assertEqual([scene.source["snapshot"]["time"] for scene in scenes],
                             [1, 0.5, 0])
            self.assertEqual([scene.source["snapshot"]["parameters"][PARAMETER]
                              for scene in scenes], [1, 0.5, 0])

    def test_base_and_timed_input_match_preview_and_observer_recovers(self):
        model = motion_model()
        recipe = kasane.PlaybackRecipe((
            kasane.PlaybackAction("set_base_parameter", parameter_id=PARAMETER,
                                  value=0.25),
            kasane.PlaybackAction("schedule_parameter_input", parameter_id=PARAMETER,
                                  time=0.5, value=0.75),
        ))
        preview = model.motion_preview()
        preview.set_base_parameter(PARAMETER, 0.25)
        preview.schedule_parameter_input(PARAMETER, 0.5, 0.75)
        request = kasane.InspectionRequest(
            view=kasane.ViewSpec(roi=(0, 0, 100, 100), resolution=(32, 32)),
            channels=("clean",),
        )
        with kasane.Observer(32, 32, 32) as observer:
            scenes = observer.capture_animation_scenes(
                model, playback=recipe, times=(0, 0.5, 1))
            for time, scene in zip((0, 0.5, 1), scenes):
                expected = preview.seek(time)
                self.assertAlmostEqual(
                    scene.source["snapshot"]["parameters"][PARAMETER],
                    expected.parameters[PARAMETER])
            packet = observer.inspect_scene(scenes[1], request=request)
            packet.close()
            with self.assertRaises(kasane.ObservationFailure):
                observer.query(packet, view_id="missing", point=(0, 0))
            recovered = observer.inspect_scene(scenes[2], request=request)
            self.assertEqual(recovered.views[0].kind, "clean")


if __name__ == "__main__":
    unittest.main()
