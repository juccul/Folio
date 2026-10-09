#!/usr/bin/env python3
"""Assemble a relocatable Windows package with pinned offline PDF/math tools."""
import argparse
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import re
import shutil
import subprocess
import tempfile
import tomllib
import urllib.request
import zipfile

ROOT = Path(__file__).resolve().parents[1]


def sha256(path):
    with path.open('rb') as source:
        return hashlib.file_digest(source, 'sha256').hexdigest()


def msvc_runtime():
    """Use the app-local redistributables from the native build toolchain."""
    vswhere = Path(os.environ.get('ProgramFiles(x86)', 'C:/Program Files (x86)')) / 'Microsoft Visual Studio/Installer/vswhere.exe'
    if not vswhere.is_file():
        raise ValueError('MSVC runtime not found; provide its redistributable DLLs with --extra-dll.')
    installation = subprocess.check_output([
        str(vswhere), '-latest', '-products', '*', '-requires',
        'Microsoft.VisualStudio.Component.VC.Tools.x86.x64', '-property', 'installationPath', '-utf8'
    ], encoding='utf-8').strip()
    candidates = list((Path(installation) / 'VC/Redist/MSVC').glob('*/x64/Microsoft.VC*.CRT'))
    if not candidates:
        raise ValueError('No x64 MSVC redistributable directory found; use --extra-dll.')
    latest = max(candidates, key=lambda path: tuple(int(part) for part in path.parents[1].name.split('.')))
    return sorted(latest.glob('*.dll'))


def fetch(asset, cache):
    path = cache / asset['url'].rsplit('/', 1)[-1]
    cache.mkdir(parents=True, exist_ok=True)
    if not path.exists() or sha256(path) != asset['sha256']:
        partial = path.with_suffix(path.suffix + '.partial')
        with urllib.request.urlopen(asset['url'], timeout=60) as source, partial.open('wb') as target:
            shutil.copyfileobj(source, target)
        if sha256(partial) != asset['sha256']:
            partial.unlink()
            raise ValueError(f'Checksum mismatch: {path.name}')
        partial.replace(path)
    return path


def safe_archive_path(entry):
    # ZipInfo normalizes Windows separators and truncates NULs in filename.
    # Validate the original archive spelling before either transformation.
    name = entry.orig_filename
    path = PurePosixPath(name)
    if path.is_absolute() or '..' in path.parts or '\\' in name or ':' in name or '\0' in name:
        raise ValueError(f'Unsafe archive path: {name}')
    if (entry.external_attr >> 16) & 0o170000 == 0o120000:
        raise ValueError(f'Archive symlink: {name}')
    return path


def extract(archive, destination, prefix=''):
    with zipfile.ZipFile(archive) as source:
        for entry in source.infolist():
            name = entry.filename
            safe_archive_path(entry)
            if not name.startswith(prefix):
                continue
            relative = name[len(prefix):]
            if not relative:
                continue
            target = destination.joinpath(*PurePosixPath(relative).parts)
            if entry.is_dir():
                target.mkdir(parents=True, exist_ok=True)
            else:
                target.parent.mkdir(parents=True, exist_ok=True)
                with source.open(entry) as input_file, target.open('wb') as output:
                    shutil.copyfileobj(input_file, output)


def stage_python_packages(archives, destination):
    """Keep every pure-Python wheel member in one standard zipimport payload."""
    destination.parent.mkdir(parents=True, exist_ok=True)
    seen = set()
    # Store members once; the outer portable ZIP provides the compression.
    with zipfile.ZipFile(destination, 'w', compression=zipfile.ZIP_STORED) as target:
        for archive in archives:
            with zipfile.ZipFile(archive) as source:
                for entry in source.infolist():
                    path = safe_archive_path(entry)
                    key = path.as_posix().rstrip('/').casefold()
                    if key in seen:
                        raise ValueError(f'Duplicate Python wheel path: {entry.filename}')
                    seen.add(key)
                    if not entry.is_dir() and path.suffix.lower() in ('.pyd', '.dll', '.so'):
                        raise ValueError(f'Python zipimport needs pure-Python wheels: {entry.filename}')
                    target.writestr(entry, source.read(entry), compress_type=zipfile.ZIP_STORED)


def stage_notices(destination):
    """Retain full notices and exact originals without thousands of disk writes."""
    source = ROOT / 'third_party/licenses'
    target = destination / 'third_party/licenses'
    target.mkdir(parents=True, exist_ok=True)
    index = ['# Retained dependency notices', '',
             'All original files are retained byte for byte in [licenses.zip](licenses.zip).',
             'The [full searchable text](THIRD_PARTY_NOTICES.txt) has original path and SHA-256 markers.',
             'Search either file for the package name or path below. Non-UTF-8 bytes are escaped in the text;',
             'the archive preserves the exact original bytes. The corresponding source retains the full tree.',
             '', '| Original path | SHA-256 | Bytes |', '| --- | --- | --- |']
    with (target / 'THIRD_PARTY_NOTICES.txt').open('wb') as notices, zipfile.ZipFile(
            target / 'licenses.zip', 'w', compression=zipfile.ZIP_DEFLATED) as originals:
        notices.write(('Folio retained third-party notices\n'
                       'Original paths and SHA-256 markers identify every full notice.\n'
                       'licenses.zip retains exact original bytes; the corresponding source retains the full tree.\n\n').encode())
        for path in sorted(source.rglob('*')):
            if not path.is_file():
                continue
            relative = path.relative_to(source).as_posix()
            raw = path.read_bytes()
            checksum = hashlib.sha256(raw).hexdigest()
            notices.write(f'===== third_party/licenses/{relative} | SHA-256 {checksum} =====\n'.encode())
            notices.write(raw.decode('utf-8', errors='backslashreplace').encode('utf-8'))
            notices.write(b'\n\n')
            originals.writestr(relative, raw)
            index.append(f'| `{relative}` | `{checksum}` | {len(raw)} |')
    (target / 'INDEX.md').write_text('\n'.join(index) + '\n', encoding='utf-8')
    inventory = destination / 'LICENSES.md'
    if inventory.is_file():
        text = inventory.read_text(encoding='utf-8')
        text = re.sub(r'\]\(third_party/licenses/[^)]+\)', '](third_party/licenses/INDEX.md)', text)
        text = ('Windows package notices: see [the searchable index](third_party/licenses/INDEX.md), '
                '[full text](third_party/licenses/THIRD_PARTY_NOTICES.txt), and '
                '[exact original files](third_party/licenses/licenses.zip).\n\n' + text)
        inventory.write_text(text, encoding='utf-8')


def stage_runtime(destination, cache):
    assets = json.loads((ROOT / 'packaging/windows/runtime-assets.json').read_text())
    files = {name: fetch(asset, cache) for name, asset in assets.items()}
    extract(files['python'], destination / 'python')
    python_paths = list((destination / 'python').glob('python*._pth'))
    if len(python_paths) != 1:
        raise ValueError('Expected one embedded Python path configuration')
    standard_library = python_paths[0].stem + '.zip'
    python_paths[0].write_text(standard_library + '\n.\n../math-solver\n../math-solver/site-packages.zip\n', encoding='utf-8')
    stage_python_packages([files[name] for name in ('sympy', 'mpmath')],
                          destination / 'math-solver/site-packages.zip')
    for name in ('math-solver-worker.py', 'math_parser.py'):
        shutil.copy2(ROOT / 'scripts' / name, destination / 'math-solver' / name)
    config = {'python': '../python/python.exe', 'worker': 'math-solver-worker.py',
              'timeout_seconds': 8, 'sympy': '1.14.0', 'mpmath': '1.3.0', 'engine': 'folio-math-1'}
    (destination / 'math-solver/pack.json').write_text(json.dumps(config, indent=2) + '\n', encoding='utf-8')
    with zipfile.ZipFile(files['poppler']) as archive:
        roots = {name.split('/', 1)[0] for name in archive.namelist()}
    if len(roots) != 1:
        raise ValueError('Unexpected Poppler archive layout')
    prefix = roots.pop() + '/'
    # Preserve the toolchain DLLs, font data, and notices in a portable layout.
    extract(files['poppler'], destination / 'pdf', prefix + 'Library/')
    extract(files['poppler'], destination / 'pdf/share', prefix + 'share/')
    if not (destination / 'pdf/bin/pdftoppm.exe').is_file():
        raise ValueError('Poppler pack is missing pdftoppm.exe')
    (destination / 'runtime-assets.json').write_text(json.dumps(assets, indent=2) + '\n', encoding='utf-8')
    return assets


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, default=ROOT / 'target/x86_64-pc-windows-msvc/release/folio.exe')
    parser.add_argument('--output', type=Path, default=ROOT / 'artifacts/dist')
    parser.add_argument('--cache', type=Path, default=ROOT / 'artifacts/windows-runtime-downloads')
    parser.add_argument('--runtime-only', action='store_true', help='Prepare tools for a native build/test before the executable exists')
    parser.add_argument('--installer', action='store_true', help='Also compile the per-user installer with an installed Inno Setup 6 compiler')
    parser.add_argument('--extra-dll', type=Path, action='append', default=[], help='Copy an app runtime dependency beside folio.exe')
    args = parser.parse_args()
    if not args.runtime_only and not args.binary.is_file():
        parser.error(f'Build the Windows executable first: {args.binary}')
    if not args.runtime_only and not args.extra_dll and os.name == 'nt':
        args.extra_dll = msvc_runtime()
    version = tomllib.loads((ROOT / 'Cargo.toml').read_text())['workspace']['package']['version']
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    name = f'folio-{version}-windows-x64'
    destination = output / name
    if destination.exists():
        parser.error(f'Output directory already exists; choose a fresh --output: {destination}')
    with tempfile.TemporaryDirectory(prefix='folio-windows-stage-', dir=output) as temporary:
        stage = Path(temporary) / name
        stage.mkdir()
        assets = stage_runtime(stage, args.cache)
        if not args.runtime_only:
            (stage / 'bin').mkdir()
            shutil.copy2(args.binary, stage / 'bin/folio.exe')
            for dll in args.extra_dll:
                shutil.copy2(dll, stage / 'bin' / dll.name)
        (stage / 'tools').mkdir()
        for file in ('verify-windows.ps1', 'verify-windows-runtime.py', 'windows-pen-replay.cs'):
            shutil.copy2(ROOT / 'scripts' / file, stage / 'tools' / file)
        for file in ('LICENSE', 'LICENSES.md', 'README.md', 'WINDOWS.md', 'UPDATES.md'):
            shutil.copy2(ROOT / file, stage / file)
        stage_notices(stage)
        shutil.copytree(ROOT / 'third_party/ocr', stage / 'third_party/ocr')
        manifest = {'version': version, 'target': 'x86_64-pc-windows-msvc', 'runtime_only': args.runtime_only,
                    'assets': assets, 'cargo_lock_sha256': sha256(ROOT / 'Cargo.lock')}
        if not args.runtime_only:
            manifest['binary_sha256'] = sha256(args.binary)
            manifest['app_runtime_dlls'] = {dll.name: sha256(dll) for dll in args.extra_dll}
        (stage / 'manifest.json').write_text(json.dumps(manifest, indent=2) + '\n', encoding='utf-8')
        shutil.move(stage, destination)
    if args.runtime_only:
        print(destination)
        return
    archive = shutil.make_archive(str(output / name), 'zip', root_dir=output, base_dir=name)
    (output / (name + '.zip.sha256')).write_text(sha256(Path(archive)) + '  ' + Path(archive).name + '\n')
    if args.installer:
        compiler = shutil.which('ISCC.exe') or shutil.which('iscc')
        if compiler is None:
            raise SystemExit('Portable ZIP is ready. Install Inno Setup 6 to compile the installer.')
        subprocess.run([compiler, f'/DPayloadDir={destination}', f'/DFolioVersion={version}',
                        f'/O{output}', str(ROOT / 'packaging/windows/folio.iss')], check=True)
        installer = output / f'{name}-setup.exe'
        if not installer.is_file():
            raise SystemExit(f'Inno Setup did not produce the expected installer: {installer.name}')
        installer.with_suffix('.exe.sha256').write_text(sha256(installer) + '  ' + installer.name + '\n')
    print(archive)


if __name__ == '__main__':
    main()
