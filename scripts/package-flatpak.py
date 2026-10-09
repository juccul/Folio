#!/usr/bin/env python3
"""Build a real x86_64 Flatpak bundle with offline math and PDF previews.

Requires installed Freedesktop SDK/Platform 25.08, cmake/ninja in the SDK, and a
Python with scripts/math-solver-requirements.txt. Network is used only for first-use OCR asset downloads.
Poppler is fetched only at build time and verified against a pinned SHA-256.
"""
import argparse
import base64
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
    parser.add_argument('--update-repo-url', help='HTTPS URL of the signed Folio OSTree repository')
    parser.add_argument('--gpg-sign', help='Release signing key fingerprint')
    parser.add_argument('--gpg-homedir', type=Path, help='Private release key directory (never packaged)')
    parser.add_argument('--gpg-public-key', type=Path, help='Exported binary public key')
    args = parser.parse_args()
    if args.update_repo_url and not (args.update_repo_url.startswith('https://') and args.gpg_sign and args.gpg_homedir and args.gpg_public_key):
        parser.error('An update repository needs HTTPS, a signing key and its exported public key')
    if not shutil.which('flatpak'):
        parser.error('flatpak is required')
    work = args.work_dir.resolve()
    work.mkdir(parents=True, exist_ok=True)
    args.output = args.output.resolve()
    args.output.mkdir(parents=True, exist_ok=True)
    source_archive = work / f'poppler-{POPPLER_VERSION}.tar.xz'
    if not source_archive.exists():
        retained_source = ROOT / 'third_party/flatpak' / source_archive.name
        if retained_source.is_file():
            copy(retained_source, source_archive)
        else:
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
    # Publish catalog metadata/icons as well as installed component metadata.
    run('flatpak', 'build', build, 'appstreamcli', 'compose', '--no-net',
        '--prefix=/app', f'--origin={APP_ID}', '--data-dir=/app/share/app-info/xmls',
        '--icons-dir=/app/share/app-info/icons', f'--components={APP_ID}', '/')
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
        '--socket=x11', '--socket=wayland', '--device=dri', '--share=network', build)
    repository = work / 'repo'
    signing = [f'--gpg-sign={args.gpg_sign}', f'--gpg-homedir={args.gpg_homedir.resolve()}'] if args.gpg_sign else []
    run('flatpak', 'build-export', *signing, repository, build, 'stable')
    if args.update_repo_url:
        run('flatpak', 'build-update-repo', *signing, '--generate-static-deltas', '--static-delta-jobs=2', '--title=Folio', '--default-branch=stable', repository)
        key = base64.b64encode(args.gpg_public_key.read_bytes()).decode('ascii')
        remote = f'[Flatpak Repo]\nTitle=Folio\nUrl={args.update_repo_url}\nDefaultBranch=stable\nGPGKey={key}\n'
        reference = f'[Flatpak Ref]\nTitle=Folio\nName={APP_ID}\nBranch=stable\nUrl={args.update_repo_url}\nIsRuntime=false\nGPGKey={key}\nRuntimeRepo=https://dl.flathub.org/repo/flathub.flatpakrepo\n'
        (args.output / 'folio.flatpakrepo').write_text(remote)
        (args.output / 'folio.flatpakref').write_text(reference)
    bundle = args.output / f'folio-{version()}-x86_64.flatpak'
    if bundle.exists():
        bundle.unlink()
    bundle_remote = [f'--repo-url={args.update_repo_url}', f'--gpg-keys={args.gpg_public_key.resolve()}'] if args.update_repo_url else []
    run('flatpak', 'build-bundle', '--arch=x86_64', *bundle_remote,
        '--runtime-repo=https://dl.flathub.org/repo/flathub.flatpakrepo',
        repository, bundle, APP_ID, 'stable')
    print(f'Created {bundle}')


if __name__ == '__main__':
    main()
