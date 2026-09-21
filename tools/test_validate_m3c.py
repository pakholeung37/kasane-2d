"""M3C stage acceptance must fail closed on missing evidence."""
import json
import tempfile
import unittest
from contextlib import ExitStack
from pathlib import Path
from unittest.mock import patch

import validate_m3c as m3c
import validate_s6 as s6


class GateTests(unittest.TestCase):
    def test_missing_real_asset_is_not_stage_success(self):
        report = {"status": "passed", "gate": {
            "constructed_fixture": True, "real_asset": "not_run", "passed": True}}
        m3c.finalize_required_gates(report)
        self.assertEqual(report["status"], "not_run")
        self.assertFalse(report["gate"]["passed"])
        report["gate"]["real_asset"] = "passed"
        self.assertEqual(m3c.finalize_required_gates(report)["status"], "passed")

    def test_all_retains_earlier_failures_even_with_passed_s7_report(self):
        for failing_stage in range(6):
            with self.subTest(stage=failing_stage), tempfile.TemporaryDirectory() as temp, ExitStack() as stack:
                (Path(temp) / "s7_report.json").write_text(json.dumps({"status": "passed"}))
                stack.enter_context(patch.object(m3c, 'execute_stage', side_effect=lambda stage, builder, output, *args: builder(*args)))
                stack.enter_context(patch.object(m3c.evidence, 'reusable', return_value=True))
                stack.enter_context(patch('sys.argv', ['validate_m3c.py', '--stage', 'all', '--output', temp]))
                names = ['build_s0_baseline_report'] + [f'build_s{i}_report' for i in range(1, 6)]
                for i, name in enumerate(names):
                    stack.enter_context(patch.object(m3c, name, return_value={"gate": {"passed": i != failing_stage}}))
                stack.enter_context(patch.object(s6, 'acceptance', return_value={"status": "passed"}))
                self.assertEqual(m3c.main(), 1)


class EvidenceTests(unittest.TestCase):
    def test_no_evidence_or_modified_evidence_cannot_pass(self):
        import acceptance_evidence as e
        empty = {'checks': [e.check('claim', 'tests', 'passed')]}
        self.assertEqual(e.finalize(empty)['status'], 'not_run')
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / 'log'; path.write_text('original')
            report = {'checks': [e.check('run', 'tests', 'passed', [e.artifact(path, 'log')])]}
            self.assertEqual(e.finalize(report)['status'], 'passed')
            path.write_text('different')
            self.assertEqual(e.finalize(report)['status'], 'failed')

    def test_failed_stage_invalidates_stale_success(self):
        with tempfile.TemporaryDirectory() as tmp:
            output = Path(tmp); path = output / 's2_report.json'
            path.write_text('{"status":"passed"}')
            with patch.object(m3c.evidence, 'source_identity', return_value={'source_sha256':'test'}):
                result = m3c.execute_stage('S2', lambda: (_ for _ in ()).throw(RuntimeError('missing SDK')), output)
            self.assertEqual(result['status'], 'failed')
            self.assertEqual(json.loads(path.read_text())['status'], 'failed')

    def test_stale_source_report_is_not_reusable(self):
        import acceptance_evidence as e
        with patch.object(e, 'source_identity', return_value={'source_sha256':'new'}):
            self.assertFalse(e.reusable({'status':'passed','provenance':{'source_sha256':'old'}}, Path('.')))

    def test_scene_and_build_config_changes_invalidate_source_identity(self):
        import acceptance_evidence as e
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / 'apps/editor').mkdir(parents=True)
            (root / 'tools/probes').mkdir(parents=True)
            files = [root / 'apps/editor/main.tscn', root / 'tools/probes/CMakeLists.txt']
            for path in files: path.write_text('before')
            names = b'\0'.join(str(path.relative_to(root)).encode() for path in files)
            def git_output(command, **kwargs):
                if command[1] == 'ls-files': return names
                if command[1] == 'submodule': return b''
                return 'revision'
            with patch.object(e.subprocess, 'check_output', side_effect=git_output):
                baseline = e.source_identity(root)
                for path in files:
                    path.write_text('changed')
                    self.assertNotEqual(e.source_identity(root), baseline)
                    path.write_text('before')
                    self.assertEqual(e.source_identity(root), baseline)

    def test_zero_tests_are_not_acceptance(self):
        with tempfile.TemporaryDirectory() as tmp, patch.object(m3c, 'run_cmd', return_value='test result: ok. 0 passed; 0 failed;'):
            with self.assertRaises(RuntimeError):
                m3c.test_evidence(['cargo','test'], Path(tmp), 'empty')

if __name__ == "__main__":
    unittest.main()
