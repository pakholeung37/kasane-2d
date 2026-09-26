"""O3 integration tests, run against the observe wheel with its inspection extra."""
from dataclasses import replace
import json
from pathlib import Path
from tempfile import TemporaryDirectory
import unittest

import kasane


PROJECT = (Path(__file__).resolve().parents[3] /
           "tests/fixtures/observe_inspection/projects/pixel-binding.kasane.json")
PARAMETER = "10000000-0000-4000-8000-00000000001f"
DOCUMENT = "30000000-0000-4000-8000-000000000001"
X = "30000000-0000-4000-8000-000000000002"
Y = "30000000-0000-4000-8000-000000000003"


class ComparisonTests(unittest.TestCase):
    def setUp(self):
        self.model = kasane.open_project(PROJECT.resolve())
        self.request = kasane.InspectionRequest(
            view=kasane.ViewSpec(roi=(0, 0, 100, 100), resolution=(96, 64)),
            channels=("clean", "alpha"),
        )

    def test_registered_packet_metrics_and_policy_rejection(self):
        with kasane.Observer(96, 64, 96) as observer:
            baseline, changed = observer.inspect_samples(
                self.model, [{PARAMETER: 0}, {PARAMETER: 1}], request=self.request,
            )
            result = kasane.compare_observations(
                changed, baseline,
                options=kasane.CompareOptions(target_roi=(25, 25, 60, 60), threshold=4),
            )
            self.assertEqual(result.registration_status, "registered")
            self.assertEqual(result.view_compatibility["status"], "compatible")
            self.assertEqual([artifact.kind for artifact in result.artifacts],
                             ["side_by_side", "onion_skin", "outline",
                              "abs_diff_heatmap"])
            self.assertEqual(result.artifacts[0].width, 192)
            self.assertIsNotNone(result.change_bounds)
            self.assertGreater(result.metrics["opaque_rgb"]["target"]["mae"], 0)
            self.assertEqual(result.metrics["compatible_raw_rgba"]["status"],
                             "unavailable")
            self.assertEqual(result.metrics["transparent_alpha"]["whole_view"]["status"],
                             "complete")
            moving = "10000000-0000-4000-8000-000000000015"
            geometry = kasane.compare_observations(
                changed, baseline,
                options=kasane.CompareOptions(target_mesh_ids=(moving,)),
            )
            x0, _, x1, _ = geometry.target_basis["roi_canvas"]
            self.assertAlmostEqual(x0, 20, places=3)
            self.assertAlmostEqual(x1, 64, places=3)
            self.assertEqual(geometry.target_basis["reference_geometry_status"], "included")
            dark = observer.inspect(self.model, request=replace(
                self.request,
                presentation=kasane.PresentationSpec(background="dark"),
            ))
            with self.assertRaisesRegex(ValueError, "INCOMPATIBLE_PRESENTATION"):
                kasane.compare_observations(changed, dark)

    def test_external_reference_registration_and_offline_report_compare(self):
        with TemporaryDirectory() as temporary, kasane.Observer(96, 64, 96) as observer:
            packet = observer.inspect(self.model, request=self.request)
            path = Path(temporary).resolve() / "reference.png"
            path.write_bytes(packet.views[0].png)
            unregistered = kasane.compare_observations(
                packet, kasane.ExternalReference(path),
            )
            self.assertEqual(unregistered.registration_status, "unregistered")
            self.assertEqual(unregistered.metrics["status"], "unavailable")
            self.assertEqual(len(unregistered.artifacts), 1)
            registered = kasane.compare_observations(
                packet, kasane.ExternalReference(
                    path, image_registration=(1, 0, 0, 0, 1, 0),
                ),
            )
            self.assertEqual(registered.metrics["opaque_rgb"]["whole_view"]["mae"], 0)
            self.assertIsNone(registered.change_bounds)
            self.assertEqual(registered.registration["kind"], "image_affine")
            directory = Path(temporary).resolve() / "packet"
            packet.save(directory, profile="report")
            offline = kasane.open_inspection_packet(directory)
            self.assertIsNone(offline.views[0].rgba)
            self.assertEqual(kasane.compare_observations(offline, packet).metrics[
                "opaque_rgb"]["whole_view"]["mae"], 0)

    def test_raw_alpha_and_cross_document_registration_boundaries(self):
        with kasane.Observer(96, 64, 96) as observer:
            raw_request = kasane.RawInspectionRequest((0, 0, 100, 100), (96, 64))
            first = observer.inspect(self.model, {PARAMETER: 0}, request=raw_request)
            second = observer.inspect(self.model, {PARAMETER: 1}, request=raw_request)
            result = kasane.compare_observations(second, first)
            self.assertGreater(result.metrics["compatible_raw_rgba"]["whole_view"]["mae"], 0)
            self.assertEqual(result.metrics["transparent_alpha"]["whole_view"]["status"],
                             "complete")
            self.assertEqual(result.metrics["compatible_raw_rgba"]["target"]["status"],
                             "unavailable")
            other = kasane.Session(DOCUMENT, 100, 100, (50, 50), 10)
            different_document = observer.inspect(other, request=self.request)
            current = observer.inspect(self.model, request=self.request)
            with self.assertRaisesRegex(ValueError, "UNREGISTERED_CANVAS"):
                kasane.compare_observations(current, different_document)
            aligned = kasane.compare_observations(
                current, different_document,
                options=kasane.CompareOptions(canvas_registration=(1, 0, 0, 0, 1, 0)),
            )
            self.assertEqual(aligned.registration_status, "registered")

    def test_inline_baseline_survives_report_profile_reopen(self):
        with TemporaryDirectory() as temporary, kasane.Observer(96, 64, 96) as observer:
            packet = observer.inspect(
                self.model, {PARAMETER: 1}, request=self.request,
                baseline_values={PARAMETER: 0},
            )
            self.assertTrue(packet.capabilities["comparison"])
            self.assertEqual(packet.comparison.registration_status, "registered")
            self.assertIsNotNone(packet.comparison.change_bounds)
            directory = Path(temporary).resolve() / "inline"
            packet.save(directory, profile="report")
            reopened = kasane.open_inspection_packet(directory)
            self.assertEqual(reopened.comparison.artifacts[0].png,
                             packet.comparison.artifacts[0].png)
            self.assertEqual(reopened.comparison.metrics,
                             packet.comparison.metrics)
            self.assertEqual(reopened.comparison.registration,
                             packet.comparison.registration)
            manifest_path = directory / "packet.json"
            manifest = json.loads(manifest_path.read_text())
            manifest["comparison"]["artifacts"][0]["path"] = "../outside.png"
            manifest_path.write_text(json.dumps(manifest))
            with self.assertRaises(ValueError):
                kasane.open_inspection_packet(directory)


class ReportTests(unittest.TestCase):
    @staticmethod
    def model():
        model = kasane.Session(DOCUMENT, 100, 100, (50, 50), 10)
        with model.edit("two parameters") as edit:
            edit.create_parameter(X, "横", 0, 1, 0)
            edit.create_parameter(Y, "纵", 0, 1, 0)
        return model

    def test_grid_missing_cells_actual_values_and_failed_duplicate(self):
        model = self.model()
        request = kasane.InspectionRequest(
            view=kasane.ViewSpec(roi=(0, 0, 100, 100), resolution=(48, 32)),
            channels=("clean",),
        )
        grid = kasane.GridLayout(X, Y, (0, 1), (0, 1))
        with TemporaryDirectory() as temporary, kasane.Observer(48, 32, 48) as observer:
            root = Path(temporary).resolve()
            with self.assertRaisesRegex(ValueError, "fixed_union"):
                observer.inspect_run(
                    model, [{X: 0, Y: 0}], output=root,
                    request=replace(request, view=replace(request.view,
                                                          framing="follow")),
                )
            run = observer.inspect_run(
                model, [{X: 0, Y: 0}, {X: 1, Y: 0}, {X: 0, Y: 1}],
                request=request, output=root, layout=grid, baseline_index=0,
            )
            self.assertEqual(run.status, "complete")
            self.assertEqual(len(run.report["sdk"]["native_binary_sha256"]), 64)
            self.assertEqual(run.report["resources"]["output_bytes"], sum(
                path.stat().st_size for path in run.run_directory.rglob("*")
                if path.is_file()))
            self.assertEqual([cell["status"] for cell in run.pages[0]["cells"]],
                             ["complete", "complete", "complete", "missing"])
            self.assertEqual(len(run.report["comparisons"]), 2)
            self.assertFalse(run.report["samples"][1]["actual"][0]["clamped"])
            self.assertEqual(run.report["samples"][1]["actual"][0]["name"], "横")
            self.assertTrue(run.pages[0]["view_transforms"][0]["label_fallback"])
            self.assertEqual(run.packet(1).views[0].kind, "clean")
            self.assertEqual(kasane.open_inspection_run(run.run_directory).status,
                             "complete")
            with self.assertRaisesRegex(ValueError, "DUPLICATE_GRID_CELL") as failure:
                observer.inspect_run(
                    model, [{X: 1, Y: 0}, {X: 1.5, Y: 0}],
                    request=request, output=root, layout=grid,
                )
            failed = kasane.open_inspection_run(failure.exception.run_directory)
            self.assertEqual(failed.status, "failed")
            self.assertEqual(failed.report["diagnostics"][0]["severity"], "error")
            self.assertTrue(failed.report["samples"][1]["actual"][0]["clamped"])
            image_path = run.run_directory / run.report["samples"][0]["views"][0]["path"]
            image_path.write_bytes(b"tampered")
            with self.assertRaises(ValueError):
                kasane.open_inspection_run(run.run_directory)

    def test_large_contact_sheet_paginates_without_shrinking_views(self):
        model = self.model()
        request = kasane.InspectionRequest(
            view=kasane.ViewSpec(roi=(0, 0, 100, 100), resolution=(2048, 2048)),
            channels=("clean",),
        )
        with TemporaryDirectory() as temporary, kasane.Observer(64, 64, 64) as observer:
            run = observer.inspect_run(
                model, [{X: value} for value in (0, 0.25, 0.5, 1)],
                request=request, output=Path(temporary).resolve(),
                layout=kasane.SequenceLayout(columns=1),
            )
            self.assertEqual(run.status, "complete")
            self.assertEqual(len(run.pages), 2)
            self.assertTrue(all(page["width"] * page["height"] <= 16_000_000
                                for page in run.pages))
            transforms = [entry for page in run.pages for entry in page["view_transforms"]]
            self.assertEqual(len(transforms), 4)
            self.assertTrue(all(entry["view_rect"][2] - entry["view_rect"][0] == 2048
                                for entry in transforms))


if __name__ == "__main__":
    unittest.main()
