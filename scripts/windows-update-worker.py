#!/usr/bin/env python3
"""Private staged Windows update helper; no network and no forced process termination.

The app verifies the signed manifest, saves every document and backs up its
library before invoking this helper. A readiness handshake is required before
it exits. Failure restores the previous payload and relaunches the same library.
"""
import ctypes
from ctypes import wintypes
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import shutil
import subprocess
import sys
import time
import zipfile


def resolved(path):
    value = str(Path(path).resolve())
    # Rust canonical paths use the extended Windows prefix. Normalize all
    # comparisons consistently and pass a regular DOS/UNC path to Inno Setup.
    if os.name == 'nt' and value.startswith('\\\\?\\'):
        value = ('\\\\' + value[8:]) if value.startswith('\\\\?\\UNC\\') else value[4:]
    return Path(value)


class Progress:
    """Report completed work, never a timer heartbeat that hides a stuck task."""
    def __init__(self, job):
        self.job = Path(job)
        self.sequence = 0
        self.last_write = float('-inf')
        self.phase = None

    def __call__(self, phase):
        now = time.monotonic()
        if phase == self.phase and now - self.last_write < 1:
            return
        sequence = self.sequence + 1
        temporary = self.job / 'progress.tmp'
        try:
            temporary.write_text(json.dumps({'protocol': 1, 'sequence': sequence, 'phase': phase}), encoding='utf-8')
            os.replace(temporary, self.job / 'progress.json')
        except OSError:
            # A transient sharing violation must not discard a valid payload.
            # If reports remain unavailable, the app's inactivity limit stops us.
            return
        self.sequence = sequence
        self.last_write = now
        self.phase = phase


def report(progress, phase):
    if progress is not None:
        progress(phase)


def digest(path, progress=None, phase='Verify executable'):
    with Path(path).open('rb') as source:
        if progress is None:
            return hashlib.file_digest(source, 'sha256').hexdigest()
        checksum = hashlib.sha256()
        while chunk := source.read(2 * 1024**2):
            checksum.update(chunk)
            report(progress, phase)
        return checksum.hexdigest()


def verify_package(intent, progress=None):
    package = Path(intent['package'])
    if package.stat().st_size != intent['size'] or digest(package, progress, 'Verify package') != intent['sha256']:
        raise ValueError('The downloaded update changed; reopen Folio and download it again')


def validate_payload(root, intent, progress=None):
    manifest = json.loads((root / 'manifest.json').read_text(encoding='utf-8'))
    if manifest['version'] != intent['version'] or manifest['binary_sha256'] != intent['binary_sha256']:
        raise ValueError('The installed version does not match the signed release')
    if digest(root / 'bin/folio.exe', progress) != intent['binary_sha256']:
        raise ValueError('The installed executable did not pass verification')
    required = ['python/python.exe', 'python/pythonw.exe', 'math-solver/pack.json', 'pdf/bin/pdftoppm.exe']
    for name in required:
        if not (root / name).is_file():
            raise ValueError('The updated runtime is incomplete')
        report(progress, 'Verify runtime')
    for name, expected in manifest.get('app_runtime_dlls', {}).items():
        if Path(name).name != name or digest(root / 'bin' / name, progress, 'Verify runtime') != expected:
            raise ValueError('The updated application runtime did not pass verification')


def extract_portable(package, destination, intent, progress=None):
    prefix = f"folio-{intent['version']}-windows-x64/"
    with zipfile.ZipFile(package) as archive:
        total = 0
        if len(archive.infolist()) > 100000:
            raise ValueError('Too many update archive entries')
        seen = set()
        for entry in archive.infolist():
            name = entry.filename
            path = PurePosixPath(name)
            if not name.startswith(prefix) or path.is_absolute() or '..' in path.parts or '\\' in name or ':' in name:
                raise ValueError('Unsafe update archive path')
            relative = name[len(prefix):]
            if not relative:
                continue
            parts = PurePosixPath(relative).parts
            if any(part.rstrip(' .') != part or part.split('.')[0].upper() in {'CON', 'PRN', 'AUX', 'NUL', *[f'COM{i}' for i in range(1, 10)], *[f'LPT{i}' for i in range(1, 10)]} for part in parts):
                raise ValueError('Unsafe Windows archive filename')
            if (entry.external_attr >> 16) & 0o170000 == 0o120000:
                raise ValueError('Update archives cannot contain symlinks')
            identity = relative.casefold().rstrip('/')
            if identity in seen:
                raise ValueError('Duplicate update archive entry')
            seen.add(identity)
            total += entry.file_size
            if total > 5 * 1024**3:
                raise ValueError('Update archive is too large')
            target = destination.joinpath(*parts)
            if entry.is_dir():
                target.mkdir(parents=True, exist_ok=True)
            else:
                target.parent.mkdir(parents=True, exist_ok=True)
                with archive.open(entry) as source, target.open('xb') as output:
                    while chunk := source.read(2 * 1024**2):
                        output.write(chunk)
                        report(progress, 'Extract portable payload')
                    output.flush()
                    os.fsync(output.fileno())
            report(progress, 'Extract portable payload')
    validate_payload(destination, intent, progress)


class Windows:
    def __init__(self):
        self.kernel = ctypes.WinDLL('kernel32', use_last_error=True)
        k = self.kernel
        k.OpenProcess.argtypes = [wintypes.DWORD, wintypes.BOOL, wintypes.DWORD]
        k.OpenProcess.restype = wintypes.HANDLE
        k.CloseHandle.argtypes = [wintypes.HANDLE]
        k.CreateMutexW.argtypes = [ctypes.c_void_p, wintypes.BOOL, wintypes.LPCWSTR]
        k.CreateMutexW.restype = wintypes.HANDLE
        k.WaitForSingleObject.argtypes = [wintypes.HANDLE, wintypes.DWORD]
        k.WaitForSingleObject.restype = wintypes.DWORD
        k.QueryFullProcessImageNameW.argtypes = [wintypes.HANDLE, wintypes.DWORD, wintypes.LPWSTR, ctypes.POINTER(wintypes.DWORD)]
        k.QueryFullProcessImageNameW.restype = wintypes.BOOL
        k.CreateToolhelp32Snapshot.argtypes = [wintypes.DWORD, wintypes.DWORD]
        k.CreateToolhelp32Snapshot.restype = wintypes.HANDLE
        class Process(ctypes.Structure):
            _fields_ = [('dwSize', wintypes.DWORD), ('cntUsage', wintypes.DWORD), ('th32ProcessID', wintypes.DWORD), ('th32DefaultHeapID', ctypes.c_size_t), ('th32ModuleID', wintypes.DWORD), ('cntThreads', wintypes.DWORD), ('th32ParentProcessID', wintypes.DWORD), ('pcPriClassBase', wintypes.LONG), ('dwFlags', wintypes.DWORD), ('szExeFile', wintypes.WCHAR * 260)]
        self.Process = Process
        for name in ['Process32FirstW', 'Process32NextW']:
            method = getattr(k, name)
            method.argtypes = [wintypes.HANDLE, ctypes.POINTER(Process)]
            method.restype = wintypes.BOOL

    def reserve(self, root):
        name = 'Local\\FolioUpdate-' + hashlib.sha256(str(root).casefold().encode('utf-8')).hexdigest()
        ctypes.set_last_error(0)
        handle = self.kernel.CreateMutexW(None, False, name)
        error = ctypes.get_last_error()
        if not handle:
            raise ctypes.WinError(error)
        if error == 183:
            self.kernel.CloseHandle(handle)
            raise RuntimeError('Another update is already preparing this installation')
        self.reservation = handle  # Held until this private helper exits.

    def parent(self, pid):
        handle = self.kernel.OpenProcess(0x00100000, False, pid)  # SYNCHRONIZE
        if not handle:
            raise ctypes.WinError(ctypes.get_last_error())
        return handle

    def wait_parent(self, handle):
        try:
            if self.kernel.WaitForSingleObject(handle, 120000) != 0:
                raise RuntimeError('Folio did not close; the installation was left unchanged')
        finally:
            self.kernel.CloseHandle(handle)

    def other_instances(self, root, parent_pid=None):
        snapshot = self.kernel.CreateToolhelp32Snapshot(2, 0)
        if snapshot == ctypes.c_void_p(-1).value:
            raise ctypes.WinError(ctypes.get_last_error())
        processes = []
        try:
            item = self.Process(); item.dwSize = ctypes.sizeof(item)
            valid = self.kernel.Process32FirstW(snapshot, ctypes.byref(item))
            while valid:
                if item.szExeFile.casefold() in ({'folio.exe'} if parent_pid is not None else {'folio.exe', 'python.exe', 'pythonw.exe', 'pdftoppm.exe'}) and item.th32ProcessID not in {os.getpid(), parent_pid}:
                    handle = self.kernel.OpenProcess(0x1000, False, item.th32ProcessID)
                    if not handle:
                        # Cannot prove a same-named process is safe to ignore.
                        if ctypes.get_last_error() != 87:  # ERROR_INVALID_PARAMETER: process exited
                            raise RuntimeError('Could not inspect another Folio process; close other Folio windows and retry')
                    else:
                        try:
                            size = wintypes.DWORD(32768); path = ctypes.create_unicode_buffer(size.value)
                            if not self.kernel.QueryFullProcessImageNameW(handle, 0, path, ctypes.byref(size)):
                                raise ctypes.WinError(ctypes.get_last_error())
                            if Path(path.value).resolve().is_relative_to(root):
                                processes.append(item.th32ProcessID)
                        finally:
                            self.kernel.CloseHandle(handle)
                valid = self.kernel.Process32NextW(snapshot, ctypes.byref(item))
        finally:
            self.kernel.CloseHandle(snapshot)
        return processes

    def message(self, text):
        ctypes.windll.user32.MessageBoxW(None, text, 'Folio update', 0x10)


def relaunch(root, data):
    subprocess.Popen([str(root / 'bin/folio.exe'), '--data-dir', str(data), '--restart-after-update'], close_fds=True, creationflags=0x00000008)


def install(intent, job, windows):
    root = resolved(intent['root'])
    data = resolved(intent['data'])
    package = resolved(intent['package'])
    if data.is_relative_to(root) or resolved(job).is_relative_to(root):
        raise ValueError('The library and updater must be outside the installation directory')
    progress = Progress(job)
    progress('Check update paths')
    verify_package(intent, progress)
    for entry in root.rglob('*'):
        if entry.is_symlink() or (hasattr(entry, 'is_junction') and entry.is_junction()):
            raise ValueError('The installation contains linked directories. Use the manual installer after moving these links outside Folio')
        progress('Inspect current installation')
    windows.reserve(root)
    if windows.other_instances(root, parent_pid=intent['parent_pid']):
        raise RuntimeError('Close the other Folio windows using this installation, then click Restart to update again')
    parent = windows.parent(intent['parent_pid'])
    # Preflight write access and enough room before asking the app to exit.
    probe = root.parent / f'.folio-write-{job.name}'
    probe.mkdir(); probe.rmdir()
    payload_bytes = 0
    for file in root.rglob('*'):
        if file.is_file():
            payload_bytes += file.stat().st_size
        progress('Check available space')
    if shutil.disk_usage(root.parent).free < payload_bytes * 3 + intent['size']:
        raise RuntimeError('There is not enough disk space to safely update and keep the previous version')
    previous = root.parent / f'.{root.name}-previous-{job.name}'
    incoming = root.parent / f'.{root.name}-incoming-{job.name}'
    if intent['portable']:
        incoming.mkdir()
        extract_portable(package, incoming, intent, progress)
    (job / 'ready').write_text('ready', encoding='utf-8')
    windows.wait_parent(parent)
    (job / 'parent-exited').write_text('closed', encoding='utf-8')
    deadline = time.monotonic() + 120
    while windows.other_instances(root):
        if time.monotonic() >= deadline:
            raise RuntimeError('Another Folio window is still open. Close it and retry; no files were replaced')
        time.sleep(0.5)
    applied = False
    rollback_ready = False
    try:
        if intent['portable']:
            root.rename(previous)
            rollback_ready = True
            incoming.rename(root)
        else:
            shutil.copytree(root, previous)
            rollback_ready = True
            result = subprocess.run([str(package), '/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART', '/NOCLOSEAPPLICATIONS', '/NOFORCECLOSEAPPLICATIONS', '/NORESTARTAPPLICATIONS', '/SP-', f'/DIR={root}', f'/LOG={job / "installer.log"}'], check=False)
            if result.returncode != 0:
                raise RuntimeError(f'The installer failed with exit code {result.returncode}; the previous version will be restored')
        applied = True
        validate_payload(root, intent)
        (job / 'result.json').write_text(json.dumps({'status': 'installed', 'version': intent['version'], 'previous': str(previous), 'library': str(data)}, indent=2), encoding='utf-8')
    except Exception:
        if rollback_ready and previous.exists():
            failed = root.parent / f'.{root.name}-failed-{job.name}'
            if root.exists():
                root.rename(failed)
            previous.rename(root)
            if failed.exists():
                shutil.rmtree(failed, ignore_errors=True)
        if not rollback_ready and previous.exists():
            shutil.rmtree(previous, ignore_errors=True)
        raise
    finally:
        if incoming.exists():
            shutil.rmtree(incoming, ignore_errors=True)
    # Keep the previous payload for recovery rather than deleting it before a
    # successful first launch. This is intentional and recorded in result.json.
    if applied:
        relaunch(root, data)


def main():
    intent_file = Path(sys.argv[1]).resolve()
    job = intent_file.parent
    intent = json.loads(intent_file.read_text(encoding='utf-8'))
    windows = Windows()
    try:
        install(intent, job, windows)
    except Exception as error:
        message = f'{error}\n\nYour library was not removed. Update diagnostics: {job}'
        (job / 'error.txt').write_text(message, encoding='utf-8')
        (job / 'result.json').write_text(json.dumps({'status': 'failed', 'error': str(error)}, indent=2), encoding='utf-8')
        if (job / 'parent-exited').exists() and (Path(intent['root']) / 'bin/folio.exe').exists():
            relaunch(Path(intent['root']), Path(intent['data']))
        windows.message(message)
        raise SystemExit(1)


if __name__ == '__main__':
    main()
