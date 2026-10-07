#!/usr/bin/env python3
"""Download pinned, hash-verified benchmark assets into ignored artifacts."""
import concurrent.futures
import hashlib
import json
from pathlib import Path
import tarfile
import subprocess
import urllib.request

ROOT = Path(__file__).resolve().parents[1] / 'artifacts/benchmarks/vulkan-2026-10-07'
REVISION = '65a42de1148dbed2297e922b5dbc7d9b70c36578'
FILES = {
    'GLM-OCR-Q8_0.gguf': '45bc244a6446aff850521dc41f18bc8d7105ad5f0c2c8c28af04e7cc4f4d50b1',
    'GLM-OCR-f16.gguf': 'b06675e983db9593db78603b06f097e48c0cf078b37731c0a09612f4a249cf6f',
    'mmproj-GLM-OCR-Q8_0.gguf': '9c4b58e33e316ed142eb5dcb41abec3844d3e6e5dc361ffb782c3fa9d175141f',
}


def download(item):
    name, url, sha = item
    path = ROOT / name
    if not path.exists():
        temp = path.with_suffix(path.suffix + '.partial')
        subprocess.run(['curl', '--fail', '--location', '--silent', '--show-error',
                        '--retry', '3', '--retry-all-errors', '--retry-delay', '1',
                        '--continue-at', '-', '--output', str(temp), url], check=True)
        temp.rename(path)
    actual = hashlib.file_digest(path.open('rb'), 'sha256').hexdigest()
    if actual != sha:
        raise ValueError(f'Hash mismatch for {name}: {actual}')
    print(f'Verified {name}: {path.stat().st_size} bytes', flush=True)
    return {'name': name, 'url': url, 'sha256': actual, 'bytes': path.stat().st_size}


def main():
    ROOT.mkdir(parents=True, exist_ok=True)
    release_path = ROOT / 'releases.json'
    if not release_path.exists():
        with urllib.request.urlopen('https://api.github.com/repos/ggml-org/llama.cpp/releases/tags/b11457') as response:
            release_path.write_text(json.dumps([json.load(response)], indent=2) + '\n')
    releases = json.loads(release_path.read_text())
    release = next(r for r in releases if r['tag_name'] == 'b11457')
    asset = next(a for a in release['assets'] if a['name'] == 'llama-b11457-bin-ubuntu-vulkan-x64.tar.gz')
    items = [(asset['name'], asset['browser_download_url'], asset['digest'].split(':')[1])]
    items += [(name, f'https://huggingface.co/ggml-org/GLM-OCR-GGUF/resolve/{REVISION}/{name}', sha)
              for name, sha in FILES.items()]
    with concurrent.futures.ThreadPoolExecutor(max_workers=4) as pool:
        records = list(pool.map(download, items))
    runtime = ROOT / 'runtime'
    runtime.mkdir(exist_ok=True)
    with tarfile.open(ROOT / asset['name']) as archive:
        archive.extractall(runtime, filter='data')
    (ROOT / 'assets.json').write_text(json.dumps({'model_revision': REVISION,
        'runtime_tag': release['tag_name'], 'assets': records}, indent=2) + '\n')
    source = ROOT / 'source'
    if not source.exists():
        subprocess.run(['git', 'clone', '--depth', '1', '--branch', 'b11457',
                        'https://github.com/ggml-org/llama.cpp.git', str(source)], check=True)


if __name__ == '__main__':
    main()
