"""Harness controls. Set SDK_EXPERIMENT_WHEEL for installed-wheel integration tests."""
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
import zipfile

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "tools"))
import sdk_experiments as harness


SOLUTION = '''from pathlib import Path
from uuid import uuid4
import argparse, json
import kasane
p = argparse.ArgumentParser()
p.add_argument('--output', type=Path, required=True)
out = p.parse_args().output
out.mkdir(parents=True, exist_ok=True)
packet = Path(__file__).resolve().parent
task = TASK
if task == 'edit':
    model = kasane.open_project(packet / 'input/project')
    mesh = model.require_unique_mesh('right')
    binding = model.binding_for_mesh(mesh.id)
    form = next(f for f in binding.keyforms if f.keys == [1])
    with model.edit('shift') as edit:
        edit.set_mesh_keyform(binding.id, form._replace(positions=[(x+10,y) for x,y in form.positions]))
else:
    model = kasane.Session(str(uuid4()), 100, 100, (50,50), 10)
    asset, mesh, parameter = (str(uuid4()) for _ in range(3))
    with model.edit('create') as edit:
        edit.add_png_asset(asset, 'freely chosen asset name', packet / 'input/texture.png')
        edit.create_rectangle(mesh, 'face', asset, (40,40), (60,60))
    if task == 'parameter':
        base = model.mesh(mesh).positions
        with model.edit('parameter') as edit:
            edit.create_parameter(parameter, 'Open', 0, 1, 0)
            edit.create_mesh_binding(str(uuid4()), mesh, [kasane.Axis(parameter,[0,1])],
                [kasane.MeshKeyform([0],base),kasane.MeshKeyform([1],[(x+10,y) for x,y in base])])
manifest = model.save(out / 'my-custom-layout' / 'project').manifest
(out / 'result.json').write_text(json.dumps({'project_manifest':str(manifest)}))
'''


class InfrastructureTests(unittest.TestCase):
    def test_timeout_retains_partial_output(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            report = harness.command([sys.executable, '-u', '-c',
                "import time; print('started'); time.sleep(30)"], root, root / 'log', 0.2)
            self.assertEqual(report['status'], 'timeout')
            self.assertIn('started', (root / 'log/stdout.log').read_text())
            self.assertTrue((root / 'log/command.json').exists())

    def test_launch_failure_is_not_a_subject_failure(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            report = harness.command(['/nonexistent/sdk-experiment-command'], root, root / 'log', 1)
            self.assertEqual(report['status'], 'launch_error')
            self.assertIsNone(report['returncode'])

    def test_evidence_cannot_be_overwritten(self):
        with tempfile.TemporaryDirectory() as folder:
            path = Path(folder) / 'report.json'
            harness.write(path, {'status': 'failed'})
            with self.assertRaises(FileExistsError):
                harness.write(path, {'status': 'passed'})
            self.assertEqual(harness.read(path)['status'], 'failed')

    def test_symlink_is_rejected(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            (root / 'link').symlink_to('/tmp')
            with self.assertRaises(ValueError):
                harness.hashes(root)


@unittest.skipUnless(os.environ.get('SDK_EXPERIMENT_WHEEL'), 'set SDK_EXPERIMENT_WHEEL for wheel tests')
class WheelIntegrationTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.temp = tempfile.TemporaryDirectory(prefix='kasane-harness-tests-')
        cls.root = Path(cls.temp.name).resolve() / 'experiment'
        cls.cli('init', '--wheel', os.environ['SDK_EXPERIMENT_WHEEL'],
                '--python', os.environ.get('SDK_EXPERIMENT_PYTHON', '3.14'))
        cls.python = str(cls.root / '.venv/bin/python')

    @classmethod
    def tearDownClass(cls):
        cls.temp.cleanup()

    @classmethod
    def cli(cls, action, *args, expected=0):
        result = subprocess.run([sys.executable, str(ROOT / 'tools/sdk_experiments.py'),
            action, '--experiment', str(cls.root), *args], capture_output=True, text=True)
        if result.returncode != expected:
            raise AssertionError(f'{result.args}\n{result.stdout}\n{result.stderr}')
        return result.stdout

    def prepare(self, trial, task='create'):
        self.cli('prepare', '--trial', trial, '--task', task, '--model', 'test-fixture-not-agent')
        packet = self.root / 'packets' / trial
        (packet / 'solution.py').write_text(SOLUTION.replace('TASK', repr(task)))
        (packet / 'notes.md').write_text('Deterministic harness control, not a subject experiment.')
        return packet

    def run_solution(self, trial, packet):
        self.cli('run', '--trial', trial, '--', self.python, str(packet / 'solution.py'),
                 '--output', str(packet / 'output'))

    def test_all_tasks_controls_replay_and_unknown_review(self):
        for task in ('create', 'parameter', 'edit'):
            with self.subTest(task=task):
                trial = 'positive-' + task
                packet = self.prepare(trial, task)
                self.run_solution(trial, packet)
                self.cli('assess', '--trial', trial)
                rows = json.loads(self.cli('summarize'))['trials']
                row = next(r for r in rows if r['trial'] == trial)
                self.assertEqual(row['artifact_status'], 'passed')
                self.assertIsNone(row['independent_success'])
                self.assertTrue(row['measured_run_matches'])

    def test_input_tamper_cannot_pass_and_assessments_are_preserved(self):
        packet = self.prepare('tamper')
        self.run_solution('tamper', packet)
        self.cli('assess', '--trial', 'tamper')
        (packet / 'input/extra.txt').write_text('changed')
        self.cli('assess', '--trial', 'tamper', expected=1)
        self.assertEqual(len(list((self.root / 'host/trials/tamper/assessments').glob('*/report.json'))), 2)
        row = next(r for r in json.loads(self.cli('summarize'))['trials'] if r['trial'] == 'tamper')
        self.assertEqual(row['artifact_status'], 'failed')

    def test_missing_and_outside_manifest_rejected(self):
        packet = self.prepare('bad-manifest')
        self.cli('assess', '--trial', 'bad-manifest', expected=1)
        (packet / 'output').mkdir()
        (packet / 'output/result.json').write_text(json.dumps({'project_manifest': '/etc/hosts'}))
        self.cli('assess', '--trial', 'bad-manifest', expected=1)

    def test_changed_script_does_not_inherit_audited_success(self):
        packet = self.prepare('reviewed')
        self.run_solution('reviewed', packet)
        self.cli('assess', '--trial', 'reviewed')
        trace = self.root / 'host/trials/reviewed/agent/stdout.log'
        self.cli('review', '--trial', 'reviewed', '--reviewer', 'test', '--human-prompts', '0',
                 '--public-api', 'yes', '--trace', str(trace), '--notes', 'synthetic harness test')
        row = next(r for r in json.loads(self.cli('summarize'))['trials'] if r['trial'] == 'reviewed')
        self.assertTrue(row['independent_success'])
        with (packet / 'solution.py').open('a') as stream:
            stream.write('\n# changed after review\n')
        row = next(r for r in json.loads(self.cli('summarize'))['trials'] if r['trial'] == 'reviewed')
        self.assertIsNone(row['independent_success'])

    def test_bad_geometry_is_rejected_even_when_script_replays(self):
        packet = self.prepare('wrong-geometry')
        script = (packet / 'solution.py').read_text().replace('(60,60))', '(61,60))')
        (packet / 'solution.py').write_text(script)
        self.run_solution('wrong-geometry', packet)
        self.cli('assess', '--trial', 'wrong-geometry', expected=1)

    def test_changed_frozen_document_blocks_trial(self):
        document = self.root / 'host/frozen/API.md'
        original = document.read_bytes()
        try:
            document.write_bytes(original + b'\nchanged\n')
            self.cli('prepare', '--trial', 'changed-doc', '--task', 'create', '--model', 'test', expected=2)
        finally:
            document.write_bytes(original)

    def test_edit_cannot_pass_by_saving_unchanged_input(self):
        packet = self.prepare('no-edit', 'edit')
        (packet / 'solution.py').write_text('''from pathlib import Path
import argparse, json, kasane
p = argparse.ArgumentParser(); p.add_argument('--output', type=Path)
out = p.parse_args().output; out.mkdir(parents=True)
model = kasane.open_project(Path(__file__).resolve().parent / 'input/project')
saved = model.save(out / 'project')
(out / 'result.json').write_text(json.dumps({'project_manifest':str(saved.manifest)}))
''')
        self.run_solution('no-edit', packet)
        self.cli('assess', '--trial', 'no-edit', expected=1)

    def test_alternative_vertex_ids_order_and_diagonal_are_accepted(self):
        packet = self.prepare('alternate-geometry')
        script = (packet / 'solution.py').read_text()
        alternative = '''record = model.mesh_record(mesh)
geometry = kasane.MeshGeometryData(
    [11,22,33,44], [(60,60),(40,60),(40,40),(60,40)],
    [(1,1),(0,1),(0,0),(1,0)], [(11,22,44),(22,33,44)])
with model.edit('alternative triangulation') as edit:
    edit.replace_mesh(record._replace(geometry=geometry))
'''
        (packet / 'solution.py').write_text(script.replace('manifest = model.save', alternative + 'manifest = model.save'))
        self.run_solution('alternate-geometry', packet)
        self.cli('assess', '--trial', 'alternate-geometry')

    def test_subjects_share_uv_environment_and_can_install_dependencies(self):
        first = self.prepare('deps-first')
        second = self.prepare('deps-second')
        wheel = first / 'kasane_experiment_helper-1.0.0-py3-none-any.whl'
        metadata = 'kasane_experiment_helper-1.0.0.dist-info'
        files = {
            'kasane_experiment_helper.py': 'VALUE = 7\n',
            metadata + '/METADATA': 'Metadata-Version: 2.1\nName: kasane-experiment-helper\nVersion: 1.0.0\n',
            metadata + '/WHEEL': 'Wheel-Version: 1.0\nGenerator: test\nRoot-Is-Purelib: true\nTag: py3-none-any\n',
        }
        files[metadata + '/RECORD'] = ''.join(name + ',,\n' for name in files) + metadata + '/RECORD,,\n'
        with zipfile.ZipFile(wheel, 'w') as archive:
            for name, content in files.items():
                archive.writestr(name, content)
        for packet in (first, second):
            script = packet / 'solution.py'
            script.write_text('from kasane_experiment_helper import VALUE\nassert VALUE == 7\n' + script.read_text())
        runner = first / 'runner.py'
        runner.write_text('''import os, subprocess
from pathlib import Path
packet = Path(os.environ['KASANE_PACKET'])
python = os.environ['KASANE_PYTHON']
subprocess.check_call([os.environ['KASANE_UV'], 'pip', 'install', '--python', python,
                      str(packet / 'kasane_experiment_helper-1.0.0-py3-none-any.whl')])
subprocess.check_call([python, str(packet / 'solution.py'), '--output', str(packet / 'output')])
''')
        try:
            self.cli('run', '--trial', 'deps-first', '--', self.python, str(runner))
            self.cli('assess', '--trial', 'deps-first')
            self.run_solution('deps-second', second)
            self.cli('assess', '--trial', 'deps-second')
            self.assertFalse((first / '.venv').exists())
            self.assertFalse((second / '.venv').exists())
            before = harness.read(self.root / 'host/trials/deps-first/environment-before.json')
            after = harness.read(self.root / 'host/trials/deps-first/agent/environment.json')
            self.assertFalse(any('kasane-experiment-helper' in p for p in before['requirements']))
            self.assertTrue(any('kasane-experiment-helper' in p for p in after['requirements']))
            self.assertTrue(after['sdk_unchanged'])
            self.cli('review', '--trial', 'deps-first', '--reviewer', 'test', '--human-prompts', '0',
                     '--public-api', 'yes', '--trace', str(self.root / 'host/trials/deps-first/agent/stdout.log'),
                     '--notes', 'synthetic shared environment control')
        finally:
            subprocess.check_call(['uv', 'pip', 'uninstall', '--python', self.python, 'kasane-experiment-helper'])
        rows = json.loads(self.cli('summarize'))['trials']
        self.assertTrue(next(r for r in rows if r['trial'] == 'deps-first')['independent_success'])


if __name__ == '__main__':
    unittest.main()
