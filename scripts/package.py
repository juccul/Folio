#!/usr/bin/env python3
"""Build local Linux binary and complete corresponding-source archives.
Does not install, deploy or publish. Cargo vendors registry source into a temporary
folder so the source archive can build offline, including third-party code/notices.
"""
import argparse
import hashlib
import json
import os
import re
from pathlib import Path
import subprocess
import tarfile
import tempfile
import tomllib

ROOT = Path(__file__).resolve().parents[1]
PROJECT_DOCUMENTS = (
    'README.md', 'WINDOWS.md', 'UPDATES.md', 'DEVELOPMENT.md',
    'RELEASE_NOTES.md', 'LICENSES.md',
)

def add(archive, path, name):
    def filter_file(info):
        if Path(info.name).name in {'.cargo-ok', '.git', '__pycache__'} or info.name.endswith('.pyc'):
            return None
        info.uid = info.gid = 0
        info.uname = info.gname = 'root'
        return info
    archive.add(path, arcname=name, filter=filter_file)

RUNE_GIT = 'https://github.com/anomalyco/rune'


def rune_checkout():
    """Cargo's checkout of the pinned Rune revision and its crates by name."""
    metadata = json.loads(subprocess.check_output(
        ['cargo', 'metadata', '--locked', '--format-version', '1'], cwd=ROOT, text=True))
    crates = {package['name']: Path(package['manifest_path']).parent
              for package in metadata['packages']
              if (package.get('source') or '').startswith(f'git+{RUNE_GIT}')}
    if not crates:
        raise SystemExit('Rune is not a git dependency of this workspace')
    root = Path(os.path.commonpath(list(crates.values())))
    while not (root / 'Cargo.toml').is_file() or 'workspace' not in tomllib.loads((root / 'Cargo.toml').read_text()):
        root = root.parent
    return root, crates


def stage_source(tree, poppler_source):
    """The workspace as a self-contained source tree: Rune's git dependencies
    point at a bundled copy of the pinned Rune revision (with Folio's patched
    rune-image in place of its own), so the archive builds without network."""
    import shutil
    ignore = shutil.ignore_patterns('.git', 'target', '__pycache__', '*.pyc', '.cargo-ok')
    tree.mkdir(parents=True)
    for item in ['Cargo.toml', 'Cargo.lock', 'apps', 'crates', 'vendor', 'scripts', 'packaging',
                 'recognition', 'third_party', '.github', '.gitignore', '.cargo', 'LICENSE',
                 'AGENTS.md', *PROJECT_DOCUMENTS]:
        source = ROOT / item
        if source.is_dir():
            shutil.copytree(source, tree / item, ignore=ignore)
        elif source.is_file():
            shutil.copy2(source, tree / item)
    root, crates = rune_checkout()
    rune = tree / 'third_party/rune'
    shutil.copytree(root, rune, ignore=ignore)
    # Folio patches rune-image (so Cargo reports the patch, not Rune's copy).
    original = next(path.parent for path in root.glob('crates/*/Cargo.toml')
                    if tomllib.loads(path.read_text()).get('package', {}).get('name') == 'rune-image')
    patched = rune / original.relative_to(root)
    shutil.rmtree(patched)
    shutil.copytree(ROOT / 'vendor/rune-image', patched, ignore=ignore)
    pin = re.compile(r'git = "' + re.escape(RUNE_GIT) + r'", rev = "[0-9a-f]+"')
    for manifest, base in [(tree / 'Cargo.toml', tree), (tree / 'vendor/rune-image/Cargo.toml', tree / 'vendor/rune-image'),
                           (patched / 'Cargo.toml', patched)]:
        text = manifest.read_text()
        def local(match):
            name = text[:match.start()].rsplit('\n', 1)[-1].split('=', 1)[0].strip()
            return f'path = "{os.path.relpath(rune / crates[name].relative_to(root), base)}"'
        text = pin.sub(local, text)
        # Patches of the git source do not apply to path dependencies; the
        # patched crate already sits inside the bundled copy.
        text = re.sub(r'\[patch\."' + re.escape(RUNE_GIT) + r'"\]\n(?:[^\[\n][^\n]*\n)*', '', text)
        if manifest == tree / 'Cargo.toml':
            # Rune's crates belong to Rune's own workspace.
            text = text.replace('exclude = [', 'exclude = ["third_party/rune", ', 1)
        manifest.write_text(text)
    if poppler_source:
        retained = ROOT / 'third_party/flatpak' / poppler_source.name
        if retained.is_file():
            if hashlib.sha256(retained.read_bytes()).digest() != hashlib.sha256(poppler_source.read_bytes()).digest():
                raise SystemExit('Retained Poppler source differs from the packaged build source')
        else:
            (tree / 'third_party/flatpak').mkdir(parents=True, exist_ok=True)
            shutil.copy2(poppler_source, tree / 'third_party/flatpak' / poppler_source.name)


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
        for item in [*PROJECT_DOCUMENTS, 'LICENSE', 'recognition', 'third_party', 'packaging']:
            add(archive, ROOT / item, f'{binary_name}/{item}')
    with tempfile.TemporaryDirectory(prefix='folio-corresponding-source-') as temp:
        tree = Path(temp) / source_name
        stage_source(tree, args.poppler_source)
        # Registry crates, vendored so the archive builds offline. Rune's
        # repository is bundled whole (it carries its own copy of a crate that
        # also comes from the registry, which `cargo vendor` cannot merge).
        result = subprocess.run(['cargo', 'vendor', '--versioned-dirs', 'third_party/source'],
                                cwd=tree, capture_output=True, text=True)
        if result.returncode:
            raise SystemExit(f'cargo vendor failed:\n{result.stderr}')
        config = tree / '.cargo/config.toml'
        config.write_text(config.read_text() + '\n' + result.stdout)
        with tarfile.open(source_archive, 'w:gz', compresslevel=6) as archive:
            add(archive, tree, source_name)
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
