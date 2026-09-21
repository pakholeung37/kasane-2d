"""The numerical gate must reject out-of-contract results."""
import copy
import importlib.util
from pathlib import Path
import unittest
import tempfile
import json
import sys
sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "tools"))

spec = importlib.util.spec_from_file_location(
    "validate_m3b", Path(__file__).resolve().parents[1] / "tools/validate_m3b.py"
)
validation = importlib.util.module_from_spec(spec)
spec.loader.exec_module(validation)


class ConformanceThresholdTests(unittest.TestCase):
    def sample(self):
        return [[{
            "runtime_id": "mesh", "texture_slot": 0, "double_sided": True,
            "inverted_mask": False, "blend_mode": 0, "indices": [0, 1, 2],
            "draw_order": 0, "render_order": 0, "visible": True,
            "mask_indices": [], "opacity": 1.0,
            "multiply_color": [1.0, 1.0, 1.0, 1.0],
            "screen_color": [0.0, 0.0, 0.0, 1.0],
            "uvs": [[0.0, 0.0]], "positions": [[0.0, 0.0]],
        }]]

    def test_pixel_limit_is_additional_to_float_tolerance(self):
        expected = self.sample()
        actual = copy.deepcopy(expected)
        actual[0][0]["positions"][0][0] = 9e-6
        with self.assertRaisesRegex(RuntimeError, "pixel_error"):
            validation.compare_samples(expected, actual, 5800, "stress")

    def test_small_color_values_do_not_get_flat_2e_minus_5_tolerance(self):
        expected = self.sample()
        actual = copy.deepcopy(expected)
        actual[0][0]["screen_color"][0] = 1.9e-5
        with self.assertRaisesRegex(RuntimeError, "screen_color"):
            validation.compare_samples(expected, actual, 1, "color")

    def test_duplicate_ids_fail(self):
        expected = self.sample()
        actual = [expected[0] * 2]
        with self.assertRaisesRegex(RuntimeError, "duplicated"):
            validation.compare_samples(expected, actual, 1, "ids")

    def test_vectorized_and_scalar_metrics_agree(self):
        expected = self.sample() * 2
        actual = copy.deepcopy(expected)
        actual[1][0]["positions"][0][0] = 1e-6
        self.assertEqual(validation.compare_samples(expected, actual, 5800, "fast"),
                         validation.compare_samples_scalar(expected, actual, 5800, "scalar"))

    def test_nan_cannot_pass_comparison(self):
        expected = self.sample()
        actual = copy.deepcopy(expected)
        actual[0][0]["opacity"] = float("nan")
        with self.assertRaisesRegex(RuntimeError, "non-finite"):
            validation.compare_samples(expected, actual, 1, "nan")

    def test_equal_samples_pass(self):
        self.assertEqual(validation.compare_samples(self.sample(), self.sample(), 5800, "same")["max_pixel_error"], 0)


class AcceptanceEvidenceTests(unittest.TestCase):
    def test_missing_gate_and_wrong_model_cannot_complete_milestone(self):
        spec = importlib.util.spec_from_file_location("m3b_acceptance", Path(__file__).resolve().parents[1] / "tools/m3b_acceptance.py")
        evidence = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(evidence)
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            numeric = {"numerical_status":"passed", "cases":[{"inputs":{"orig":{"sha256":"model"}}}]}
            self.assertEqual(evidence.collect(copy.deepcopy(numeric), root/'baseline', root/'editor', root/'gpu')['status'], 'failed')
            (root/'baseline').write_text(json.dumps({'status':'passed', 'model':{'moc3_sha256':'model'}}))
            (root/'gpu').write_text(json.dumps({'status':'passed'}))
            editor = {'status':'passed', 'source_sha256':'wrong', 'gates':{name:{'status':'passed'} for name in ['gpu_comparison','packaged_editor_workflow','detached_texture_project','new_feature_edit_roundtrips']}}
            (root/'editor').write_text(json.dumps(editor))
            self.assertEqual(evidence.collect(copy.deepcopy(numeric), root/'baseline', root/'editor', root/'gpu')['status'], 'failed')
            editor['source_sha256'] = 'model'
            (root/'editor').write_text(json.dumps(editor))
            self.assertEqual(evidence.collect(copy.deepcopy(numeric), root/'baseline', root/'editor', root/'gpu')['status'], 'passed')


if __name__ == "__main__":
    unittest.main()
