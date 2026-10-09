#!/usr/bin/env python3
"""Build local Linux binary and complete corresponding-source archives.
Does not install, deploy or publish. Cargo vendors registry source into a temporary
folder so the source archive can build offline, including third-party code/notices.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tarfile
import tempfile
import tomllib

ROOT = Path(__file__).resolve().parents[1]

def add(archive, path, name):
    def filter_file(info):
        if Path(info.name).name in {'.cargo-ok', '.git', '__pycache__'} or info.name.endswith('.pyc'):
            return None
        info.uid = info.gid = 0
        info.uname = info.gname = 'root'
        return info
    archive.add(path, arcname=name, filter=filter_file)

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--binary',type=Path,help='Use a separately built distribution-compatible executable')
    parser.add_argument('--no-build', action='store_true', help='Package an already-built release binary')
    parser.add_argument('--poppler-source', type=Path, help='Include the exact Flatpak Poppler source archive')
    parser.add_argument('--output', default='artifacts/dist')
    args = parser.parse_args()
    if not args.no_build:
        subprocess.run(['cargo', 'build', '--locked', '--release', '-p', 'folio'], cwd=ROOT, check=True)
    binary = args.binary.resolve() if args.binary else ROOT / 'target/release/folio'
    if not binary.is_file():
        raise SystemExit('Build target/release/folio first')
    subprocess.run(['python3', 'scripts/dependency-notices.py'], cwd=ROOT, check=True)
    output = Path(args.output).resolve()
    output.mkdir(parents=True, exist_ok=True)
    version = tomllib.loads((ROOT/'Cargo.toml').read_text())['workspace']['package']['version']
    binary_name = f'folio-{version}-linux-{os.uname().machine}'
    source_name = f'folio-{version}-source'
    binary_archive = output / f'{binary_name}.tar.gz'
    source_archive = output / f'{source_name}.tar.gz'
    with tarfile.open(binary_archive, 'w:gz', compresslevel=6) as archive:
        add(archive, binary, f'{binary_name}/bin/folio-native')
        add(archive, ROOT / 'packaging/folio-launcher', f'{binary_name}/bin/folio')
        for item in ['README.md', 'UPDATES.md', 'DEVELOPMENT.md', 'RECOGNITION_RESEARCH.md', 'MATH_SOLVER_DESIGN.md', 'LICENSE', 'LICENSES.md', 'third_party', 'packaging']:
            add(archive, ROOT / item, f'{binary_name}/{item}')
    with tempfile.TemporaryDirectory(prefix='folio-corresponding-source-') as temp:
        vendor = Path(temp) / 'source'
        result = subprocess.run(['cargo', 'vendor', '--locked', '--versioned-dirs', str(vendor)], cwd=ROOT, capture_output=True, text=True, check=True)
        config = result.stdout.replace(str(vendor), 'third_party/source')
        config_path = Path(temp) / 'config.toml'
        config_path.write_text((ROOT / '.cargo/config.toml').read_text() + '\n' + config)
        with tarfile.open(source_archive, 'w:gz', compresslevel=6) as archive:
            for item in ['Cargo.toml', 'Cargo.lock', 'apps', 'crates', 'vendor', 'scripts', 'packaging', 'third_party', '.github', 'LICENSE']:
                add(archive, ROOT / item, f'{source_name}/{item}')
            for item in sorted(ROOT.glob('*.md')):
                add(archive, item, f'{source_name}/{item.name}')
            if args.poppler_source:
                retained = ROOT / 'third_party/flatpak' / args.poppler_source.name
                if retained.is_file():
                    if hashlib.sha256(retained.read_bytes()).digest() != hashlib.sha256(args.poppler_source.read_bytes()).digest():
                        raise SystemExit('Retained Poppler source differs from the packaged build source')
                else:
                    add(archive, args.poppler_source,
                        f'{source_name}/third_party/flatpak/{args.poppler_source.name}')
            add(archive, config_path, f'{source_name}/.cargo/config.toml')
            add(archive, vendor, f'{source_name}/third_party/source')
    archives=[binary_archive,source_archive]
    checksums = {}
    for path in archives:
        with path.open('rb') as source:
            checksums[path.name] = hashlib.file_digest(source,'sha256').hexdigest()
    (output / 'SHA256SUMS').write_text(''.join(f'{digest}  {name}\n' for name, digest in checksums.items()))
    (output / 'manifest.json').write_text(json.dumps({'version':version, 'target':os.uname().machine, 'cargo_lock_sha256':hashlib.sha256((ROOT/'Cargo.lock').read_bytes()).hexdigest(), 'archives':checksums}, indent=2)+'\n')
    print(f'Created {binary_archive}\nCreated {source_archive}')

if __name__ == '__main__':
    main()
