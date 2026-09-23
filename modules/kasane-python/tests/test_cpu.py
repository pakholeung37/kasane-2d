"""Run against an installed wheel from outside the source tree."""

from concurrent.futures import ThreadPoolExecutor
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
TEXTURE = Path(__file__).resolve().parents[3] / "examples/sdk/asymmetric-2x2.png"
EXTERNAL = Path(__file__).resolve().parents[3] / "tests/fixtures/external_v50"


def session():
    return kasane.Session(DOCUMENT, 100, 100, (50, 50), 10)


class CpuWheelTests(unittest.TestCase):
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

    def test_two_threads_receive_one_stale_version(self):
        model = session()
        with model.edit("create") as edit:
            edit.add_png_asset(ASSET, "texture", TEXTURE)
            edit.create_rectangle(MESH, "face", ASSET, (40, 40), (60, 60))
        edits = [model.edit("rename") for _ in range(2)]

        def publish(index):
            try:
                with edits[index] as edit:
                    edit.rename_mesh(MESH, f"face-{index}")
                return "ok"
            except kasane.SdkFailure as failure:
                return failure.code

        with ThreadPoolExecutor(max_workers=2) as executor:
            futures = [executor.submit(publish, index) for index in range(2)]
            results = [future.result(timeout=10) for future in futures]
        self.assertEqual(sorted(results), ["STALE_VERSION", "ok"])

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
