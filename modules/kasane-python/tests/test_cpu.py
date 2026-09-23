"""Run against an installed wheel from outside the source tree."""

from concurrent.futures import ThreadPoolExecutor
import json
import os
from pathlib import Path
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
        self.assertEqual(model.parameter(PARAMETER).name, "open")
        middle = model.evaluate({PARAMETER: 0.5})
        self.assertEqual(middle.parameters[0].value, 0.5)
        self.assertEqual(middle.drawables[0].positions[0], (-0.5, 1))
        self.assertEqual(model.evaluate({PARAMETER: 2}).parameters[0].value, 1)
        with TemporaryDirectory() as directory:
            destination = Path(directory).resolve() / "project"
            model.save(destination)
            reopened = kasane.open_project(destination)
            self.assertEqual(reopened.evaluate({PARAMETER: 0.5}), middle)

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
