"""Exercise bounded computation, malformed requests and recovery in the real worker.

Uses the existing local math runtime; never installs packages or changes its pack.
"""
import json
import os
from pathlib import Path
import selectors
import subprocess
import tempfile
import unittest


class WorkerProtocolTests(unittest.TestCase):
    def test_invalid_requests_and_timeout_do_not_poison_the_next_problem(self):
        root = Path(__file__).resolve().parents[1]
        pack_path = Path(os.environ.get('FOLIO_MATH_CONFIG', root / 'artifacts/math-solver/pack.json'))
        pack = json.loads(pack_path.read_text())
        absolute = lambda path: str(Path(path) if Path(path).is_absolute() else pack_path.parent / path)
        with tempfile.TemporaryDirectory(prefix='folio-math-protocol-') as directory:
            config = Path(directory) / 'pack.json'
            config.write_text(json.dumps(dict(pack, timeout_seconds=1)))
            worker = subprocess.Popen([absolute(pack['python']), '-u', absolute(pack['worker']),
                                       '--config', str(config)], stdin=subprocess.PIPE,
                                      stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
            selector = selectors.DefaultSelector()
            selector.register(worker.stdout, selectors.EVENT_READ)
            def request(value):
                worker.stdin.write(json.dumps(value) + '\n')
                worker.stdin.flush()
                self.assertTrue(selector.select(6), 'Math worker did not answer within its time limit')
                line = worker.stdout.readline()
                self.assertTrue(line, f'Math worker exited: {worker.poll()}')
                return json.loads(line)
            try:
                for value in [None, [], {}, {'expression': None}, {'expression': '1/0'},
                              {'expression': r'\input{private}'}, {'expression': 'x^1001'},
                              {'expression': 'x^2', 'operation': 'differentiate', 'variable': 'pi'},
                              {'expression': r'\int_{-1}^{1} sqrt(x) dx'},
                              {'expression': 'x', 'operation': 'graph', 'x_min': 1, 'x_max': 1}]:
                    with self.subTest(value=value):
                        self.assertIn('error', request(value))
                result = request({'expression': '(x+1)^999-x^999', 'operation': 'factor'})
                self.assertIn('error', result)
                self.assertIn('time limit', result['error'])
                self.assertEqual(request({'expression': '2x+3=11'})['answer'], '{4}')
                self.assertIsNone(worker.poll())
            finally:
                selector.close()
                worker.terminate()
                try:
                    worker.wait(timeout=3)
                except subprocess.TimeoutExpired:
                    worker.kill()
                    worker.wait(timeout=3)
                worker.stdin.close()
                worker.stdout.close()
                worker.stderr.close()


if __name__ == '__main__':
    unittest.main()
