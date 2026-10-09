"""Verify compact Windows payloads preserve imports, metadata and notices."""
import importlib.util
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch
import zipfile

spec = importlib.util.spec_from_file_location('folio_windows_package', Path(__file__).with_name('package-windows.py'))
packaging = importlib.util.module_from_spec(spec)
spec.loader.exec_module(packaging)


class CompactWindowsPayloadTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)

    def wheel(self, name, members):
        archive = self.root / name
        with zipfile.ZipFile(archive, 'w') as target:
            for path, contents in members:
                target.writestr(path, contents)
        return archive

    def test_zipimport_keeps_package_resources_and_distribution_metadata(self):
        members = [('compact_demo/__init__.py', b'answer = 42\n'),
                   ('compact_demo/data.txt', b'full resource\n'),
                   ('compact_demo-1.0.dist-info/METADATA', b'Metadata-Version: 2.1\nName: compact-demo\nVersion: 1.0\n'),
                   ('compact_demo-1.0.dist-info/LICENSE', b'original distribution license\n')]
        archive = self.wheel('demo.whl', members)
        payload = self.root / 'math-solver/site-packages.zip'
        packaging.stage_python_packages([archive], payload)
        with zipfile.ZipFile(payload) as result:
            self.assertEqual(set(result.namelist()), {p for p, _ in members})
            for name, contents in members:
                self.assertEqual(result.read(name), contents)
        script = """
import importlib.metadata, importlib.resources, sys
sys.path.insert(0, sys.argv[1])
import compact_demo
assert compact_demo.answer == 42
assert importlib.resources.files(compact_demo).joinpath('data.txt').read_bytes() == b'full resource\\n'
assert importlib.metadata.version('compact-demo') == '1.0'
"""
        subprocess.run([sys.executable, '-I', '-S', '-c', script, str(payload)], check=True)

    def test_rejects_unsafe_duplicate_and_native_members(self):
        bad_members = [('../outside.py', b''), ('/outside.py', b''),
                       ('C:/outside.py', b''),
                       ('module.pyd', b'')]
        link = zipfile.ZipInfo('linked.py')
        link.external_attr = 0o120777 << 16
        bad_members.append((link, b'target.py'))
        for index, member in enumerate(bad_members):
            with self.subTest(member=member[0]):
                wheel = self.wheel(f'bad-{index}.whl', [member])
                with self.assertRaises(ValueError):
                    packaging.stage_python_packages([wheel], self.root / f'bad-{index}.zip')
        for index, duplicate in enumerate(('module.py', 'MODULE.py', './module.py')):
            with self.subTest(duplicate=duplicate):
                first = self.wheel(f'first-{index}.whl', [('module.py', b'first')])
                second = self.wheel(f'second-{index}.whl', [(duplicate, b'second')])
                with self.assertRaisesRegex(ValueError, 'Duplicate'):
                    packaging.stage_python_packages([first, second], self.root / f'duplicate-{index}.zip')

    def test_rejects_raw_names_before_windows_zip_normalization(self):
        for index, (safe, unsafe) in enumerate([
            ('folder/outside.py', 'folder\\outside.py'),
            ('module.pyXoutside.py', 'module.py\0outside.py'),
        ]):
            with self.subTest(name=unsafe):
                # Writing a ZIP normally sanitizes separators on Windows.
                # Patch both equal-length headers to model an actual raw input.
                archive = self.wheel(f'raw-{index}.whl', [(safe, b'content')])
                archive.write_bytes(archive.read_bytes().replace(safe.encode(), unsafe.encode()))
                with self.assertRaisesRegex(ValueError, 'Unsafe archive path'):
                    packaging.stage_python_packages([archive], self.root / f'raw-{index}.zip')

    def test_notices_keep_exact_non_utf8_bytes_and_working_inventory_links(self):
        source = self.root / 'source'
        notices = source / 'third_party/licenses/demo-1.0'
        notices.mkdir(parents=True)
        raw = b'Copyright example\r\nLicense bytes: \xff\n'
        (notices / 'LICENSE').write_bytes(raw)
        destination = self.root / 'package'
        destination.mkdir()
        (destination / 'LICENSES.md').write_text('[notices](third_party/licenses/demo-1.0/)\n')
        with patch.object(packaging, 'ROOT', source):
            packaging.stage_notices(destination)
        result = destination / 'third_party/licenses'
        with zipfile.ZipFile(result / 'licenses.zip') as original:
            self.assertEqual(original.read('demo-1.0/LICENSE'), raw)
        self.assertIn('demo-1.0/LICENSE', (result / 'INDEX.md').read_text())
        self.assertIn('SHA-256 ' + packaging.sha256(notices / 'LICENSE'),
                      (result / 'THIRD_PARTY_NOTICES.txt').read_text())
        inventory = (destination / 'LICENSES.md').read_text()
        self.assertNotIn('](third_party/licenses/demo-1.0/)', inventory)
        self.assertIn('](third_party/licenses/INDEX.md)', inventory)


if __name__ == '__main__':
    unittest.main()
