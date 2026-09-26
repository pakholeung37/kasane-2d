"""O6 image-coordinate geometry query checks against an observe wheel."""
from pathlib import Path
from tempfile import TemporaryDirectory
import unittest

import kasane


FIXTURES = (Path(__file__).resolve().parents[3] /
            "tests/fixtures/observe_inspection/projects")
MESH = "10000000-0000-4000-8000-000000000015"
TEXTURE = (Path(__file__).resolve().parent / "fixtures/asymmetric-2x2.png").resolve()
PADDING = (FIXTURES / "assets/99f2934890d59becd85db747b678b830d9b0cbdb234e1ef5def77fad1b07b8b7.png").resolve()
SOLID = (Path(__file__).resolve().parent / "fixtures/texture_00.png").resolve()


class GeometryQueryTests(unittest.TestCase):
    def test_shared_edge_returns_both_triangles_and_truncation(self):
        model = kasane.open_project((FIXTURES / "pixel-binding.kasane.json").resolve())
        request = kasane.InspectionRequest(
            view=kasane.ViewSpec(roi=(0, 0, 100, 100), resolution=(100, 100)),
            channels=("clean",),
        )
        with kasane.Observer(100, 100, 100) as observer:
            packet = observer.inspect(model, request=request)
            self.assertTrue(packet.capabilities["geometry_query"])
            mesh = next(item for item in packet.evaluated_frame["drawables"] if item["id"] == MESH)
            indices = mesh["indices"]
            shared = set(indices[:3]) & set(indices[3:6])
            self.assertEqual(len(shared), 2)
            points = [packet.views[0].canvas_to_image(packet.views[0].runtime_to_canvas(
                (mesh["positions"][index]["x"], mesh["positions"][index]["y"])))
                for index in shared]
            midpoint = tuple((points[0][axis] + points[1][axis]) / 2 for axis in (0, 1))
            result = observer.query(packet, view_id=packet.views[0].view_id, point=midpoint)
            self.assertEqual(result.status, "hit")
            same_mesh = [hit for hit in result.hits if hit.object_id == MESH]
            self.assertEqual(len(same_mesh), 2)
            self.assertEqual(len({hit.triangle_key for hit in same_mesh}), 2)
            for hit in same_mesh:
                self.assertAlmostEqual(sum(hit.barycentric), 1)
                self.assertEqual(hit.canvas_point, packet.views[0].image_to_canvas(midpoint))
                self.assertIsNotNone(hit.uv)
                self.assertEqual(hit.coverage_status, "not_requested")
            clipped = observer.query(packet, view_id=packet.views[0].view_id,
                                     point=midpoint, max_hits=1)
            self.assertTrue(clipped.truncated)
            self.assertGreaterEqual(clipped.total, 2)
            self.assertEqual(len(clipped.hits), 1)

    def test_rotated_bbox_corner_is_not_triangle_hit_and_region_is_exact(self):
        model = kasane.open_project((FIXTURES / "rotated-warp.kasane.json").resolve())
        with kasane.Observer(100, 100, 100) as observer:
            packet = observer.inspect(model, request=kasane.InspectionRequest(
                view=kasane.ViewSpec(roi=(0, 0, 100, 100), resolution=(100, 100)),
                channels=("clean",),
            ))
            mesh = next(item for item in packet.objects if item["kind"] == "mesh")
            x0, y0, x1, y1 = mesh["geometry_bounds_canvas"]
            corner = (x0 + 0.1, y0 + 0.1)
            result = observer.query(packet, view_id=packet.views[0].view_id, point=corner)
            self.assertEqual(result.status, "no_hit")
            self.assertEqual(result.total, 0)
            region = (int(x0), int(y0), int(x0) + 1, int(y0) + 1)
            box_result = observer.query(packet, view_id=packet.views[0].view_id,
                                        region=region)
            self.assertEqual(box_result.status, "no_hit")
            broad = observer.query(packet, view_id=packet.views[0].view_id,
                                   region=(int(x0), int(y0), int(x1) + 1, int(y1) + 1))
            self.assertEqual(broad.status, "hit")
            self.assertTrue(all(hit.region_intersection_area > 0 for hit in broad.hits))

    def test_analysis_roundtrip_and_outside(self):
        model = kasane.open_project((FIXTURES / "pixel-binding.kasane.json").resolve())
        with TemporaryDirectory() as temporary, kasane.Observer(64, 64, 64) as observer:
            packet = observer.inspect(model, request=kasane.InspectionRequest(
                view=kasane.ViewSpec(roi=(0, 0, 100, 100), resolution=(64, 64)),
                channels=("clean",),
            ))
            directory = Path(temporary).resolve() / "analysis"
            packet.save(directory, profile="analysis")
            reopened = kasane.open_inspection_packet(directory)
            self.assertTrue(reopened.capabilities["geometry_query"])
            result = observer.query(reopened, view_id=reopened.views[0].view_id,
                                    point=(20, 20))
            self.assertEqual(reopened.query(view_id=reopened.views[0].view_id,
                                            point=(20, 20)), result)
            details = observer.object_details(reopened, object_id=MESH)
            self.assertEqual(details.kind, "mesh")
            self.assertEqual(details.edit_mapping_status,
                             "requires_target_selection")
            self.assertEqual(details.binding_provenance["status"],
                             "source_records_only")
            self.assertTrue(details.binding_provenance["binding_ids"])
            self.assertEqual(result.pixel_probe.presentation_status, "captured")
            self.assertEqual(result.pixel_probe.raw_status, "not_captured")
            outside = observer.query(reopened, view_id=reopened.views[0].view_id,
                                     point=(-1, 20))
            self.assertEqual(outside.status, "outside")
            self.assertIsNone(outside.pixel_probe)
            report = Path(temporary).resolve() / "report"
            packet.save(report, profile="report")
            with self.assertRaises(kasane.ObservationFailure) as failure:
                observer.query(kasane.open_inspection_packet(report),
                               view_id=packet.views[0].view_id, point=(20, 20))
            self.assertEqual(failure.exception.code, "CAPTURE_NOT_AVAILABLE")
            reopened.close()
            with self.assertRaises(kasane.ObservationFailure):
                observer.query(reopened, view_id=reopened.views[0].view_id,
                               point=(20, 20))

    def test_coverage_matches_single_mesh_raw_alpha_and_blend_modes(self):
        def uid(number):
            return f"70000000-0000-4000-8000-{number:012d}"
        model = kasane.Session(uid(1), 100, 100, (50, 50), 10)
        with model.edit("coverage quad") as edit:
            edit.add_png_asset(uid(2), "texture", TEXTURE)
            edit.create_rectangle(uid(3), "quad", uid(2), (10, 10), (90, 90))
        request = kasane.InspectionRequest(
            view=kasane.ViewSpec(roi=(0, 0, 100, 100), resolution=(100, 100)),
            channels=("clean",),
        )
        with kasane.Observer(100, 100, 100) as observer:
            baseline = observer.inspect(model, request=request)
            raw = observer.inspect(model, request=kasane.RawInspectionRequest(
                (0, 0, 100, 100), (100, 100))).views[0]
            original = {}
            for point in ((25.2, 25.1), (75.2, 25.1), (25.2, 75.1), (75.2, 75.1)):
                query = observer.query(baseline, view_id=baseline.views[0].view_id,
                                       point=point, mode="coverage")
                hit = next(item for item in query.hits if item.object_id == uid(3))
                x, y = int(point[0]), int(point[1])
                expected = raw.rgba[(y * 100 + x) * 4 + 3] / 255
                self.assertAlmostEqual(hit.coverage_value, expected, delta=1 / 255)
                self.assertEqual(hit.coverage_status, "complete")
                self.assertEqual(query.sample_point, (x + 0.5, y + 0.5))
                self.assertEqual(query.resources["render_count"], 1)
                self.assertEqual(query.resources["readback_bytes"], 4)
                original[point] = hit.coverage_value
            self.assertGreater(len(set(original.values())), 1)
            for blend in ("additive", "multiplicative"):
                properties = model.mesh_properties(uid(3))
                with model.edit(blend) as edit:
                    edit.update_mesh_properties(uid(3), kasane.MeshProperties(
                        properties.texture_asset_id, properties.appearance,
                        properties.draw_order, blend, properties.enabled,
                        properties.double_sided, properties.inverted_mask,
                        properties.masks,
                    ))
                packet = observer.inspect(model, request=request)
                for point, expected in original.items():
                    query = observer.query(packet, view_id=packet.views[0].view_id,
                                           point=point, mode="coverage")
                    hit = next(item for item in query.hits if item.object_id == uid(3))
                    self.assertAlmostEqual(hit.coverage_value, expected, delta=1 / 255)

    def test_mask_offscreen_gate_region_and_unsupported_composition(self):
        target = "10000000-0000-4000-8000-00000000001a"
        group = "10000000-0000-4000-8000-000000000047"
        model = kasane.open_project((FIXTURES / "masked-offscreen.kasane.json").resolve())
        request = kasane.InspectionRequest(
            focus=kasane.Focus(mesh_ids=(target,)),
            view=kasane.ViewSpec(roi=(0, 0, 100, 100), resolution=(100, 100)),
            channels=("clean", "mask"),
        )
        with kasane.Observer(100, 100, 100) as observer:
            packet = observer.inspect(model, request=request)
            coverage = next(view for view in packet.views if view.kind == "mask_coverage")
            for point in ((50.2, 50.2), (41.2, 41.2)):
                result = observer.query(packet, view_id=packet.views[0].view_id,
                                        point=point, mode="coverage")
                hit = next(item for item in result.hits if item.object_id == target)
                x, y = int(point[0]), int(point[1])
                expected = coverage.rgba[(y * 100 + x) * 4] / 255
                self.assertAlmostEqual(hit.coverage_value, expected, delta=1 / 255)
            front = observer.query(packet, view_id=packet.views[0].view_id,
                                   point=(50.2, 50.2), mode="frontmost_covered")
            self.assertEqual(front.frontmost_object_id,
                             "10000000-0000-4000-8000-00000000001b")
            below = observer.query(packet, view_id=packet.views[0].view_id,
                                   point=(41.2, 41.2), mode="frontmost_covered")
            self.assertEqual(below.frontmost_object_id,
                             "10000000-0000-4000-8000-000000000019")
            region = observer.query(packet, view_id=packet.views[0].view_id,
                                    region=(35, 35, 65, 65), mode="coverage")
            hit = next(item for item in region.hits if item.object_id == target)
            self.assertGreater(hit.coverage_pixel_count, 0)
            self.assertIsNotNone(hit.coverage_bounds)
            self.assertLess(hit.coverage_pixel_count, 30 * 30)
            layer = model.offscreen(group)
            with model.edit("destination-reading layer") as edit:
                edit.replace_offscreen(layer._replace(blend_mode=1))
            unsupported = observer.inspect(model, request=kasane.InspectionRequest(
                focus=request.focus, view=request.view, channels=("clean",),
            ))
            result = observer.query(unsupported, view_id=unsupported.views[0].view_id,
                                    point=(50.2, 50.2), mode="coverage")
            target_hits = [item for item in result.hits if item.object_id == target]
            self.assertTrue(target_hits)
            self.assertEqual(target_hits[0].coverage_status, "unsupported_composition")
            self.assertIsNone(target_hits[0].coverage_value)
            pick = observer.query(unsupported, view_id=unsupported.views[0].view_id,
                                  point=(50.2, 50.2), mode="frontmost_covered")
            self.assertEqual(pick.status, "unsupported_composition")
            self.assertIsNone(pick.frontmost_object_id)
            clean = observer.inspect(model, request=kasane.InspectionRequest(
                focus=request.focus, view=request.view, channels=("clean",),
            ))
            self.assertEqual(clean.views[0].rgba, unsupported.views[0].rgba)

    def test_frontmost_rule_keeps_all_covered_candidates(self):
        def uid(number):
            return f"80000000-0000-4000-8000-{number:012d}"
        model = kasane.Session(uid(1), 100, 100, (50, 50), 10)
        with model.edit("overlap") as edit:
            edit.add_png_asset(uid(2), "texture", TEXTURE)
            edit.create_rectangle(uid(3), "rear", uid(2), (20, 20), (80, 80))
            edit.create_rectangle(uid(4), "front", uid(2), (20, 20), (80, 80))
        with kasane.Observer(100, 100, 100) as observer:
            packet = observer.inspect(model, request=kasane.InspectionRequest(
                view=kasane.ViewSpec(roi=(0, 0, 100, 100), resolution=(100, 100)),
                channels=("clean",),
            ))
            result = observer.query(packet, view_id=packet.views[0].view_id,
                                    point=(50.2, 50.2), mode="frontmost_covered")
            covered = {hit.object_id for hit in result.hits if
                       hit.coverage_status == "complete" and hit.coverage_value > 0}
            self.assertEqual(covered, {uid(3), uid(4)})
            order = [command["DrawMesh"]["mesh_id"] for command in
                     packet.evaluated_frame["render_plan"] if "DrawMesh" in command]
            self.assertEqual(result.frontmost_object_id, order[-1])
            self.assertEqual(result.pick_rule,
                             "frontmost_covered_not_color_contribution")

    def test_transparent_texel_is_geometry_without_coverage(self):
        def uid(number):
            return f"90000000-0000-4000-8000-{number:012d}"
        model = kasane.Session(uid(1), 100, 100, (50, 50), 10)
        with model.edit("transparent border") as edit:
            edit.add_png_asset(uid(2), "padding", PADDING)
            edit.create_rectangle(uid(3), "quad", uid(2), (10, 10), (90, 90))
        with kasane.Observer(100, 100, 100) as observer:
            packet = observer.inspect(model, request=kasane.InspectionRequest(
                view=kasane.ViewSpec(roi=(0, 0, 100, 100), resolution=(100, 100)),
                channels=("clean",),
            ))
            transparent = (15.2, 50.2)
            geometry = observer.query(packet, view_id=packet.views[0].view_id,
                                      point=transparent, mode="geometry")
            coverage = observer.query(packet, view_id=packet.views[0].view_id,
                                      point=transparent, mode="coverage")
            opaque = observer.query(packet, view_id=packet.views[0].view_id,
                                    point=(50.2, 50.2), mode="coverage")
            self.assertEqual(geometry.status, "hit")
            self.assertEqual(coverage.status, "no_hit")
            self.assertTrue(all(hit.coverage_value == 0 for hit in coverage.hits))
            self.assertEqual(opaque.status, "hit")
            self.assertTrue(any(hit.coverage_value > 0 for hit in opaque.hits))

    def test_mesh_mask_and_inversion_gate_coverage(self):
        def uid(number):
            return f"a0000000-0000-4000-8000-{number:012d}"
        model = kasane.Session(uid(1), 100, 100, (50, 50), 10)
        with model.edit("mask gate") as edit:
            edit.add_png_asset(uid(2), "texture", SOLID)
            edit.create_rectangle(uid(3), "mask", uid(2), (40, 40), (60, 60))
            edit.create_rectangle(uid(4), "target", uid(2), (20, 20), (80, 80))
        properties = model.mesh_properties(uid(4))
        with model.edit("attach mask") as edit:
            edit.update_mesh_properties(uid(4), kasane.MeshProperties(
                properties.texture_asset_id, properties.appearance,
                properties.draw_order, properties.blend_mode, properties.enabled,
                properties.double_sided, False, [uid(3)],
            ))
        request = kasane.InspectionRequest(
            view=kasane.ViewSpec(roi=(0, 0, 100, 100), resolution=(100, 100)),
            channels=("clean",),
        )
        with kasane.Observer(100, 100, 100) as observer:
            def target_alpha(position):
                packet = observer.inspect(model, request=request)
                result = observer.query(packet, view_id=packet.views[0].view_id,
                                        point=position, mode="coverage")
                return next(hit.coverage_value for hit in result.hits
                            if hit.object_id == uid(4))
            self.assertGreater(target_alpha((50.2, 50.2)), 0)
            self.assertEqual(target_alpha((30.2, 50.2)), 0)
            properties = model.mesh_properties(uid(4))
            with model.edit("invert mask") as edit:
                edit.update_mesh_properties(uid(4), kasane.MeshProperties(
                    properties.texture_asset_id, properties.appearance,
                    properties.draw_order, properties.blend_mode, properties.enabled,
                    properties.double_sided, True, properties.masks,
                ))
            self.assertEqual(target_alpha((50.2, 50.2)), 0)
            self.assertGreater(target_alpha((30.2, 50.2)), 0)

    def test_coverage_region_budget_preflight(self):
        model = kasane.open_project((FIXTURES / "pixel-binding.kasane.json").resolve())
        with kasane.Observer(600, 600, 600) as observer:
            packet = observer.inspect(model, request=kasane.InspectionRequest(
                view=kasane.ViewSpec(roi=(0, 0, 100, 100), resolution=(600, 600)),
                channels=("clean",),
            ))
            with self.assertRaises(kasane.ObservationFailure) as failure:
                observer.query(packet, view_id=packet.views[0].view_id,
                               region=(0, 0, 600, 600), mode="coverage")
            self.assertEqual(failure.exception.code, "OBSERVATION_BUDGET_EXCEEDED")


if __name__ == "__main__":
    unittest.main()
