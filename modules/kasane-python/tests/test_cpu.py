"""Run against an installed wheel from outside the source tree."""

import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
from tempfile import TemporaryDirectory
import unittest

import kasane


DOCUMENT = "00000000-0000-4000-8000-000000000001"
ASSET = "00000000-0000-4000-8000-000000000002"
MESH = "00000000-0000-4000-8000-000000000003"
PARAMETER = "00000000-0000-4000-8000-000000000004"
BINDING = "00000000-0000-4000-8000-000000000005"
PART = "00000000-0000-4000-8000-000000000006"
CHILD_PART = "00000000-0000-4000-8000-000000000007"
ROTATION = "00000000-0000-4000-8000-000000000008"
WARP = "00000000-0000-4000-8000-000000000009"
SCENE_PART = "00000000-0000-4000-8000-000000000010"
SCENE_ROTATION = "00000000-0000-4000-8000-000000000011"
SCENE_WARP = "00000000-0000-4000-8000-000000000012"
OFFSCREEN = "00000000-0000-4000-8000-000000000013"
MESH_B = "00000000-0000-4000-8000-000000000014"
GLUE = "00000000-0000-4000-8000-000000000015"
BLEND_PARAMETER = "00000000-0000-4000-8000-000000000016"
BLEND_TABLE = "00000000-0000-4000-8000-000000000017"
BLEND_CONSTRAINT = "00000000-0000-4000-8000-000000000018"
BLEND_MESH = "00000000-0000-4000-8000-000000000019"
BLEND_WARP = "00000000-0000-4000-8000-000000000020"
BLEND_ROTATION = "00000000-0000-4000-8000-000000000021"
BLEND_PART = "00000000-0000-4000-8000-000000000022"
BLEND_GLUE = "00000000-0000-4000-8000-000000000023"
BLEND_OFFSCREEN = "00000000-0000-4000-8000-000000000024"
TEXTURE = Path(__file__).resolve().parents[3] / "examples/sdk/asymmetric-2x2.png"
EXTERNAL = Path(__file__).resolve().parents[3] / "tests/fixtures/external_v50"


def session():
    return kasane.Session(DOCUMENT, 100, 100, (50, 50), 10)


class CpuWheelTests(unittest.TestCase):
    def test_shared_authoring_contract_matches_expected_results(self):
        spec = json.loads((TEXTURE.parent / "authoring-contract.json").read_text())
        model = kasane.Session(spec["document_id"], 100, 100, (50, 50), 10)
        before = model.version
        with self.assertRaises(kasane.SdkFailure) as failure:
            with model.edit("invalid") as edit:
                edit.create_rectangle(
                    spec["mesh_id"], spec["initial_name"], spec["missing_asset_id"],
                    tuple(spec["minimum"]), tuple(spec["maximum"]),
                )
        self.assertEqual(failure.exception.code, spec["invalid_create_code"])
        self.assertEqual(model.version, before)
        self.assertEqual(model.history_lengths(), (0, 0))
        with model.edit("create") as edit:
            edit.add_png_asset(spec["asset_id"], "texture", TEXTURE)
            edit.create_rectangle(
                spec["mesh_id"], spec["initial_name"], spec["asset_id"],
                tuple(spec["minimum"]), tuple(spec["maximum"]),
            )
        self.assertEqual(model.evaluate({}).drawables[0].positions[0],
                         tuple(spec["evaluated_first_position"]))
        with model.edit("rename") as edit:
            edit.rename_mesh(spec["mesh_id"], spec["updated_name"])
        self.assertEqual(model.mesh(spec["mesh_id"]).name, spec["updated_name"])
        model.undo()
        self.assertEqual(model.mesh(spec["mesh_id"]).name, spec["initial_name"])
        model.redo()
        self.assertEqual(model.mesh(spec["mesh_id"]).name, spec["updated_name"])

    def test_validation_capabilities_identify_the_engine(self):
        capabilities = kasane.capabilities()
        self.assertIn("purism_core_validation", capabilities)
        self.assertFalse(capabilities["official_core_validation"])

    def test_create_save_reopen_and_snapshot_copy(self):
        model = session()
        with model.edit("create") as edit:
            edit.add_png_asset(ASSET, "texture", TEXTURE)
            edit.create_rectangle(MESH, "face", ASSET, (40, 40), (60, 60))
        self.assertEqual(model.mesh_ids(), [MESH])
        snapshot = model.mesh(MESH)
        self.assertEqual(snapshot.name, "face")
        snapshot.positions[0] = (999, 999)
        self.assertEqual(model.mesh(MESH).positions[0], (40, 40))
        self.assertEqual(model.evaluate({}).drawables[0].positions[0], (-1, 1))
        with TemporaryDirectory() as directory:
            destination = Path(directory).resolve() / "project"
            result = model.save(destination)
            self.assertTrue(result.manifest.is_file())
            self.assertEqual(model.project_path, result.manifest)
            self.assertFalse(model.modified)
            reopened = kasane.open_project(destination)
            self.assertEqual(reopened.mesh_ids(), [MESH])
            self.assertEqual(reopened.mesh(MESH).positions, model.mesh(MESH).positions)
            self.assertEqual(reopened.mesh(MESH).vertex_ids, model.mesh(MESH).vertex_ids)
            self.assertEqual(reopened.diagnose_resources(), [])
        model.undo()
        self.assertIsNone(model.mesh(MESH))
        model.redo()
        self.assertFalse(model.modified)

    def test_rollback_and_structured_error(self):
        model = session()
        with self.assertRaisesRegex(ValueError, "cancel"):
            with model.edit("cancel") as edit:
                edit.add_png_asset(ASSET, "texture", TEXTURE)
                raise ValueError("cancel")
        self.assertEqual(model.asset_ids(), [])
        with self.assertRaises(kasane.SdkFailure) as failure:
            with model.edit("missing asset") as edit:
                edit.create_rectangle(MESH, "face", ASSET, (40, 40), (60, 60))
        self.assertEqual(failure.exception.code, "MISSING_ASSET")
        self.assertEqual(failure.exception.operation, "create_mesh")
        self.assertEqual(model.mesh_ids(), [])

        with self.assertRaises(kasane.SdkFailure) as aborted:
            with model.edit("caught invalid input") as edit:
                edit.add_png_asset(ASSET, "texture", TEXTURE)
                try:
                    edit.create_rectangle(MESH, "face", ASSET, ("bad", 40), (60, 60))
                except TypeError:
                    pass
        self.assertEqual(aborted.exception.code, "EDIT_ABORTED")
        self.assertEqual(model.asset_ids(), [])

    def test_active_edit_blocks_conflicting_operations_and_releases_on_cancel(self):
        model = session()
        with model.edit("create") as edit:
            edit.add_png_asset(ASSET, "texture", TEXTURE)
            edit.create_rectangle(MESH, "face", ASSET, (40, 40), (60, 60))
        before = model.version
        with TemporaryDirectory() as directory:
            destination = Path(directory).resolve() / "project"
            edit = model.edit("rename")
            edit.rename_mesh(MESH, "changed")
            for action in (lambda: model.edit("nested"),
                           lambda: model.save(destination), model.undo,
                           lambda: model.new_project(DOCUMENT, 100, 100, (50, 50), 10)):
                with self.assertRaises(kasane.SdkFailure) as failure:
                    action()
                self.assertEqual(failure.exception.code, "EDIT_ACTIVE")
            self.assertFalse(destination.exists())
            self.assertEqual(model.version, before)
            edit.cancel()
            self.assertEqual(model.mesh(MESH).name, "face")
            with model.edit("rename") as next_edit:
                next_edit.rename_mesh(MESH, "changed")
            self.assertEqual(model.mesh(MESH).name, "changed")

    def test_aborted_edit_cannot_release_a_newer_edit(self):
        model = session()
        old = model.edit("bad")
        with self.assertRaises(kasane.SdkFailure):
            old.create_rectangle(MESH, "face", ASSET, (40, 40), (60, 60))
        current = model.edit("current")
        old.cancel()
        del old
        with self.assertRaises(kasane.SdkFailure) as failure:
            model.edit("nested")
        self.assertEqual(failure.exception.code, "EDIT_ACTIVE")
        current.cancel()

    def test_repeated_parameter_replacement_uses_candidate_state(self):
        model = session()
        with model.edit("parameter") as edit:
            edit.create_parameter(PARAMETER, "p", 0, 1, 0.5)
        with model.edit("two replacements") as edit:
            edit.replace_parameter(PARAMETER, "p", 0, 1, 0.5, kind="blend_shape")
            self.assertEqual(edit.parameter(PARAMETER).kind, "blend_shape")
            self.assertEqual(model.parameter(PARAMETER).kind, "normal")
            edit.replace_parameter(PARAMETER, "renamed", 0, 1, 0.5)
            self.assertEqual(edit.parameter(PARAMETER).name, "renamed")
        self.assertEqual(model.parameter(PARAMETER).kind, "blend_shape")
        self.assertEqual(model.parameter(PARAMETER).name, "renamed")
        model.undo()
        self.assertEqual(model.parameter(PARAMETER).kind, "normal")

    def test_full_evaluation_snapshot_exposes_render_attributes(self):
        model = session()
        with model.edit("mesh") as edit:
            edit.add_png_asset(ASSET, "texture", TEXTURE)
            edit.create_rectangle(MESH, "face", ASSET, (40, 40), (60, 60))
        properties = model.mesh_properties(MESH)
        with model.edit("opacity") as edit:
            edit.update_mesh_properties(MESH, kasane.MeshProperties(
                properties.texture_asset_id, kasane.Appearance(0.5),
                properties.draw_order, properties.blend_mode, properties.enabled,
                properties.double_sided, properties.inverted_mask, properties.masks,
            ))
        snapshot = model.evaluate_snapshot({})
        self.assertEqual(snapshot.version, model.version)
        self.assertEqual(snapshot.source_revision, model.evaluation_revision)
        self.assertEqual(snapshot.drawables[0].positions, model.evaluate({}).drawables[0].positions)
        self.assertEqual(snapshot.drawables[0].opacity, 0.5)
        self.assertTrue(snapshot.drawables[0].visible)
        self.assertEqual(snapshot.drawables[0].texture_asset_id, ASSET)
        self.assertIn(("draw_mesh", MESH), snapshot.render_plan)
        self.assertEqual(model.preview_snapshot().drawables[0].id, MESH)

    def test_update_positions_publishes_one_edit(self):
        model = session()
        with model.edit("create") as edit:
            edit.add_png_asset(ASSET, "texture", TEXTURE)
            edit.create_rectangle(MESH, "face", ASSET, (40, 40), (60, 60))
        before = model.version
        mesh = model.mesh(MESH)
        shifted = [(x + 10, y) for x, y in mesh.positions]
        with model.edit("move", expected_version=before) as edit:
            edit.update_positions(MESH, mesh.vertex_ids, shifted)
        self.assertEqual(model.version[2], before[2] + 1)
        self.assertEqual(model.evaluate({}).drawables[0].positions[0], (0, 1))

    def test_import_edit_save_and_export(self):
        model = session()
        imported = model.import_model3(EXTERNAL / "model.model3.json")
        self.assertEqual(imported.moc_version, 5)
        self.assertEqual(imported.diagnostics, [])
        mesh_id = model.mesh_ids()[0]
        with model.edit("rename") as edit:
            edit.rename_mesh(mesh_id, "changed")
        with TemporaryDirectory() as directory:
            root = Path(directory).resolve()
            model.save(root / "project")
            exported = model.export_package(root / "package")
            self.assertTrue(exported.published)
            self.assertTrue((root / "package/model.moc3").is_file())
            reopened = kasane.open_project(root / "project")
            self.assertEqual(reopened.mesh(mesh_id).name, "changed")
            self.assertEqual(reopened.diagnose_resources(), [])
        bare = session()
        bare_import = bare.import_bare_moc3(
            EXTERNAL / "model.moc3", {0: EXTERNAL / "texture_00.png"}
        )
        self.assertEqual(bare_import.moc_version, 5)
        self.assertEqual(bare_import.diagnostics, [])

    def test_parameter_binding_samples_and_reopens(self):
        model = session()
        with model.edit("create") as edit:
            edit.add_png_asset(ASSET, "texture", TEXTURE)
            edit.create_rectangle(MESH, "face", ASSET, (40, 40), (60, 60))
        base = model.mesh(MESH).positions
        shifted = [(x + 10, y) for x, y in base]
        with model.edit("bind") as edit:
            edit.create_parameter(PARAMETER, "open", 0, 1, 0)
            edit.create_mesh_binding(
                BINDING,
                MESH,
                [kasane.Axis(PARAMETER, [0, 1])],
                [kasane.MeshKeyform([0], base), kasane.MeshKeyform([1], shifted)],
            )
        self.assertEqual(model.parameter_ids(), [PARAMETER])
        self.assertEqual(model.binding_ids(), [BINDING])
        binding = model.binding(BINDING)
        self.assertEqual(binding.mesh_id, MESH)
        self.assertEqual(binding.axes, [kasane.Axis(PARAMETER, [0, 1])])
        self.assertEqual(model.binding_for_mesh(MESH), binding)
        binding.keyforms[0].positions[0] = (999, 999)
        self.assertEqual(model.binding(BINDING).keyforms[0].positions[0], base[0])
        self.assertEqual(model.parameter(PARAMETER).name, "open")
        middle = model.evaluate({PARAMETER: 0.5})
        self.assertEqual(middle.parameters[0].value, 0.5)
        self.assertEqual(middle.drawables[0].positions[0], (-0.5, 1))
        self.assertEqual(model.evaluate({PARAMETER: 2}).parameters[0].value, 1)
        preview_before = model.preview_revision
        self.assertTrue(model.set_preview_values({PARAMETER: 0.5}))
        self.assertEqual(model.preview_values, {PARAMETER: 0.5})
        self.assertGreater(model.preview_revision, preview_before)
        self.assertEqual(model.preview_frame().drawables[0].positions[0], (-0.5, 1))
        self.assertGreaterEqual(model.preview_evaluation_count, 1)
        self.assertEqual(model.evaluate({PARAMETER: 0}).drawables[0].positions[0], (-1, 1))
        revision = model.preview_revision
        with self.assertRaises(kasane.SdkFailure):
            model.set_preview_parameter("00000000-0000-4000-8000-000000000099", 1)
        self.assertEqual(model.preview_revision, revision)
        self.assertTrue(model.set_preview_parameter(PARAMETER, 1))
        self.assertEqual(model.preview_frame().drawables[0].positions[0], (0, 1))
        self.assertTrue(model.reset_preview_values())
        self.assertEqual(model.preview_values, {})
        with TemporaryDirectory() as directory:
            destination = Path(directory).resolve() / "project"
            model.save(destination)
            reopened = kasane.open_project(destination)
            self.assertEqual(reopened.evaluate({PARAMETER: 0.5}), middle)

    def test_queries_geometry_history_and_events(self):
        model = session()
        self.assertEqual(model.document_id, DOCUMENT)
        self.assertEqual(model.canvas.width, 100)
        self.assertEqual(model.canvas.origin_x, 50)
        with model.edit("create") as edit:
            edit.add_png_asset(ASSET, "texture", TEXTURE)
            edit.create_rectangle(MESH, "face", ASSET, (40, 40), (60, 60))
        self.assertEqual(model.validate_structure(), [])
        self.assertEqual(model.asset(ASSET).width, 2)
        self.assertEqual(model.asset(ASSET).height, 2)
        self.assertEqual(model.references_to(ASSET), [MESH])
        self.assertEqual(model.binding_ids(), [])
        self.assertEqual(model.part_ids(), [])
        self.assertEqual(model.transform_ids(), [])
        self.assertEqual(model.scene_binding_ids(), [])
        self.assertEqual(model.blend_key_table_ids(), [])
        self.assertEqual(model.blend_constraint_ids(), [])
        self.assertEqual(model.blend_binding_ids(), [])
        self.assertEqual(model.glue_ids(), [])
        self.assertEqual(model.offscreen_ids(), [])
        self.assertEqual(model.find_meshes_by_name("face")[0].id, MESH)
        self.assertEqual(model.require_unique_mesh("face").id, MESH)
        with self.assertRaises(kasane.SdkFailure) as failure:
            model.require_unique_mesh("missing")
        self.assertEqual(failure.exception.code, "NOT_FOUND")
        geometry = model.geometry(MESH)
        self.assertEqual(geometry.space, "canvas_pixels")
        self.assertEqual(geometry.parent_id, None)
        self.assertEqual(geometry.version, model.version)
        self.assertEqual(len(geometry.uvs), 4)
        self.assertEqual(len(geometry.triangles), 2)
        geometry.positions[0] = (999, 999)
        self.assertEqual(model.geometry(MESH).positions[0], (40, 40))
        self.assertEqual(model.history_lengths(), (1, 0))
        self.assertEqual(model.history_state().undo_steps, 1)
        self.assertGreater(model.estimated_content_bytes(), 0)
        self.assertGreaterEqual(model.evaluation_revision, 1)
        events = model.drain_events()
        self.assertEqual(events[0].label, "create")
        self.assertEqual(model.drain_events(), [])

    def test_new_project_rejects_stale_version_and_resets_session(self):
        model = session()
        with model.edit("create") as edit:
            edit.add_png_asset(ASSET, "texture", TEXTURE)
            edit.create_rectangle(MESH, "face", ASSET, (40, 40), (60, 60))
        before = model.version
        stale = (before[0], before[1], before[2] + 1)
        with self.assertRaises(kasane.SdkFailure) as failure:
            model.new_project(DOCUMENT, 200, 200, (100, 100), 20, stale)
        self.assertEqual(failure.exception.code, "STALE_VERSION")
        self.assertEqual(model.version, before)
        after = model.new_project(DOCUMENT, 200, 200, (100, 100), 20, before)
        self.assertEqual(after[1], before[1] + 1)
        self.assertEqual(model.canvas.width, 200)
        self.assertEqual(model.asset_ids(), [])
        self.assertEqual(model.mesh_ids(), [])
        self.assertEqual(model.history_lengths(), (0, 0))
        self.assertEqual(model.preview_values, {})
        self.assertIsNone(model.project_path)

    def test_canvas_parameter_replace_and_erase_are_atomic(self):
        model = session()
        with model.edit("create") as edit:
            edit.create_parameter(PARAMETER, "old", 0, 1, 0)
        before = model.version
        with model.edit("replace") as edit:
            edit.replace_canvas(200, 100, (100, 50), 20)
            edit.replace_parameter(PARAMETER, "new", -1, 1, 0.5)
        self.assertEqual(model.version[2], before[2] + 1)
        self.assertEqual(model.canvas.width, 200)
        self.assertEqual(model.parameter(PARAMETER).name, "new")
        self.assertEqual(model.parameter(PARAMETER).minimum, -1)
        model.undo()
        self.assertEqual(model.canvas.width, 100)
        self.assertEqual(model.parameter(PARAMETER).name, "old")
        model.redo()
        with model.edit("erase") as edit:
            edit.erase_object(PARAMETER)
        self.assertIsNone(model.parameter(PARAMETER))
        model.undo()
        self.assertEqual(model.parameter(PARAMETER).name, "new")

    def test_parent_edit_errors_roll_back_entire_batch(self):
        operations = [
            lambda edit: edit.set_organization_parent("missing", ""),
            lambda edit: edit.set_transform_parent("missing", None),
            lambda edit: edit.set_transform_part("missing", None),
            lambda edit: edit.set_deform_parent("missing", "parent"),
            lambda edit: edit.set_mesh_part("missing", "part"),
        ]
        for operation in operations:
            with self.subTest(operation=operation):
                model = session()
                before = model.version
                with self.assertRaises(kasane.SdkFailure):
                    with model.edit("bad parent") as edit:
                        edit.create_parameter(PARAMETER, "temporary", 0, 1, 0)
                        operation(edit)
                self.assertEqual(model.version, before)
                self.assertEqual(model.parameter_ids(), [])

    def test_handles_keep_identity_and_expire_after_undo(self):
        model = session()
        with model.edit("create") as edit:
            edit.add_png_asset(ASSET, "texture", TEXTURE)
            edit.create_rectangle(MESH, "face", ASSET, (40, 40), (60, 60))
        handle = model.handle("mesh", MESH)
        self.assertEqual(handle.id, MESH)
        self.assertEqual(handle.kind, "mesh")
        model.resolve_handle(handle)
        self.assertEqual(model.mesh_by_handle(handle), model.mesh(MESH))
        with model.edit("rename") as edit:
            edit.rename_mesh(MESH, "renamed")
        self.assertEqual(model.mesh_by_handle(handle).name, "renamed")
        with self.assertRaises(kasane.SdkFailure) as wrong_kind:
            model.mesh_by_handle(model.handle("asset", ASSET))
        self.assertEqual(wrong_kind.exception.code, "WRONG_OBJECT_KIND")
        with self.assertRaises(ValueError):
            model.handle("bad kind", MESH)
        model.undo()
        model.undo()
        model.redo()
        with self.assertRaises(kasane.SdkFailure) as stale:
            model.resolve_handle(handle)
        self.assertEqual(stale.exception.code, "STALE_HANDLE")

    def test_draw_order_groups_and_history_limits(self):
        model = kasane.Session.with_history_limits(DOCUMENT, 100, 100, (50, 50), 10, 1, 10**7)
        self.assertEqual(model.history_state().max_steps, 1)
        self.assertIsNone(model.draw_order_groups)
        with model.edit("create") as edit:
            edit.add_png_asset(ASSET, "texture", TEXTURE)
            edit.create_rectangle(MESH, "face", ASSET, (40, 40), (60, 60))
        group = kasane.DrawOrderGroup("", [MESH], -100, 100)
        with model.edit("order") as edit:
            edit.replace_draw_order_groups([group])
        self.assertEqual(model.draw_order_groups, [group])
        self.assertEqual(model.history_lengths(), (1, 0))
        with self.assertRaises(kasane.SdkFailure) as invalid:
            with model.edit("invalid order") as edit:
                edit.replace_draw_order_groups([kasane.DrawOrderGroup("", [], 0, 0)])
        self.assertEqual(invalid.exception.code, "INVALID_DRAW_GROUP")
        self.assertEqual(model.draw_order_groups, [group])
        model.undo()
        self.assertIsNone(model.draw_order_groups)

    def test_parts_and_organization_parent_publish(self):
        model = session()
        with model.edit("base") as edit:
            edit.add_png_asset(ASSET, "texture", TEXTURE)
            edit.create_rectangle(MESH, "face", ASSET, (40, 40), (60, 60))
        with model.edit("parts") as edit:
            edit.create_part(PART, "root")
            edit.create_part(CHILD_PART, "child")
            edit.set_organization_parent(CHILD_PART, PART)
            edit.set_mesh_part(MESH, CHILD_PART)
        self.assertEqual(model.part_ids(), [PART, CHILD_PART])
        child = model.part(CHILD_PART)
        self.assertEqual(child.parent_id, PART)
        self.assertEqual(child.runtime_id, CHILD_PART)
        self.assertEqual(model.validate_structure(), [])
        with model.edit("rename part") as edit:
            edit.replace_part(CHILD_PART, "renamed", PART, False, 2.5)
        self.assertEqual(model.part(CHILD_PART).name, "renamed")
        self.assertEqual(model.part(CHILD_PART).runtime_id, CHILD_PART)
        self.assertFalse(model.part(CHILD_PART).enabled)
        self.assertEqual(model.part(CHILD_PART).draw_order, 2.5)
        model.undo()
        self.assertEqual(model.part(CHILD_PART).name, "child")

    def test_transform_hierarchy_and_updates(self):
        model = session()
        with model.edit("base") as edit:
            edit.add_png_asset(ASSET, "texture", TEXTURE)
            edit.create_rectangle(MESH, "face", ASSET, (0, 0), (1, 1))
        points = [(0, 0), (1, 0), (0, 1), (1, 1)]
        with model.edit("hierarchy") as edit:
            edit.create_part(PART, "root")
            edit.create_part(CHILD_PART, "child")
            edit.create_rotation_transform(
                ROTATION, "rotate", kasane.RotationData(0, kasane.RotationPose((0, 0))), PART
            )
            edit.create_warp_transform(
                WARP, "warp", kasane.WarpData(1, 1, True, points), PART
            )
            edit.set_transform_parent(WARP, ROTATION)
            edit.set_transform_part(WARP, CHILD_PART)
            edit.set_deform_parent(MESH, WARP)
            edit.set_mesh_part(MESH, CHILD_PART)
        self.assertEqual(model.transform_ids(), [ROTATION, WARP])
        self.assertEqual(model.transform(WARP).parent_id, ROTATION)
        self.assertEqual(model.transform(WARP).part_id, CHILD_PART)
        self.assertEqual(model.transform(WARP).warp.points, points)
        self.assertEqual(model.geometry(MESH).space, "parent_local")
        self.assertEqual(model.geometry(MESH).parent_id, WARP)
        self.assertEqual(model.validate_structure(), [])
        with model.edit("update") as edit:
            edit.update_rotation(
                ROTATION, kasane.RotationData(5, kasane.RotationPose((0, 0), angle=10))
            )
            edit.update_warp_points(WARP, [(x + 0.1, y) for x, y in points])
        self.assertEqual(model.transform(ROTATION).rotation.base_angle, 5)
        self.assertEqual(model.transform(ROTATION).rotation.pose.angle, 10)
        self.assertAlmostEqual(model.transform(WARP).warp.points[0][0], 0.1, places=5)
        model.undo()
        self.assertEqual(model.transform(ROTATION).rotation.base_angle, 0)

    def test_replace_transform_preserves_identity_and_full_fields(self):
        model = session()
        points = [(0, 0), (1, 0), (0, 1), (1, 1)]
        with model.edit("transforms") as edit:
            edit.create_rotation_transform(
                ROTATION, "rotate", kasane.RotationData(0, kasane.RotationPose((0, 0)))
            )
            edit.create_warp_transform(
                WARP, "warp", kasane.WarpData(1, 1, True, points)
            )
        original = model.transform(ROTATION)
        warp = model.transform(WARP)
        with model.edit("replace transforms") as edit:
            edit.replace_transform(original._replace(
                name="turned", enabled=False,
                appearance=kasane.Appearance(0.5),
                rotation=kasane.RotationData(15, kasane.RotationPose((2, 3), angle=20)),
            ))
            edit.replace_transform(warp._replace(
                name="curved", warp=kasane.WarpData(1, 1, True, [
                    (0.25, 0), (1, 0), (0, 1), (1, 1)
                ]),
            ))
        replaced = model.transform(ROTATION)
        self.assertEqual(replaced.runtime_id, original.runtime_id)
        self.assertEqual(replaced.name, "turned")
        self.assertFalse(replaced.enabled)
        self.assertAlmostEqual(replaced.appearance.opacity, 0.5)
        self.assertEqual(replaced.rotation.base_angle, 15)
        self.assertEqual(replaced.rotation.pose.origin, (2, 3))
        self.assertAlmostEqual(model.transform(WARP).warp.points[0][0], 0.25)
        with TemporaryDirectory() as directory:
            destination = Path(directory).resolve() / "project"
            model.save(destination)
            reopened = kasane.open_project(destination)
            self.assertEqual(reopened.transform(ROTATION).runtime_id, original.runtime_id)
            self.assertAlmostEqual(reopened.transform(ROTATION).appearance.opacity, 0.5)
        version = model.version
        with self.assertRaises(ValueError):
            with model.edit("invalid transform") as edit:
                edit.replace_transform(original._replace(kind="warp"))
        self.assertEqual(model.version, version)
        model.undo()
        self.assertEqual(model.transform(ROTATION).name, original.name)

    def test_offscreen_create_replace_and_rollback(self):
        model = session()
        with model.edit("part and offscreen") as edit:
            edit.create_part(PART, "owner")
            edit.create_offscreen(kasane.OffscreenSpec(
                OFFSCREEN, "layer", PART, keyforms=[kasane.OffscreenKeyform(0.5)],
            ))
        original = model.offscreen(OFFSCREEN)
        self.assertEqual(original.runtime_id, OFFSCREEN)
        self.assertEqual(original.keyforms[0].opacity, 0.5)
        original.keyforms[0] = kasane.OffscreenKeyform(0)
        self.assertEqual(model.offscreen(OFFSCREEN).keyforms[0].opacity, 0.5)
        with model.edit("replace offscreen") as edit:
            edit.replace_offscreen(original._replace(
                name="soft layer", flags=0, part_keyform_indices=[0],
                keyforms=[kasane.OffscreenKeyform(0.75, (0.8, 1, 1), (0, 0.1, 0))],
            ))
        replaced = model.offscreen(OFFSCREEN)
        self.assertEqual(replaced.name, "soft layer")
        self.assertEqual(replaced.runtime_id, OFFSCREEN)
        self.assertEqual(replaced.part_keyform_indices, [0])
        self.assertAlmostEqual(replaced.keyforms[0].opacity, 0.75)
        with TemporaryDirectory() as directory:
            destination = Path(directory).resolve() / "project"
            model.save(destination)
            reopened = kasane.open_project(destination)
            self.assertEqual(reopened.offscreen(OFFSCREEN).part_keyform_indices, [0])
        version = model.version
        with self.assertRaises(kasane.SdkFailure) as error:
            with model.edit("invalid index") as edit:
                edit.replace_offscreen(replaced._replace(part_keyform_indices=[9]))
        self.assertEqual(error.exception.code, "INDEX_OUT_OF_BOUNDS")
        self.assertEqual(model.version, version)
        model.undo()
        self.assertEqual(model.offscreen(OFFSCREEN).name, "layer")

    def test_part_binding_and_offscreen_resize_atomically(self):
        model = session()
        with model.edit("base") as edit:
            edit.create_parameter(PARAMETER, "switch", 0, 1, 0)
            edit.create_part(PART, "owner")
            edit.create_scene_binding(
                SCENE_PART, "part", PART, [kasane.Axis(PARAMETER, [0, 1])], [
                    kasane.ScenePartKeyform([0], 0),
                    kasane.ScenePartKeyform([1], 10),
                ],
            )
            edit.create_offscreen(kasane.OffscreenSpec(
                OFFSCREEN, "layer", PART, part_keyform_indices=[0, 1],
                keyforms=[kasane.OffscreenKeyform(0.5), kasane.OffscreenKeyform(1)],
            ))
        binding = model.scene_binding(SCENE_PART)
        offscreen = model.offscreen(OFFSCREEN)
        resized_binding = binding._replace(
            axes=[kasane.Axis(PARAMETER, [0, 0.5, 1])],
            keyforms=[
                kasane.ScenePartKeyform([0], 0),
                kasane.ScenePartKeyform([0.5], 5),
                kasane.ScenePartKeyform([1], 10),
            ],
        )
        resized_offscreen = offscreen._replace(part_keyform_indices=[0, -1, 1])
        old_version = model.version
        with self.assertRaises(kasane.SdkFailure) as error:
            with model.edit("invalid independent resize") as edit:
                edit.replace_scene_binding(
                    resized_binding.id, resized_binding.kind, resized_binding.target_id,
                    resized_binding.axes, resized_binding.keyforms,
                )
        self.assertEqual(error.exception.code, "INVALID_LENGTH")
        self.assertEqual(model.version, old_version)
        with model.edit("joint resize") as edit:
            edit.replace_part_binding_with_offscreen(resized_binding, resized_offscreen)
        self.assertEqual(len(model.scene_binding(SCENE_PART).keyforms), 3)
        self.assertEqual(model.offscreen(OFFSCREEN).part_keyform_indices, [0, -1, 1])
        model.undo()
        self.assertEqual(len(model.scene_binding(SCENE_PART).keyforms), 2)
        self.assertEqual(model.offscreen(OFFSCREEN).part_keyform_indices, [0, 1])

    def test_glue_create_replace_binding_and_rollback(self):
        model = session()
        with model.edit("meshes") as edit:
            edit.add_png_asset(ASSET, "texture", TEXTURE)
            edit.create_rectangle(MESH, "a", ASSET, (0, 0), (1, 1))
            edit.create_rectangle(MESH_B, "b", ASSET, (0, 0), (1, 1))
            edit.create_parameter(PARAMETER, "intensity", 0, 1, 0)
            edit.create_glue(kasane.GlueSpec(
                GLUE, "seam", MESH, MESH_B,
                [kasane.GlueVertexPair(0, 0, 1, 1)],
            ))
        glue = model.glue(GLUE)
        self.assertEqual(glue.runtime_id, GLUE)
        self.assertEqual(glue.pairs[0].vertex_a, 0)
        glue.pairs.clear()
        self.assertEqual(len(model.glue(GLUE).pairs), 1)
        with model.edit("bind glue") as edit:
            edit.replace_glue(glue._replace(
                name="animated seam", intensity=0.5,
                pairs=[kasane.GlueVertexPair(0, 0, 0.75, 1)],
                binding=kasane.GlueBinding([kasane.Axis(PARAMETER, [0, 1])], [0, 1]),
            ))
        updated = model.glue(GLUE)
        self.assertEqual(updated.name, "animated seam")
        self.assertEqual(updated.runtime_id, GLUE)
        self.assertEqual(updated.binding.intensities, [0, 1])
        self.assertAlmostEqual(updated.pairs[0].weight_a, 0.75)
        with TemporaryDirectory() as directory:
            destination = Path(directory).resolve() / "project"
            model.save(destination)
            reopened = kasane.open_project(destination)
            self.assertEqual(reopened.glue(GLUE).binding.intensities, [0, 1])
        version = model.version
        with self.assertRaises(kasane.SdkFailure) as error:
            with model.edit("invalid glue") as edit:
                edit.replace_glue(updated._replace(pairs=[kasane.GlueVertexPair(99, 0, 1, 1)]))
        self.assertEqual(error.exception.code, "MISSING_VERTEX")
        self.assertEqual(model.version, version)
        model.undo()
        self.assertEqual(model.glue(GLUE).name, "seam")

    def test_blend_table_constraint_and_parameter_kind(self):
        model = session()
        with model.edit("parameters and blend metadata") as edit:
            edit.create_parameter(BLEND_PARAMETER, "shape", 0, 1, 0, kind="blend_shape")
            edit.create_parameter(PARAMETER, "limit", 0, 1, 0)
            edit.create_blend_key_table(kasane.BlendKeyTableSpec(
                BLEND_TABLE, BLEND_PARAMETER, [0, 1], 0,
            ))
            edit.create_blend_constraint(kasane.BlendConstraintSpec(
                BLEND_CONSTRAINT, PARAMETER, [0, 1], [1, 1],
            ))
        self.assertEqual(model.parameter(BLEND_PARAMETER).kind, "blend_shape")
        table = model.blend_key_table(BLEND_TABLE)
        constraint = model.blend_constraint(BLEND_CONSTRAINT)
        table.keys[0] = 0.25
        self.assertEqual(model.blend_key_table(BLEND_TABLE).keys, [0, 1])
        with model.edit("replace blend metadata") as edit:
            edit.replace_blend_key_table(table._replace(keys=[0, 0.5, 1], base_key_idx=1))
            edit.replace_blend_constraint(constraint._replace(weights=[0.5, 1]))
        self.assertEqual(model.blend_key_table(BLEND_TABLE).base_key_idx, 1)
        self.assertEqual(model.blend_constraint(BLEND_CONSTRAINT).weights, [0.5, 1])
        with TemporaryDirectory() as directory:
            destination = Path(directory).resolve() / "project"
            model.save(destination)
            reopened = kasane.open_project(destination)
            self.assertEqual(reopened.blend_key_table(BLEND_TABLE).keys, [0, 0.5, 1])
            self.assertEqual(reopened.parameter(BLEND_PARAMETER).kind, "blend_shape")
        version = model.version
        with self.assertRaises(kasane.SdkFailure) as error:
            with model.edit("invalid constraint") as edit:
                edit.replace_blend_constraint(constraint._replace(weights=[1]))
        self.assertEqual(error.exception.code, "INVALID_LENGTH")
        self.assertEqual(model.version, version)
        model.undo()
        self.assertEqual(model.blend_key_table(BLEND_TABLE).keys, [0, 1])

    def test_blend_bindings_cover_all_targets(self):
        model = session()
        zero = [(0, 0)] * 4
        warp_points = [(0, 0), (1, 0), (0, 1), (1, 1)]
        with model.edit("blend targets") as edit:
            edit.add_png_asset(ASSET, "texture", TEXTURE)
            edit.create_rectangle(MESH, "mesh", ASSET, (0, 0), (1, 1))
            edit.create_rectangle(MESH_B, "second", ASSET, (0, 0), (1, 1))
            edit.create_part(PART, "part")
            edit.create_rotation_transform(
                ROTATION, "rotation", kasane.RotationData(0, kasane.RotationPose((0, 0)))
            )
            edit.create_warp_transform(WARP, "warp", kasane.WarpData(1, 1, True, warp_points))
            edit.create_glue(kasane.GlueSpec(
                GLUE, "seam", MESH, MESH_B, [kasane.GlueVertexPair(0, 0, 1, 1)]
            ))
            edit.create_offscreen(kasane.OffscreenSpec(
                OFFSCREEN, "layer", PART, keyforms=[kasane.OffscreenKeyform(1)]
            ))
            edit.create_parameter(BLEND_PARAMETER, "shape", 0, 1, 0, kind="blend_shape")
            edit.create_parameter(PARAMETER, "limit", 0, 1, 0)
            edit.create_blend_key_table(kasane.BlendKeyTableSpec(
                BLEND_TABLE, BLEND_PARAMETER, [0, 1], 0,
            ))
            edit.create_blend_constraint(kasane.BlendConstraintSpec(
                BLEND_CONSTRAINT, PARAMETER, [0, 1], [1, 1],
            ))
            for spec in [
                kasane.BlendBindingSpec(BLEND_MESH, MESH, "mesh", BLEND_TABLE,
                    [BLEND_CONSTRAINT], [kasane.BlendMeshDelta(zero),
                    kasane.BlendMeshDelta([(1, 0)] * 4, 0.2, 2, (0.8, 1, 1), (0, 0.1, 0))]),
                kasane.BlendBindingSpec(BLEND_WARP, WARP, "warp", BLEND_TABLE, [],
                    [kasane.BlendWarpDelta(zero), kasane.BlendWarpDelta([(0.1, 0)] * 4, 0.3)]),
                kasane.BlendBindingSpec(BLEND_ROTATION, ROTATION, "rotation", BLEND_TABLE, [],
                    [kasane.BlendRotationDelta(), kasane.BlendRotationDelta((1, 2), 30, 1.5, 0.4)]),
                kasane.BlendBindingSpec(BLEND_PART, PART, "part", BLEND_TABLE, [],
                    [kasane.BlendPartDelta(0), kasane.BlendPartDelta(5)]),
                kasane.BlendBindingSpec(BLEND_GLUE, GLUE, "glue", BLEND_TABLE, [],
                    [kasane.BlendGlueDelta(0), kasane.BlendGlueDelta(1)]),
                kasane.BlendBindingSpec(BLEND_OFFSCREEN, OFFSCREEN, "offscreen", BLEND_TABLE, [],
                    [kasane.BlendOffscreenDelta(0), kasane.BlendOffscreenDelta(0.5, (1, 0.9, 1))]),
            ]:
                edit.create_blend_binding(spec)
        self.assertEqual(len(model.blend_binding_ids()), 6)
        self.assertEqual(model.blend_binding(BLEND_MESH).keyforms[1].draw_order, 2)
        self.assertAlmostEqual(model.blend_binding(BLEND_WARP).keyforms[1].points[0][0], 0.1)
        self.assertEqual(model.blend_binding(BLEND_ROTATION).keyforms[1].angle, 30)
        self.assertEqual(model.blend_binding(BLEND_PART).keyforms[1].draw_order, 5)
        self.assertEqual(model.blend_binding(BLEND_GLUE).keyforms[1].intensity, 1)
        self.assertAlmostEqual(model.blend_binding(BLEND_OFFSCREEN).keyforms[1].opacity, 0.5)
        binding = model.blend_binding(BLEND_MESH)
        binding.keyforms[1].positions[0] = (99, 99)
        self.assertEqual(model.blend_binding(BLEND_MESH).keyforms[1].positions[0], (1, 0))
        with model.edit("replace blend binding") as edit:
            edit.replace_blend_binding(binding._replace(keyforms=[
                kasane.BlendMeshDelta(zero), kasane.BlendMeshDelta([(2, 0)] * 4),
            ]))
        self.assertEqual(model.blend_binding(BLEND_MESH).keyforms[1].positions[0], (2, 0))
        with TemporaryDirectory() as directory:
            destination = Path(directory).resolve() / "project"
            model.save(destination)
            reopened = kasane.open_project(destination)
            self.assertEqual(reopened.blend_binding(BLEND_MESH).keyforms[1].positions[0], (2, 0))
        version = model.version
        with self.assertRaises(kasane.SdkFailure) as error:
            with model.edit("invalid blend binding") as edit:
                edit.replace_blend_binding(binding._replace(keyforms=[kasane.BlendMeshDelta(zero)]))
        self.assertEqual(error.exception.code, "INCOMPLETE_KEYFORMS")
        self.assertEqual(model.version, version)
        model.undo()
        self.assertEqual(model.blend_binding(BLEND_MESH).keyforms[1].positions[0], (1, 0))

    def test_custom_mesh_create_and_complete_replace(self):
        model = session()
        with model.edit("asset") as edit:
            edit.add_png_asset(ASSET, "texture", TEXTURE)
        geometry = kasane.MeshGeometryData(
            [10, 11, 12], [(40, 40), (60, 40), (40, 60)],
            [(0, 0), (1, 0), (0, 1)], [(10, 11, 12)],
        )
        with model.edit("custom mesh") as edit:
            edit.create_mesh(kasane.MeshRecordSpec(
                MESH, "triangle", geometry, kasane.MeshDrawingData(ASSET),
            ))
        original = model.mesh_record(MESH)
        self.assertEqual(original.runtime_id, MESH)
        self.assertEqual(original.geometry.triangles, [(10, 11, 12)])
        original.geometry.positions[0] = (999, 999)
        self.assertEqual(model.mesh_record(MESH).geometry.positions[0], (40, 40))
        with model.edit("replace mesh") as edit:
            edit.replace_mesh(original._replace(
                name="triangle updated",
                geometry=geometry._replace(positions=[(41, 40), (60, 40), (40, 60)]),
                drawing=original.drawing._replace(
                    appearance=kasane.Appearance(0.5), draw_order=3,
                ),
            ))
        updated = model.mesh_record(MESH)
        self.assertEqual(updated.runtime_id, MESH)
        self.assertEqual(updated.name, "triangle updated")
        self.assertEqual(updated.geometry.positions[0], (41, 40))
        self.assertAlmostEqual(updated.drawing.appearance.opacity, 0.5)
        with TemporaryDirectory() as directory:
            destination = Path(directory).resolve() / "project"
            model.save(destination)
            reopened = kasane.open_project(destination)
            self.assertEqual(reopened.mesh_record(MESH).geometry.triangles, [(10, 11, 12)])
        version = model.version
        with self.assertRaises(kasane.SdkFailure) as error:
            with model.edit("invalid triangle") as edit:
                edit.replace_mesh(updated._replace(
                    geometry=updated.geometry._replace(triangles=[(10, 11, 99)]),
                ))
        self.assertEqual(error.exception.code, "MISSING_VERTEX")
        self.assertEqual(model.version, version)
        model.undo()
        self.assertEqual(model.mesh_record(MESH).name, "triangle")

    def test_topology_replace_rewrites_all_dependencies_atomically(self):
        model = session()
        with model.edit("topology dependencies") as edit:
            edit.add_png_asset(ASSET, "texture", TEXTURE)
            edit.create_rectangle(MESH, "source", ASSET, (40, 40), (60, 60))
            edit.create_rectangle(MESH_B, "other", ASSET, (40, 40), (60, 60))
            edit.create_parameter(PARAMETER, "ordinary", 0, 1, 0)
            edit.create_parameter(BLEND_PARAMETER, "shape", 0, 1, 0, kind="blend_shape")
            edit.create_mesh_binding(BINDING, MESH, [kasane.Axis(PARAMETER, [0, 1])], [
                kasane.MeshKeyform([0], [(40, 40), (60, 40), (40, 60), (60, 60)]),
                kasane.MeshKeyform([1], [(41, 40), (61, 40), (41, 60), (61, 60)]),
            ])
            edit.create_blend_key_table(kasane.BlendKeyTableSpec(
                BLEND_TABLE, BLEND_PARAMETER, [0, 1], 0,
            ))
            edit.create_blend_binding(kasane.BlendBindingSpec(
                BLEND_MESH, MESH, "mesh", BLEND_TABLE, [], [
                    kasane.BlendMeshDelta([(0, 0)] * 4),
                    kasane.BlendMeshDelta([(1, 0)] * 4),
                ],
            ))
            edit.create_glue(kasane.GlueSpec(
                GLUE, "seam", MESH, MESH_B, [kasane.GlueVertexPair(0, 0, 1, 1)],
            ))
        source = model.geometry(MESH)
        record = model.mesh_record(MESH)
        mapping = {0: 10, 1: 11, 2: 12, 3: 13}
        new_geometry = record.geometry._replace(
            vertex_ids=[10, 11, 12, 13],
            triangles=[tuple(mapping[vertex] for vertex in triangle)
                       for triangle in record.geometry.triangles],
        )
        replacement = record._replace(geometry=new_geometry)
        binding = model.binding(BINDING)
        blend = model.blend_binding(BLEND_MESH)
        glue = model.glue(GLUE)._replace(
            pairs=[kasane.GlueVertexPair(10, 0, 1, 1)],
        )
        version = model.version
        with self.assertRaises(kasane.SdkFailure) as error:
            with model.edit("incomplete map") as edit:
                edit.replace_topology(source, replacement, {0: 10},
                    binding, [blend], [glue])
        self.assertEqual(error.exception.code, "INVALID_VERTEX_MAPPING")
        self.assertEqual(model.version, version)
        with model.edit("topology replacement") as edit:
            edit.replace_topology(source, replacement, mapping,
                binding, [blend], [glue])
        self.assertEqual(model.geometry(MESH).vertex_ids, [10, 11, 12, 13])
        self.assertEqual(model.glue(GLUE).pairs[0].vertex_a, 10)
        self.assertEqual(model.binding(BINDING).id, BINDING)
        self.assertEqual(model.blend_binding(BLEND_MESH).id, BLEND_MESH)
        with TemporaryDirectory() as directory:
            destination = Path(directory).resolve() / "project"
            model.save(destination)
            reopened = kasane.open_project(destination)
            self.assertEqual(reopened.geometry(MESH).vertex_ids, [10, 11, 12, 13])
        current = model.version
        with self.assertRaises(kasane.SdkFailure) as error:
            with model.edit("stale source") as edit:
                edit.replace_topology(source, replacement, mapping,
                    binding, [blend], [glue])
        self.assertEqual(error.exception.code, "STALE_TOPOLOGY")
        self.assertEqual(model.version, current)
        model.undo()
        self.assertEqual(model.geometry(MESH).vertex_ids, [0, 1, 2, 3])
        self.assertEqual(model.glue(GLUE).pairs[0].vertex_a, 0)

    def test_png_base_relocation_and_replacement(self):
        model = session()
        with model.edit("asset") as edit:
            edit.add_png_asset_from_base(ASSET, "texture", TEXTURE.parent, Path(TEXTURE.name))
        original = model.asset(ASSET)
        with TemporaryDirectory() as directory:
            relocated = Path(directory).resolve() / "same.png"
            shutil.copyfile(TEXTURE, relocated)
            with self.assertRaises(kasane.SdkFailure) as mismatch:
                with model.edit("bad relocation") as edit:
                    edit.relocate_png_asset(ASSET, EXTERNAL / "texture_00.png")
            self.assertEqual(mismatch.exception.code, "RESOURCE_DIMENSIONS")
            self.assertEqual(model.asset(ASSET), original)
            with model.edit("relocate") as edit:
                edit.relocate_png_asset(ASSET, relocated)
            self.assertEqual(model.asset(ASSET).sha256, original.sha256)
            self.assertEqual(model.asset(ASSET).source, str(relocated))
            with model.edit("replace") as edit:
                edit.replace_png_asset(ASSET, "new texture", EXTERNAL / "texture_00.png")
            self.assertEqual(model.asset(ASSET).width, 4)
            self.assertEqual(model.asset(ASSET).name, "new texture")
            model.undo()
            self.assertEqual(model.asset(ASSET).sha256, original.sha256)

    def test_geometry_diagnostics_are_advisory(self):
        model = session()
        with model.edit("rectangle") as edit:
            edit.add_png_asset(ASSET, "texture", TEXTURE)
            edit.create_rectangle(MESH, "face", ASSET, (40, 40), (60, 60))
        version = model.version
        issues = model.diagnose_geometry(201, ((0, 0), (50, 50)))
        self.assertEqual(
            sum(issue.kind == "small_triangle" for issue in issues), 2
        )
        self.assertTrue(
            any(issue.kind == "outside_canvas_bounds" for issue in issues)
        )
        self.assertEqual({issue.mesh_id for issue in issues}, {MESH})
        self.assertEqual(model.version, version)
        with self.assertRaises(kasane.SdkFailure) as invalid:
            model.diagnose_geometry(-1)
        self.assertEqual(invalid.exception.code, "INVALID_DIAGNOSTIC_OPTIONS")
        self.assertEqual(model.version, version)

    def test_scene_bindings_cover_all_tracks_and_keyform_updates(self):
        model = session()
        points = [(0, 0), (1, 0), (0, 1), (1, 1)]
        axis = kasane.Axis(PARAMETER, [0, 1])
        with model.edit("scene") as edit:
            edit.create_parameter(PARAMETER, "pose", 0, 1, 0)
            edit.create_part(PART, "part")
            edit.create_rotation_transform(
                ROTATION, "rotation", kasane.RotationData(0, kasane.RotationPose((0, 0)))
            )
            edit.create_warp_transform(
                WARP, "warp", kasane.WarpData(1, 1, True, points)
            )
            edit.create_scene_binding(
                SCENE_PART, "part", PART, [axis], [
                    kasane.ScenePartKeyform([0], 0),
                    kasane.ScenePartKeyform([1], 10),
                ]
            )
            edit.create_scene_binding(
                SCENE_ROTATION, "rotation", ROTATION, [axis], [
                    kasane.SceneRotationKeyform([0], kasane.RotationPose((0, 0))),
                    kasane.SceneRotationKeyform(
                        [1], kasane.RotationPose((0, 0), angle=30),
                        kasane.Appearance(opacity=0.75),
                    ),
                ]
            )
            edit.create_scene_binding(
                SCENE_WARP, "warp", WARP, [axis], [
                    kasane.SceneWarpKeyform([0], points),
                    kasane.SceneWarpKeyform([1], [(x + 1, y) for x, y in points]),
                ]
            )
        self.assertEqual(model.scene_binding_ids(), [SCENE_PART, SCENE_ROTATION, SCENE_WARP])
        self.assertEqual(model.scene_binding(SCENE_PART).keyforms[1].draw_order, 10)
        rotation = model.scene_binding(SCENE_ROTATION)
        self.assertEqual(rotation.keyforms[1].rotation.angle, 30)
        self.assertEqual(rotation.keyforms[1].appearance.opacity, 0.75)
        self.assertEqual(model.binding_for_scene(ROTATION), rotation)
        warp = model.scene_binding(SCENE_WARP)
        self.assertEqual(warp.keyforms[1].positions[0], (1, 0))
        warp.keyforms[1].positions[0] = (999, 999)
        self.assertEqual(model.scene_binding(SCENE_WARP).keyforms[1].positions[0], (1, 0))
        before = model.version
        with model.edit("update scene form") as edit:
            edit.set_scene_keyform(SCENE_PART, kasane.ScenePartKeyform([1], 20))
        self.assertEqual(model.version[2], before[2] + 1)
        self.assertEqual(model.scene_binding(SCENE_PART).keyforms[1].draw_order, 20)
        with model.edit("replace scene") as edit:
            edit.replace_scene_binding(
                SCENE_ROTATION, "rotation", ROTATION, [axis], [
                    kasane.SceneRotationKeyform([0], kasane.RotationPose((0, 0))),
                    kasane.SceneRotationKeyform([1], kasane.RotationPose((0, 0), angle=45)),
                ]
            )
        self.assertEqual(model.scene_binding(SCENE_ROTATION).keyforms[1].rotation.angle, 45)
        before = model.version
        with self.assertRaises(TypeError):
            with model.edit("wrong scene form") as edit:
                edit.set_scene_keyform(SCENE_PART, object())
        self.assertEqual(model.version, before)
        with TemporaryDirectory() as directory:
            destination = Path(directory).resolve() / "project"
            model.save(destination)
            reopened = kasane.open_project(destination)
            self.assertEqual(reopened.scene_binding(SCENE_ROTATION).keyforms[1].rotation.angle, 45)
            self.assertEqual(reopened.scene_binding(SCENE_WARP).keyforms[1].positions[0], (1, 0))

    def test_mesh_binding_preserves_appearance_and_replaces_forms(self):
        model = session()
        with model.edit("base") as edit:
            edit.add_png_asset(ASSET, "texture", TEXTURE)
            edit.create_rectangle(MESH, "face", ASSET, (40, 40), (60, 60))
        base = model.mesh(MESH).positions
        shifted = [(x + 10, y) for x, y in base]
        axis = kasane.Axis(PARAMETER, [0, 1])
        with model.edit("binding") as edit:
            edit.create_parameter(PARAMETER, "open", 0, 1, 0)
            edit.create_mesh_binding(BINDING, MESH, [axis], [
                kasane.MeshKeyform([0], base),
                kasane.MeshKeyform([1], shifted, kasane.Appearance(0.5), 3),
            ])
        binding = model.binding(BINDING)
        self.assertEqual(binding.keyforms[1].appearance.opacity, 0.5)
        self.assertEqual(binding.keyforms[1].draw_order, 3)
        with model.edit("change form") as edit:
            edit.set_mesh_keyform(BINDING, kasane.MeshKeyform(
                [1], shifted, kasane.Appearance(0.75), 5
            ))
        self.assertEqual(model.binding(BINDING).keyforms[1].appearance.opacity, 0.75)
        self.assertEqual(model.binding(BINDING).keyforms[1].draw_order, 5)
        with model.edit("replace binding") as edit:
            edit.replace_mesh_binding(BINDING, MESH, [axis], [
                kasane.MeshKeyform([0], base),
                kasane.MeshKeyform([1], shifted, kasane.Appearance(1), 7),
            ])
        self.assertEqual(model.binding(BINDING).keyforms[1].draw_order, 7)
        with TemporaryDirectory() as directory:
            destination = Path(directory).resolve() / "project"
            model.save(destination)
            reopened = kasane.open_project(destination)
            self.assertEqual(reopened.binding(BINDING).keyforms[1].draw_order, 7)
            self.assertEqual(reopened.evaluate({PARAMETER: 0.5}).drawables[0].positions[0], (-0.5, 1))

    def test_mesh_properties_update_preserves_geometry(self):
        model = session()
        with model.edit("base") as edit:
            edit.add_png_asset(ASSET, "texture", TEXTURE)
            edit.create_rectangle(MESH, "face", ASSET, (40, 40), (60, 60))
        geometry = model.geometry(MESH)
        original = model.mesh_properties(MESH)
        self.assertEqual(original.texture_asset_id, ASSET)
        self.assertEqual(original.blend_mode, "normal")
        updated = kasane.MeshProperties(
            ASSET, kasane.Appearance(0.6, (0.8, 1, 1), (0, 0.1, 0)),
            5, "additive", True, False, True, [],
        )
        with model.edit("draw properties") as edit:
            edit.update_mesh_properties(MESH, updated)
        snapshot = model.mesh_properties(MESH)
        self.assertAlmostEqual(snapshot.appearance.opacity, updated.appearance.opacity)
        for actual, expected in zip(snapshot.appearance.multiply, updated.appearance.multiply):
            self.assertAlmostEqual(actual, expected)
        for actual, expected in zip(snapshot.appearance.screen, updated.appearance.screen):
            self.assertAlmostEqual(actual, expected)
        self.assertEqual(snapshot.draw_order, 5)
        self.assertEqual(snapshot.blend_mode, "additive")
        self.assertFalse(snapshot.double_sided)
        self.assertTrue(snapshot.inverted_mask)
        self.assertEqual(model.geometry(MESH).vertex_ids, geometry.vertex_ids)
        self.assertEqual(model.geometry(MESH).positions, geometry.positions)
        before = model.version
        with self.assertRaises(ValueError):
            with model.edit("invalid blend") as edit:
                edit.rename_mesh(MESH, "should roll back")
                edit.update_mesh_properties(MESH, updated._replace(blend_mode="unknown"))
        self.assertEqual(model.version, before)
        self.assertEqual(model.mesh(MESH).name, "face")
        model.undo()
        self.assertEqual(model.mesh_properties(MESH).appearance, original.appearance)

    def test_runner_reports_exception_line_and_committed_edit(self):
        with TemporaryDirectory() as directory:
            root = Path(directory).resolve()
            script = root / "script.py"
            script.write_text(
                "import kasane\n"
                f"session = kasane.Session('{DOCUMENT}', 100, 100, (50, 50), 10)\n"
                "with session.edit('asset') as edit:\n"
                f"    edit.add_png_asset('{ASSET}', 'texture', {str(TEXTURE)!r})\n"
                "raise ValueError('script failed')\n",
                encoding="utf-8",
            )
            report = root / "report.json"
            environment = os.environ.copy()
            environment["PYTHONPATH"] = ""
            process = subprocess.run(
                [sys.executable, "-m", "kasane", "run", str(script), "--report", str(report)],
                cwd=root,
                env=environment,
                text=True,
                capture_output=True,
                timeout=10,
            )
            self.assertEqual(process.returncode, 1)
            data = json.loads(report.read_text(encoding="utf-8"))
            self.assertEqual(data["status"], "failed")
            self.assertEqual(data["exception"]["line"], 5)
            self.assertEqual(data["exception"]["type"], "ValueError")
            self.assertEqual(data["sessions"][0]["version"][2], 1)
            self.assertTrue(data["sessions"][0]["modified"])


if __name__ == "__main__":
    unittest.main()
