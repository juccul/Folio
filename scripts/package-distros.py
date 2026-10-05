#!/usr/bin/env python3
"""Create local Debian and RPM packages; no installation or publishing."""
import argparse,os,re,shutil,subprocess,tarfile,tempfile,tomllib
from pathlib import Path
ROOT=Path(__file__).resolve().parents[1]
def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--binary',type=Path,default=ROOT/'target/release/folio');p.add_argument('--output',type=Path,default=ROOT/'artifacts/dist');a=p.parse_args();a.output.mkdir(parents=True,exist_ok=True)
    symbols=subprocess.check_output(['readelf','--version-info',str(a.binary)],text=True)
    glibc=max(set(re.findall(r'GLIBC_([0-9.]+)',symbols)),key=lambda v:tuple(int(n) for n in v.split('.')))
    version=tomllib.loads((ROOT/'Cargo.toml').read_text())['workspace']['package']['version']
    with tempfile.TemporaryDirectory(prefix='folio-package-') as t:
        temp=Path(t);stage=temp/'stage'
        files={a.binary:'usr/bin/folio',ROOT/'packaging/io.github.folio.Notes.desktop':'usr/share/applications/io.github.folio.Notes.desktop',ROOT/'packaging/io.github.folio.Notes.svg':'usr/share/icons/hicolor/scalable/apps/io.github.folio.Notes.svg',ROOT/'packaging/io.github.folio.Notes.metainfo.xml':'usr/share/metainfo/io.github.folio.Notes.metainfo.xml',ROOT/'LICENSE':'usr/share/doc/folio/LICENSE',ROOT/'LICENSES.md':'usr/share/doc/folio/LICENSES.md'}
        for source,name in files.items():dest=stage/name;dest.parent.mkdir(parents=True,exist_ok=True);shutil.copy2(source,dest)
        shutil.copytree(ROOT/'third_party/licenses',stage/'usr/share/doc/folio/third_party')
        data=temp/'data.tar.gz'
        with tarfile.open(data,'w:gz') as tar:
            for name in stage.iterdir():tar.add(name,arcname='./'+name.name)
        control=temp/'control';control.mkdir();(control/'control').write_text(f'Package: folio\nVersion: {version}\nArchitecture: amd64\nMaintainer: Folio contributors\nSection: editors\nPriority: optional\nDepends: libc6 (>= {glibc}), libx11-6, libxcb1, libxkbcommon0, libxkbcommon-x11-0, libxcb-xkb1, libwayland-client0, libvulkan1\nRecommends: poppler-utils\nDescription: Offline vector handwriting and notes\n Pressure-sensitive Linux notes with original ink, PDF annotations and editable text.\n')
        with tarfile.open(temp/'control.tar.gz','w:gz') as tar:tar.add(control/'control',arcname='./control')
        (temp/'debian-binary').write_text('2.0\n');deb=a.output/f'folio_{version}_amd64.deb';subprocess.run(['ar','rc',str(deb.resolve()),'debian-binary','control.tar.gz','data.tar.gz'],cwd=temp,check=True)
        if shutil.which('rpmbuild'):
            rpm=temp/'rpm';(rpm/'SOURCES').mkdir(parents=True);shutil.copy(data,rpm/'SOURCES/folio-files.tar.gz')
            spec=temp/'folio.spec';spec.write_text(f'''Name: folio
Version: {version}
Release: 1
Summary: Offline vector handwriting and notes
License: GPL-3.0-or-later
Source0: folio-files.tar.gz
BuildArch: x86_64
Requires: libwayland-client.so.0()(64bit), libvulkan.so.1()(64bit), libxcb-xkb.so.1()(64bit)
Recommends: poppler-utils
%description
Pressure-sensitive native Linux notes and PDF annotations with editable vector ink.
%prep
%build
%install
mkdir -p %{{buildroot}}
tar -xzf %{{SOURCE0}} -C %{{buildroot}}
%files
/usr/bin/folio
/usr/share/applications/io.github.folio.Notes.desktop
/usr/share/icons/hicolor/scalable/apps/io.github.folio.Notes.svg
/usr/share/metainfo/io.github.folio.Notes.metainfo.xml
/usr/share/doc/folio
''')
            subprocess.run(['rpmbuild','-bb','--define',f'_topdir {rpm}','--define','__os_install_post %{nil}',str(spec)],check=True)
            for file in (rpm/'RPMS').rglob('*.rpm'):shutil.copy2(file,a.output/file.name)
        print(deb)
if __name__=='__main__':main()
