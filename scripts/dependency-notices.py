#!/usr/bin/env python3
"""Generate the locked Linux/Windows dependency/license inventory and copy notices.
Uses only Python's standard library and cargo metadata; no license is inferred.
Run after changing Cargo.lock, and review the missing-notice report.
"""
import json
from pathlib import Path
import shutil
import subprocess

ROOT = Path(__file__).resolve().parents[1]
def main():
    resolved = {}
    for target in ('x86_64-unknown-linux-gnu', 'x86_64-pc-windows-msvc'):
        result = subprocess.run(['cargo', 'metadata', '--locked', '--format-version', '1', '--filter-platform', target], cwd=ROOT, capture_output=True, check=True)
        metadata = json.loads(result.stdout)
        active = {node['id'] for node in metadata['resolve']['nodes']}
        resolved.update({p['id']: p for p in metadata['packages'] if p['id'] in active and p['id'] not in metadata['workspace_members']})
    packages = sorted(resolved.values(), key=lambda p:(p['name'], p['version']))
    existing = (ROOT / 'LICENSES.md').read_text()
    windows_notice = '\n## Windows package\n' + existing.split('\n## Windows package\n', 1)[1] if '\n## Windows package\n' in existing else ''
    notices = ROOT / 'third_party' / 'licenses'
    notices.mkdir(parents=True, exist_ok=True)
    rows, missing = [], []
    for package in packages:
        if not package.get('license') and not package.get('license_file'):
            raise SystemExit(f"Unlicensed dependency: {package['name']}")
        source = Path(package['manifest_path']).parent
        key = f"{package['name']}-{package['version']}"
        files = []
        for path in source.rglob('*'):
            if path.is_file() and path.name.upper().startswith(('LICENSE', 'LICENCE', 'NOTICE', 'COPYING', 'COPYRIGHT', 'UNLICENSE', 'OFL', 'FTL.TXT')):
                files.append(path)
        if package.get('license_file'):
            files.append(source / package['license_file'])
        for path in set(files):
            if path.is_file():
                dest = notices / key / path.relative_to(source)
                dest.parent.mkdir(parents=True, exist_ok=True)
                shutil.copyfile(path, dest)
        retained = any((notices / key).rglob('*')) if (notices / key).exists() else False
        if not files and not retained:
            missing.append(key)
        license_name = package.get('license') or 'See license file'
        repo = package.get('repository') or f"https://crates.io/crates/{package['name']}/{package['version']}"
        link = f"[notices](third_party/licenses/{key}/)" if files or retained else '[upstream source](' + repo + ')'
        rows.append(f"| [{package['name']}]({repo}) | {package['version']} | {license_name} | {link} |")
    header = '''# Third-party licenses

Folio's original source is GPL-3.0-or-later; see [LICENSE](LICENSE). Dependencies retain their own licenses. This inventory is generated from Cargo.lock's resolved Linux and Windows graphs, including build dependencies. It records package-declared SPDX expressions without replacing upstream notices. Slash-separated legacy dual-license declarations mean alternatives.

For dual-licensed dependencies, select MIT where offered, otherwise Apache-2.0 where offered. In particular, `oo7` is used under Apache-2.0 rather than its alternative GPL-2.0-only license. MPL-2.0 `option-ext` is unmodified, and its source is available from the exact version linked below. Binary distributors must provide the matching Folio source and retained notices; the package script creates both archives.

## Bundled components and system tools

- First-use recognition downloads pinned GLM-OCR Q8_0 weights and the llama.cpp b11457 Vulkan runtime separately from the application. Both upstream components declare MIT licenses. Retained model attribution/card and llama.cpp license are in [third_party/ocr](third_party/ocr); setup copies model notices into the downloaded pack and preserves the runtime archive LICENSE. Model inference is local; the SDK and layout detector are not included.
- Optional Python recognition: GLM-OCR weights are declared MIT in the upstream model card. One local model handles text and math through native Transformers classes; no GLM SDK or layout-detector code is bundled. Setup retains the model card, attribution, pinned revision and file hashes. Python (PSF), PyTorch (BSD-3-Clause), torchvision (BSD-3-Clause), Transformers (Apache-2.0), NumPy (BSD-3-Clause) and Pillow (HPND) are optional separately installed components. The standard archive contains no model, Python wheel, CUDA runtime or dataset. Earlier ConvText inference modules remain as historical source under [third_party/htr-convtext](third_party/htr-convtext), with their GPL-3.0 license; current recognition does not load them.
- GPUI 0.2.2: Apache-2.0; local tablet/rendering patches are documented in [vendor/README.md](vendor/README.md). All upstream notices are retained.
- xattr 0.2.3: MIT or Apache-2.0; local Linux ENODATA compatibility patch.
- proc-macro-error2 2.0.1: MIT or Apache-2.0; local public proc_macro re-export compatibility patch.
- FreeType: the FreeType License (FTL), selected instead of GPL-2.0. Portions of this software are copyright © The FreeType Project (www.freetype.org). All rights reserved. The bundled FreeType and HarfBuzz notices are included under `freetype-sys`.
- SQLite's bundled engine is public domain; the Rust bindings retain their MIT license. See the SQLite source header in the corresponding `libsqlite3-sys` source package.
- STIX Two Math bundled by latex-rust: SIL Open Font License 1.1, with font copyright and notices copied under latex-rust. Equation SVG output consists of glyph outlines.
- Poppler (`pdfinfo`, `pdftoppm`): GPL-2.0-or-later. RPM/DEB use the distribution package; Flatpak bundles Poppler 26.10.0 with its COPYING notice and corresponding source in the release source archive. Original PDFs remain local.
- Packaged offline math: SymPy 1.14.0 (BSD-3-Clause) and mpmath 1.3.0 (BSD-3-Clause), including their source and distribution license notices. System Python is used, not bundled.
- Passive tablet-link diagnostic: uses separately installed [PyGObject](https://pygobject.gnome.org/) (LGPL-2.1-or-later) and [GLib/Gio](https://github.com/GNOME/glib/blob/main/COPYING) (LGPL-2.1-or-later). These system components are not bundled or required by the desktop application.
- Bluetooth HCI diagnostic: uses the separately installed BlueZ `btmon` executable (GPL-2.0-or-later), verified from this system's BlueZ RPM metadata. It is not bundled or required by the desktop application.
- Temporary per-connection sniff experiment and foreground compatibility helper: use installed BlueZ `libbluetooth.so.3` (GPL-2.0-or-later, verified from `bluez-libs` RPM metadata) through Python ctypes. Function signatures and native ABI were verified using upstream BlueZ 5.87 headers. No library binary is bundled, and the desktop application does not depend on these diagnostics.
- Krilla/krilla-svg: MIT or Apache-2.0, searchable subset-font vector PDF overlays; lopdf: MIT, original PDF object preservation and decryption. AccessKit/Unix adapter: MIT or Apache-2.0, local AT-SPI controls bridge.
- System fonts, Vulkan drivers, glibc, XCB and xkbcommon come from the Linux installation and are not copied into the binary bundle. Their distro packages retain their respective licenses.

The UI uses 37 embedded Tabler Outline SVGs by Paweł Kuna, licensed under MIT, with stroke width adjusted to 1.65. Source mappings, the pinned revision, upstream attribution and full license are retained in [third_party/licenses/tabler-icons](third_party/licenses/tabler-icons). The matching marker icon and notebook cover artwork are original Folio vector drawings under GPL-3.0-or-later. Goodnotes documentation was consulted for layout; no Goodnotes assets or source are redistributed.

The ink/geometric algorithms are original implementations. No perfect-freehand, Xournal++ or Rnote source was copied. [DEVELOPMENT.md](DEVELOPMENT.md) explains the design references and tradeoffs.

## Locked Rust dependencies

| Package / source | Version | Declared license | Retained text |
| --- | --- | --- | --- |
'''
    (ROOT / 'LICENSES.md').write_text(header + '\n'.join(rows) + '\n' + windows_notice)
    (ROOT / 'third_party' / 'README.md').write_text('Retained dependency license and copyright texts. Regenerate with `python3 scripts/dependency-notices.py` after updating Cargo.lock. These files retain notices. Where packages omit standalone license files, manifest/readme/source headers are retained for attribution. Exact source packages are identified by Cargo.lock and LICENSES.md.\n')
    print(f"Recorded {len(packages)} Linux/Windows dependencies; copied notices into {notices}")
    if missing:
        print('No standalone notice in source packages (review manifest/readme/upstream): ' + ', '.join(missing))

if __name__ == '__main__':
    main()
