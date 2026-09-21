"""Acceptance bookkeeping must not turn a partial/stale run into S6 success."""
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

import validate_s6 as s6


class GateTests(unittest.TestCase):
    def test_every_required_gate_is_mandatory(self):
        complete = {'gates': {name: {'status': 'passed'} for name in s6.REQUIRED_GATES}}
        self.assertTrue(s6.all_gates_passed(complete))
        self.assertFalse(s6.all_gates_passed({}))
        for name in s6.REQUIRED_GATES:
            with self.subTest(gate=name):
                for status in ('not_run', 'failed', 'skipped'):
                    complete['gates'][name]['status'] = status
                    self.assertFalse(s6.all_gates_passed(complete))
                complete['gates'][name]['status'] = 'passed'
                value = complete['gates'].pop(name)
                self.assertFalse(s6.all_gates_passed(complete))
                complete['gates'][name] = value

    def test_failed_setup_invalidates_previous_success(self):
        import json
        with tempfile.TemporaryDirectory() as temporary:
            output = Path(temporary)
            (output/'report.json').write_text('{"status":"passed"}')
            with patch.object(s6, 'source_manifest', side_effect=RuntimeError('missing SDK')):
                result = s6.acceptance(output, s6.SDK, s6.GODOT)
            stored = json.loads((output/'report.json').read_text())
            self.assertEqual(result['status'], 'failed')
            self.assertEqual(stored['status'], 'failed')
            self.assertEqual(set(stored['gates']), set(s6.REQUIRED_GATES))
            self.assertFalse(s6.all_gates_passed(stored))

    def test_local_failure_cannot_hide_in_whole_image_average(self):
        import numpy as np
        from PIL import Image
        with tempfile.TemporaryDirectory() as temporary:
            output = Path(temporary)
            a = np.zeros((300, 300, 4), dtype=np.uint8)
            a[100:130, 100:130] = 255
            b = a.copy()
            b[100:110, 100:130, 0] = 0
            Image.fromarray(a).save(output/'a.png')
            Image.fromarray(b).save(output/'b.png')
            result = s6.compare_images(output/'a.png', output/'b.png', output/'diff.png')
            self.assertEqual(result['checks'][0]['status'], 'passed')
            self.assertEqual(result['status'], 'failed')

    def test_matrix_exercises_both_source_representations(self):
        with tempfile.TemporaryDirectory() as temporary:
            samples = s6.matrix_cases(Path(temporary))
        self.assertEqual(len(samples), 24)
        self.assertEqual({s['premultiplied'] for s in samples}, {0, 1})
        self.assertEqual({s['source'][3] for s in samples} >= {0, 1}, True)
        self.assertEqual({s['destination'][3] for s in samples} >= {0, 1}, True)
        self.assertEqual({s['inverted'] for s in samples}, {True, False})

    def test_nested_surface_rois_follow_geometry_and_visibility(self):
        frame = {'canvas': {'origin': [50,50], 'pixels_per_unit': 100},
                 'offscreens': [{'id': id, 'runtime_id': id, 'enabled': True, 'opacity': 1} for id in ('root','pupil')],
                 'drawables': [{'id': 'mesh', 'visible': True, 'opacity': 1, 'indices': [0,1,2],
                                'positions': [[-.4,-.4],[.4,.4],[.4,-.4]]}],
                 'render_plan': [{'command': command, 'id': id} for command,id in (
                     ('begin_offscreen','root'), ('begin_offscreen','pupil'), ('draw_mesh','mesh'),
                     ('end_offscreen','pupil'), ('end_offscreen','root'))]}
        observation = {'image_size': [100,100], 'camera': {'offset': [-25,-25], 'zoom': .5}}
        self.assertEqual(s6.offscreen_regions(frame, observation), [('root',(28,28,72,72)),('pupil',(28,28,72,72))])
        frame['offscreens'][1]['enabled'] = False
        self.assertEqual(s6.offscreen_regions(frame, observation), [])


if __name__ == '__main__':
    unittest.main()
