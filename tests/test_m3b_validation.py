"""The numerical gate must reject out-of-contract results."""
import copy
import importlib.util
from pathlib import Path
import unittest

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

    def test_equal_samples_pass(self):
        self.assertEqual(validation.compare_samples(self.sample(), self.sample(), 5800, "same")["max_pixel_error"], 0)


if __name__ == "__main__":
    unittest.main()
