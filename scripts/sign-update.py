#!/usr/bin/env python3
"""Sign immutable, versioned Windows/Flatpak update metadata after packaging.

The Ed25519 private key must live outside the source/distribution trees. The
matching public key is embedded in Folio. Do not reuse a version for hotfixes.
"""
import argparse
import base64
import hashlib
import json
from pathlib import Path
import subprocess
import tempfile
import tomllib

ROOT = Path(__file__).resolve().parents[1]


def asset(dist, repository, version, suffix):
    name = f'folio-{version}-{suffix}'
    path = dist / name
    with path.open('rb') as source:
        digest = hashlib.file_digest(source, 'sha256').hexdigest()
    return {'url': f'https://github.com/{repository}/releases/download/v{version}/{name}', 'sha256': digest, 'size': path.stat().st_size}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--dist', required=True, type=Path)
    parser.add_argument('--flatpak-repo', required=True, type=Path)
    parser.add_argument('--key', required=True, type=Path)
    args = parser.parse_args()
    dist = args.dist.resolve(); key = args.key.resolve()
    if key.is_relative_to(ROOT) or key.is_relative_to(dist):
        parser.error('Keep the private key outside the project and release directories')
    version = tomllib.loads((ROOT / 'Cargo.toml').read_text())['workspace']['package']['version']
    channel = json.loads((ROOT / 'packaging/updates/channel.json').read_text())
    public = subprocess.check_output(['openssl', 'pkey', '-in', str(key), '-pubout', '-outform', 'DER'])[-32:]
    if base64.b64encode(public).decode() != channel['public_key']:
        parser.error('The private key does not match the public key embedded in this release')
    package = dist / f'folio-{version}-windows-x64'
    manifest = json.loads((package / 'manifest.json').read_text())
    binary = package / 'bin/folio.exe'
    with binary.open('rb') as source:
        digest = hashlib.file_digest(source, 'sha256').hexdigest()
    if manifest['version'] != version or manifest['binary_sha256'] != digest:
        parser.error('The packaged executable does not match this release')
    commit = (args.flatpak_repo.resolve() / 'refs/heads/app/io.github.folio.Notes/x86_64/stable').read_text().strip()
    if len(commit) != 64 or any(c not in '0123456789abcdef' for c in commit):
        parser.error('Invalid Flatpak commit reference')
    metadata = {'schema': 1, 'version': version,
                'windows_installer': asset(dist, channel['repository'], version, 'windows-x64-setup.exe'),
                'windows_portable': asset(dist, channel['repository'], version, 'windows-x64.zip'),
                'windows_binary_sha256': digest, 'flatpak_commit': commit}
    payload = json.dumps(metadata, sort_keys=True, separators=(',', ':')).encode()
    with tempfile.TemporaryDirectory(prefix='folio-sign-') as temporary:
        source = Path(temporary) / 'manifest'; source.write_bytes(payload)
        signature = subprocess.check_output(['openssl', 'pkeyutl', '-sign', '-rawin', '-inkey', str(key), '-in', str(source)])
    envelope = {'payload': base64.b64encode(payload).decode(), 'signature': base64.b64encode(signature).decode()}
    target = dist / 'folio-update.json'
    target.write_text(json.dumps(envelope, indent=2) + '\n')
    print(f'Signed {target.name} for Folio {version}; private key was not copied')


if __name__ == '__main__':
    main()
