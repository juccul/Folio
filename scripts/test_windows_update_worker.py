"""Recovery tests use disposable payloads and libraries; never the installed app."""
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
import zipfile

spec = importlib.util.spec_from_file_location('folio_update_worker', Path(__file__).with_name('windows-update-worker.py'))
worker = importlib.util.module_from_spec(spec)
spec.loader.exec_module(worker)


class Processes:
    def reserve(self, root):
        pass

    def parent(self, pid):
        return pid

    def wait_parent(self, handle):
        pass

    def other_instances(self, root, parent_pid=None):
        return []


class UpdateWorkerTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.base = Path(self.temp.name)
        self.root = self.base / 'Folio α custom installation'
        self.data = self.base / 'library 日本語'
        self.data.mkdir()
        (self.data / 'notes.sqlite3').write_bytes(b'notes must survive')
        self.job = self.base / 'job-test'; self.job.mkdir()
        self.payload(self.root, '0.1.2', b'old executable')
        self.new = self.base / 'new'
        self.payload(self.new, '0.1.3', b'new executable')
        self.package = self.base / 'update.zip'
        with zipfile.ZipFile(self.package, 'w') as archive:
            for file in self.new.rglob('*'):
                if file.is_file():
                    archive.write(file, f'folio-0.1.3-windows-x64/{file.relative_to(self.new).as_posix()}')
        self.intent = {'root': str(self.root), 'data': str(self.data), 'package': str(self.package), 'parent_pid': 123,
                       'version': '0.1.3', 'binary_sha256': worker.digest(self.new / 'bin/folio.exe'), 'sha256': worker.digest(self.package), 'size': self.package.stat().st_size, 'portable': True}

    def payload(self, root, version, binary):
        for name in ['bin/folio.exe', 'python/python.exe', 'python/pythonw.exe', 'pdf/bin/pdftoppm.exe', 'math-solver/pack.json']:
            file = root / name; file.parent.mkdir(parents=True, exist_ok=True)
            file.write_bytes(binary if name == 'bin/folio.exe' else b'runtime')
        (root / 'manifest.json').write_text(json.dumps({'version': version, 'binary_sha256': worker.digest(root / 'bin/folio.exe')}))

    def assert_notes(self):
        self.assertEqual((self.data / 'notes.sqlite3').read_bytes(), b'notes must survive')

    def test_portable_swap_verifies_payload_keeps_old_version_and_relaunches_same_library(self):
        with patch.object(worker, 'relaunch') as launch:
            worker.install(self.intent, self.job, Processes())
        launch.assert_called_once_with(self.root, self.data)
        self.assertEqual((self.root / 'bin/folio.exe').read_bytes(), b'new executable')
        previous = self.base / f'.{self.root.name}-previous-{self.job.name}'
        self.assertEqual((previous / 'bin/folio.exe').read_bytes(), b'old executable')
        self.assert_notes()

    def test_failed_installer_restores_previous_payload_and_never_touches_notes(self):
        self.intent['portable'] = False
        def fail(arguments, **kwargs):
            self.assertIn(f'/DIR={self.root}', arguments)
            self.assertIn('/NOCLOSEAPPLICATIONS', arguments)
            self.assertIn('/NOFORCECLOSEAPPLICATIONS', arguments)
            (self.root / 'bin/folio.exe').write_bytes(b'partial broken installation')
            return type('Result', (), {'returncode': 5})()
        with patch.object(worker.subprocess, 'run', side_effect=fail), patch.object(worker, 'relaunch') as launch:
            with self.assertRaisesRegex(RuntimeError, 'exit code 5'):
                worker.install(self.intent, self.job, Processes())
        launch.assert_not_called()
        self.assertEqual((self.root / 'bin/folio.exe').read_bytes(), b'old executable')
        self.assert_notes()

    def test_failed_backup_does_not_restore_a_partial_backup_over_the_working_app(self):
        self.intent['portable'] = False
        def fail(source, destination):
            destination.mkdir(); (destination / 'partial').write_text('incomplete')
            raise OSError('disk full')
        with patch.object(worker.shutil, 'copytree', side_effect=fail):
            with self.assertRaisesRegex(OSError, 'disk full'):
                worker.install(self.intent, self.job, Processes())
        self.assertEqual((self.root / 'bin/folio.exe').read_bytes(), b'old executable')
        self.assert_notes()

    def test_checksum_tampering_cannot_close_or_replace_the_app(self):
        self.package.write_bytes(b'corrupt')
        with self.assertRaisesRegex(ValueError, 'changed'):
            worker.install(self.intent, self.job, Processes())
        self.assertFalse((self.job / 'ready').exists())
        self.assertEqual((self.root / 'bin/folio.exe').read_bytes(), b'old executable')
        self.assert_notes()

    def test_library_inside_installation_is_rejected_before_handshake(self):
        self.intent['data'] = str(self.root / 'my notes')
        with self.assertRaisesRegex(ValueError, 'outside'):
            worker.install(self.intent, self.job, Processes())
        self.assertFalse((self.job / 'ready').exists())

    def test_unsafe_archives_cannot_escape_staging(self):
        for path in ['folio-0.1.3-windows-x64/../escaped', '/absolute', 'folio-0.1.3-windows-x64/bin/CON', 'folio-0.1.3-windows-x64/bin/file:stream']:
            with self.subTest(path=path):
                archive = self.base / 'bad.zip'
                with zipfile.ZipFile(archive, 'w') as output:
                    output.writestr(path, 'bad')
                with self.assertRaises(ValueError):
                    worker.extract_portable(archive, self.base / 'stage', self.intent)
        self.assertFalse((self.base / 'escaped').exists())

    def test_new_payload_with_wrong_binary_is_rejected_before_handshake(self):
        self.intent['binary_sha256'] = '0' * 64
        with self.assertRaisesRegex(ValueError, 'signed release'):
            worker.install(self.intent, self.job, Processes())
        self.assertFalse((self.job / 'ready').exists())
        self.assertEqual((self.root / 'bin/folio.exe').read_bytes(), b'old executable')


if __name__ == '__main__':
    unittest.main()
