#!/usr/bin/env python3
"""Synthetic parser/unit tests. These are not physical tablet results."""
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

spec = importlib.util.spec_from_file_location('analysis', Path(__file__).with_name('analyze-input-check.py'))
analysis = importlib.util.module_from_spec(spec)
spec.loader.exec_module(analysis)


class InputAnalysisTests(unittest.TestCase):
    def test_camera_units_and_rejection(self):
        with tempfile.TemporaryDirectory() as root:
            path = Path(root) / 'synthetic.csv'
            path.write_text('contact_frame,ink_frame\n10,12\n30,34\n')
            result = analysis.analyze_camera(path, 240)
            self.assertAlmostEqual(result['latency']['p50_ms'], 12.5)
            for fps in [0, -1, float('nan'), float('inf')]:
                with self.assertRaises(ValueError):
                    analysis.analyze_camera(path, fps)
            path.write_text('contact_frame,ink_frame\n4,3\n')
            with self.assertRaises(ValueError):
                analysis.analyze_camera(path, 240)

    def test_frames_wrap_contact_and_report(self):
        with tempfile.TemporaryDirectory() as root:
            path = Path(root) / 'synthetic.jsonl'
            events = [dict(type='tablet', phase=phase, timestamp=time, pressure=pressure)
                      for phase, time, pressure in [('Hover', 0, 1), ('Down', 4294967290, .2),
                          ('Move', 4294967292, .8), ('Move', 3, .6), ('Up', 5, 0)]]
            path.write_text('\n'.join(map(json.dumps, events)))
            report = analysis.analyze_recording(path)
            self.assertEqual(report['contact_pressure_range'], [0, .8])
            self.assertEqual(report['contact_move_intervals']['p50_ms'], 7)
            self.assertIsNone(report['dispatch_to_cpu_paint'])
            path.write_text('{"type":"folio_input_check","schema_version":1,"tablet":{}}')
            self.assertEqual(analysis.analyze_recording(path)['tablet'], {})
            path.write_text('{"type":"tablet","phase":"Down","pressure":0.2}')
            with self.assertRaises(ValueError):
                analysis.analyze_recording(path)


if __name__ == '__main__':
    unittest.main()
