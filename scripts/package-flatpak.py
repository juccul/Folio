#!/usr/bin/env python3
"""Build a real x86_64 Flatpak bundle with offline math and PDF previews.

Requires installed Freedesktop SDK/Platform 25.08, cmake/ninja in the SDK, and a
Python with scripts/math-solver-requirements.txt. No app/model network permission.
Poppler is fetched only at build time and verified against a pinned SHA-256.
"""
import argparse
import hashlib
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tarfile
import urllib.request

from packaging_common import APP_ID, ROOT, copy, stage_metadata, stage_payload, version

POPPLER_VERSION = '26.10.0'
POPPLER_SHA256 = '6792cb7c69205007ad87d2e936cecc5b3a31fac29ab54ffc3175fdb6b2a6ce35'


def run(*arguments):
    subprocess.run(list(map(str, arguments)), check=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, default=ROOT / 'target/release/folio')
    parser.add_argument('--math-python', type=Path, default=Path(sys.executable))
    parser.add_argument('--output', type=Path, default=ROOT / 'artifacts/dist')
    parser.add_argument('--work-dir', type=Path, default=ROOT / 'artifacts/flatpak')
    args = parser.parse_args()
    if not shutil.which('flatpak'):
        parser.error('flatpak is required')
    work = args.work_dir.resolve()
    work.mkdir(parents=True, exist_ok=True)
    args.output = args.output.resolve()
    args.output.mkdir(parents=True, exist_ok=True)
    source_archive = work / f'poppler-{POPPLER_VERSION}.tar.xz'
    if not source_archive.exists():
        temporary = source_archive.with_suffix('.tmp')
        urllib.request.urlretrieve(f'https://poppler.freedesktop.org/{source_archive.name}', temporary)
        temporary.replace(source_archive)
    if hashlib.sha256(source_archive.read_bytes()).hexdigest() != POPPLER_SHA256:
        raise SystemExit('Poppler source SHA-256 mismatch')
    source = work / f'poppler-{POPPLER_VERSION}'
    if not source.exists():
        with tarfile.open(source_archive) as archive:
            archive.extractall(work, filter='data')
    build = work / 'flatpak-package'
    if build.exists():
        shutil.rmtree(build)
    run('flatpak', 'build-init', '--arch=x86_64', build, APP_ID,
        'org.freedesktop.Sdk', 'org.freedesktop.Platform', '25.08')
    prefix = build / 'files'
    stage_payload(prefix, args.binary, args.math_python)
    stage_metadata(prefix)
    bind = f'--bind-mount=/run/poppler-build={work}'
    run('flatpak', 'build', bind, build, 'cmake', '-S', f'/run/poppler-build/{source.name}',
        '-B', '/run/poppler-build/poppler-build', '-G', 'Ninja', '-DCMAKE_BUILD_TYPE=Release',
        '-DCMAKE_INSTALL_PREFIX=/app', '-DCMAKE_INSTALL_LIBDIR=lib',
        '-DCMAKE_BUILD_WITH_INSTALL_RPATH=ON', '-DCMAKE_INSTALL_RPATH=/app/lib',
        '-DENABLE_CPP=OFF', '-DENABLE_GLIB=OFF', '-DENABLE_QT5=OFF', '-DENABLE_QT6=OFF',
        '-DENABLE_LIBCURL=OFF', '-DENABLE_NSS3=OFF', '-DENABLE_GPGME=OFF',
        '-DENABLE_BOOST=OFF', '-DBUILD_MANUAL_TESTS=OFF')
    run('flatpak', 'build', bind, build, 'cmake', '--build',
        '/run/poppler-build/poppler-build', '--parallel', '3', '--target', 'pdftoppm')
    copy(work / 'poppler-build/utils/pdftoppm', prefix / 'bin/pdftoppm')
    for library in (work / 'poppler-build').glob('libpoppler.so.*'):
        if re.fullmatch(r'libpoppler\.so\.\d+', library.name):
            copy(library, prefix / 'lib' / library.name)
    copy(source / 'COPYING', prefix / 'share/doc/folio/Poppler-COPYING')
    run('flatpak', 'build', build, 'sh', '-c',
        'strip /app/bin/pdftoppm /app/lib/libpoppler.so.*')
    # Test with the Platform, not the SDK that could mask missing runtime libraries.
    run('flatpak', 'build', '--runtime', build, '/app/bin/folio', '--version')
    run('flatpak', 'build', '--runtime', build, '/app/bin/pdftoppm', '-v')
    run('flatpak', 'build', '--runtime', build, '/app/bin/math-python', '-c',
        'import sympy, mpmath; assert sympy.__version__ == "1.14.0"; '
        'assert mpmath.__version__ == "1.3.0"; print("Offline math runtime OK")')
    run('flatpak', 'build-finish', '--command=folio', '--share=ipc',
        '--socket=x11', '--socket=wayland', '--device=dri', build)
    repository = work / 'repo'
    run('flatpak', 'build-export', repository, build, 'stable')
    bundle = args.output / f'folio-{version()}-x86_64.flatpak'
    if bundle.exists():
        bundle.unlink()
    run('flatpak', 'build-bundle', '--arch=x86_64',
        '--runtime-repo=https://dl.flathub.org/repo/flathub.flatpakrepo',
        repository, bundle, APP_ID, 'stable')
    print(f'Created {bundle}')


if __name__ == '__main__':
    main()
