#!/usr/bin/env python3
"""Assemble an offline CPU math pack from an already installed SymPy runtime."""
import argparse
import json
from pathlib import Path
import shutil
import sys

ROOT=Path(__file__).resolve().parents[1]
def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output',type=Path,default=ROOT/'artifacts/math-solver')
    args=parser.parse_args()
    import sympy,mpmath
    if sympy.__version__!='1.14.0' or mpmath.__version__!='1.3.0':
        parser.error('Install scripts/math-solver-requirements.txt in this Python runtime first.')
    output=args.output.resolve();output.mkdir(parents=True,exist_ok=True)
    for name in ('math-solver-worker.py','math_parser.py'):
        shutil.copy2(ROOT/'scripts'/name,output/name)
    # Retain the actual installed distributions' license notices.
    import importlib.metadata
    notices=[]
    for name in ('sympy','mpmath'):
        distribution=importlib.metadata.distribution(name)
        for file in distribution.files or []:
            if 'LICENSE' in file.name.upper():
                path=Path(distribution.locate_file(file))
                if path.is_file(): notices.append(f'===== {name}: {file.name} =====\n'+path.read_text())
    (output/'THIRD-PARTY-NOTICES.txt').write_text('\n\n'.join(notices))
    config={'python':sys.executable,'worker':'math-solver-worker.py','timeout_seconds':8,
            'sympy':sympy.__version__,'mpmath':mpmath.__version__,'engine':'folio-math-1'}
    temporary=output/'pack.json.tmp';temporary.write_text(json.dumps(config,indent=2)+'\n')
    temporary.replace(output/'pack.json')
    link=ROOT/'target/math-solver'
    if sys.platform != 'win32' and not link.exists() and not link.is_symlink():
        link.parent.mkdir(parents=True,exist_ok=True)
        link.symlink_to(output,target_is_directory=True)
    print(f'Offline math solver ready: {output / "pack.json"}')

if __name__=='__main__': main()
