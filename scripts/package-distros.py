#!/usr/bin/env python3
"""Create DEB and RPM packages with the offline CPU math solver; never install."""
import argparse
import re
import shutil
import subprocess
import sys
import tarfile
import tempfile
from pathlib import Path

from packaging_common import ROOT, stage_metadata, stage_payload, version


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, default=ROOT / 'target/release/folio')
    parser.add_argument('--math-python', type=Path, default=Path(sys.executable),
                        help='Python with pinned scripts/math-solver-requirements.txt installed')
    parser.add_argument('--output', type=Path, default=ROOT / 'artifacts/dist')
    args = parser.parse_args()
    for command in ('readelf', 'ar', 'rpmbuild'):
        if not shutil.which(command):
            parser.error(f'{command} is required')
    args.output = args.output.resolve()
    args.output.mkdir(parents=True, exist_ok=True)
    symbols = subprocess.check_output(['readelf', '--version-info', str(args.binary)], text=True)
    glibc = max(set(re.findall(r'GLIBC_([0-9.]+)', symbols)),
                key=lambda value: tuple(map(int, value.split('.'))))
    release = version()
    with tempfile.TemporaryDirectory(prefix='folio-package-') as temporary:
        temp = Path(temporary)
        stage = temp / 'stage'
        stage_payload(stage / 'usr/lib/folio', args.binary, args.math_python)
        stage_metadata(stage / 'usr')
        (stage / 'usr/bin').mkdir()
        (stage / 'usr/bin/folio').symlink_to('../lib/folio/bin/folio')
        data = temp / 'data.tar.gz'
        def root_owned(member):
            member.uid = member.gid = 0
            member.uname = member.gname = 'root'
            member.mtime = int(member.mtime)
            return member

        # dpkg rejects PAX extended headers (including fractional mtimes).
        with tarfile.open(data, 'w:gz', format=tarfile.GNU_FORMAT) as archive:
            archive.add(stage / 'usr', arcname='./usr', filter=root_owned)
        control = temp / 'control'
        control.mkdir()
        installed_size = sum(path.stat().st_size for path in stage.rglob('*')
                             if path.is_file() and not path.is_symlink()) // 1024
        (control / 'control').write_text(f'''Package: folio
Version: {release}
Architecture: amd64
Maintainer: Folio contributors <101457938+juccul@users.noreply.github.com>
Section: editors
Priority: optional
Installed-Size: {installed_size}
Homepage: https://github.com/juccul/Folio
Depends: libc6 (>= {glibc}), libgcc-s1, libx11-6, libxcb1, libxkbcommon0, libxkbcommon-x11-0, libxcb-xkb1, libwayland-client0, libvulkan1, libfontconfig1, python3 (>= 3.10), poppler-utils
Description: Offline handwriting, notes and step-by-step math
 Pressure-sensitive vector notes, PDF annotations, editable text and LaTeX,
 with a bundled offline CPU math solver. Handwriting OCR is an optional pack.
''')
        with tarfile.open(temp / 'control.tar.gz', 'w:gz', format=tarfile.GNU_FORMAT) as archive:
            archive.add(control / 'control', arcname='./control', filter=root_owned)
        (temp / 'debian-binary').write_text('2.0\n')
        deb = args.output / f'folio_{release}_amd64.deb'
        if deb.exists():
            deb.unlink()
        subprocess.run(['ar', 'rc', str(deb), 'debian-binary', 'control.tar.gz', 'data.tar.gz'],
                       cwd=temp, check=True)
        rpm = temp / 'rpm'
        (rpm / 'SOURCES').mkdir(parents=True)
        shutil.copy(data, rpm / 'SOURCES/folio-files.tar.gz')
        spec = temp / 'folio.spec'
        spec.write_text(f'''Name: folio
Version: {release}
Release: 1
Summary: Offline handwriting, notes and step-by-step math
License: GPL-3.0-or-later
URL: https://github.com/juccul/Folio
Source0: folio-files.tar.gz
BuildArch: x86_64
Requires: python3 >= 3.10, poppler-utils
Requires: libX11.so.6()(64bit), libwayland-client.so.0()(64bit), libvulkan.so.1()(64bit), libfontconfig.so.1()(64bit)
%description
Pressure-sensitive vector notes and PDF annotations with editable text and LaTeX.
Includes an offline CPU math solver. Handwriting OCR requires an optional pack.
%prep
%build
%install
mkdir -p %{{buildroot}}
tar -xzf %{{SOURCE0}} -C %{{buildroot}}
%files
/usr/bin/folio
/usr/lib/folio
/usr/share/applications/io.github.folio.Notes.desktop
/usr/share/icons/hicolor/scalable/apps/io.github.folio.Notes.svg
/usr/share/metainfo/io.github.folio.Notes.metainfo.xml
%doc /usr/share/doc/folio
''')
        subprocess.run(['rpmbuild', '-bb', '--define', f'_topdir {rpm}',
                        '--define', '__os_install_post %{nil}', str(spec)], check=True)
        for file in (rpm / 'RPMS').rglob('*.rpm'):
            shutil.copy2(file, args.output / file.name)
    print(f'Created {deb} and RPM with portable math support (glibc >= {glibc})')


if __name__ == '__main__':
    main()
