#!/usr/bin/env python3
"""Create FP16 controls and verify published GGUF tensor provenance."""
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys

APP = Path(__file__).resolve().parents[1]
OLD = APP / 'artifacts/benchmarks/recognition-2026-10-04'
ROOT = APP / 'artifacts/benchmarks/vulkan-2026-10-07'
SOURCE = ROOT / 'source'
COMMIT = '5ad1c5da0ad7f6176256b823925aad19134f0263'


def main():
    assert subprocess.check_output(['git', '-C', str(SOURCE), 'rev-parse', 'HEAD'], text=True).strip() == COMMIT
    model = OLD / 'models/glm-ocr'
    weight_sha = hashlib.file_digest((model / 'model.safetensors').open('rb'), 'sha256').hexdigest()
    assert weight_sha == 'a16eb0de98d199293371c560f95f83130d2a2c9612449df16839f08ff9498815'
    python = OLD / 'venv/bin/python'
    commands = []
    for precision, projector in [('f16', False), ('f16', True), ('q8_0', False), ('q8_0', True)]:
        name = ('mmproj-' if projector else '') + f'pinned-{precision}.gguf'
        target = ROOT / name
        command = [str(python), str(SOURCE / 'convert_hf_to_gguf.py'), str(model),
                   '--outfile', str(target), '--outtype', precision]
        if projector:
            command += ['--mmproj']
        commands.append(command)
        if not target.exists():
            with (ROOT / 'logs' / f'convert-{name}.log').open('w') as log:
                result = subprocess.run(command, env=dict(os.environ, HF_HUB_OFFLINE='1',
                    TRANSFORMERS_OFFLINE='1', OMP_NUM_THREADS='4'), stdout=log, stderr=subprocess.STDOUT)
            if result.returncode:
                print((ROOT / 'logs' / f'convert-{name}.log').read_text()[-5000:], file=sys.stderr)
                raise SystemExit(result.returncode)
        print(f'Converted {name}: {target.stat().st_size} bytes', flush=True)
    # Run the GGUF reader with the benchmark environment's NumPy and source module.
    probe = r'''
import hashlib,json
from pathlib import Path
from gguf import GGUFReader
r=Path(__import__('sys').argv[1])
out={}
for a,b in [('pinned-f16.gguf','GLM-OCR-f16.gguf'),
            ('pinned-q8_0.gguf','GLM-OCR-Q8_0.gguf'),
            ('mmproj-pinned-q8_0.gguf','mmproj-GLM-OCR-Q8_0.gguf')]:
 x={t.name:t for t in GGUFReader(r/a).tensors};y={t.name:t for t in GGUFReader(r/b).tensors}
 changed=[]
 for n in sorted(x.keys()&y.keys()):
  xx,yy=x[n],y[n]
  if xx.tensor_type != yy.tensor_type or xx.shape.tolist()!=yy.shape.tolist() or hashlib.sha256(xx.data).digest()!=hashlib.sha256(yy.data).digest():changed.append(n)
 out[b]={'tensor_count':len(x),'official_tensor_count':len(y),'only_pinned':sorted(x.keys()-y.keys()),'only_official':sorted(y.keys()-x.keys()),'different_tensors':changed,'all_tensors_identical':not changed and x.keys()==y.keys()}
print(json.dumps(out,indent=2))
(r/'tensor-provenance.json').write_text(json.dumps(out,indent=2)+'\n')
'''
    subprocess.run([str(python), '-c', probe, str(ROOT)], check=True,
                   env=dict(os.environ, PYTHONPATH=str(SOURCE / 'gguf-py')))
    records = [{'name': p.name, 'bytes': p.stat().st_size,
                'sha256': hashlib.file_digest(p.open('rb'), 'sha256').hexdigest()}
               for p in sorted(ROOT.glob('*pinned*.gguf'))]
    (ROOT / 'conversion.json').write_text(json.dumps({'source_commit': COMMIT,
        'source_checkpoint_revision': '2e85a62840ccac27daa451df36c736c4636b8628',
        'source_weight_sha256': weight_sha, 'commands': commands, 'files': records}, indent=2) + '\n')


if __name__ == '__main__':
    main()
