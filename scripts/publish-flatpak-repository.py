#!/usr/bin/env python3
"""Prepare a GitHub Pages tree for Folio's signed Flatpak distribution repository.

This prepares files locally. Publishing and enabling Pages are separate explicit
release operations, after final package verification.
"""
import argparse
from pathlib import Path
import shutil


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--repo', type=Path, required=True)
    parser.add_argument('--dist', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True, help='Fresh local staging directory')
    args = parser.parse_args()
    repo = args.repo.resolve(); output = args.output.resolve()
    if output.exists():
        parser.error('Choose a fresh output directory')
    if not (repo / 'summary.sig').is_file():
        parser.error('Only a signed repository can be published')
    commit = (repo / 'refs/heads/app/io.github.folio.Notes/x86_64/stable').read_text().strip()
    if len(commit) != 64 or any(c not in '0123456789abcdef' for c in commit):
        parser.error('Invalid application commit reference')
    output.mkdir(parents=True)
    target = output / 'flatpak'
    target.mkdir()
    # OSTree archive repositories consist of immutable compressed objects and
    # mutable signed refs/summary metadata. Never publish local lock/tmp state.
    for name in ['objects', 'refs', 'deltas', 'summaries']:
        if (repo / name).is_dir():
            shutil.copytree(repo / name, target / name, symlinks=True)
    for name in ['summary', 'summary.sig', 'summary.idx', 'summary.idx.sig', 'config']:
        if (repo / name).is_file():
            shutil.copy2(repo / name, target / name)
    for name in ['folio.flatpakrepo', 'folio.flatpakref']:
        shutil.copy2(args.dist / name, output / name)
    (output / '.nojekyll').touch()
    (output / 'index.html').write_text('<!doctype html><meta charset="utf-8"><title>Folio downloads</title><h1>Folio</h1><p><a href="folio.flatpakref">Install Folio for Linux</a></p><p>Windows downloads and corresponding source are published with each release at <a href="https://github.com/juccul/Folio/releases">GitHub releases</a>.</p>\n')
    print(f'Prepared signed repository and installation references in {output}')


if __name__ == '__main__':
    main()
