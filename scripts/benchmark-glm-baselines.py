#!/usr/bin/env python3
"""Fresh optimized Transformers baselines for the Vulkan comparison."""
import json
import os
from pathlib import Path
import subprocess
import sys

APP = Path(__file__).resolve().parents[1]
OLD = APP / 'artifacts/benchmarks/recognition-2026-10-04'
OUT = APP / 'artifacts/benchmarks/vulkan-2026-10-07'


def main():
    (OUT / 'results').mkdir(parents=True, exist_ok=True)
    (OUT / 'logs').mkdir(exist_ok=True)
    modes = [('cuda', 'bf16'), ('cuda', 'int8'), ('cpu', 'bf16')]
    for device, quant in modes:
        for kind in ('text', 'math'):
            name = f'transformers-{device}-{quant}-{kind}'
            path = OUT / 'results' / f'{name}.json'
            if path.exists() and len(json.loads(path.read_text())['samples']) == 100:
                continue
            python = OLD / 'venv/bin/python' if device == 'cuda' else APP / 'artifacts/recognition-v2/runtime/bin/python'
            command = [str(python), str(OLD / 'benchmark.py'), '--model', 'glm-ocr',
                       '--kind', kind, '--device', device, '--quantization', quant,
                       '--encoder-projection', 'linear_cuda', '--threads', '4',
                       '--limit', '100', '--profile-stages', '--out', str(path)]
            env = dict(os.environ)
            env['PYTHONPATH'] = str(OLD / 'quantization/deps') if quant == 'int8' else ''
            with (OUT / 'logs' / f'{name}.log').open('w') as log:
                print(f'Start {name}', flush=True)
                result = subprocess.run(command, cwd=APP, env=env, stdout=log, stderr=subprocess.STDOUT)
            with (OUT / 'jobs.jsonl').open('a') as jobs:
                jobs.write(json.dumps({'name': name, 'command': command, 'returncode': result.returncode}) + '\n')
            if result.returncode:
                print((OUT / 'logs' / f'{name}.log').read_text()[-4000:], file=sys.stderr)
                raise SystemExit(result.returncode)
            print(f'Completed {name}', flush=True)


if __name__ == '__main__':
    main()
