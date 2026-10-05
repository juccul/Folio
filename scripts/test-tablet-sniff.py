#!/usr/bin/env python3
"""Test the Intuos link with sniff mode temporarily disabled for that link only.

An explicit --disable-sniff is required. Does not pair, connect, scan, restart,
change adapter defaults, or install settings. Restores the original sniff bit
on exit if the same link remains connected; disconnect destroys link policy.
A private adapter-wide btmon trace accompanies the private JSONL results.
Uses installed BlueZ libbluetooth and btmon, with Python's standard library.
"""
import argparse
import contextlib
import ctypes
import errno
import fcntl
import json
import os
from pathlib import Path
import re
import shutil
import signal
import subprocess
import sys
import time

SNIFF = 0x0004
HCIGETCONNINFO = 0x800448d5  # Linux _IOR('H', 213, int); read-only connection lookup.
BT_CONNECTED = 1
MAX_CONNECTION_HANDLE = 0x0eff


class ConnectionInfo(ctypes.Structure):
    _fields_ = [('handle', ctypes.c_uint16), ('address', ctypes.c_ubyte * 6),
                ('type', ctypes.c_ubyte), ('out', ctypes.c_ubyte),
                ('state', ctypes.c_uint16), ('link_mode', ctypes.c_uint32)]


class ConnectionRequest(ctypes.Structure):
    _fields_ = [('address', ctypes.c_ubyte * 6), ('type', ctypes.c_ubyte),
                ('info', ConnectionInfo)]


def to_bluetooth(value):
    return int.from_bytes(value.to_bytes(2, 'little'), sys.byteorder)


def from_bluetooth(value):
    return int.from_bytes(value.to_bytes(2, sys.byteorder), 'little')


class Controller:
    def __init__(self, index, address):
        assert ctypes.sizeof(ConnectionInfo) == 16
        assert ctypes.sizeof(ConnectionRequest) == 24 and ConnectionRequest.info.offset == 8
        self.address = bytes.fromhex(address.replace(':', ''))[::-1]
        self.lib = ctypes.CDLL('libbluetooth.so.3', use_errno=True)
        self.lib.hci_open_dev.argtypes = [ctypes.c_int]
        self.lib.hci_open_dev.restype = ctypes.c_int
        self.lib.hci_close_dev.argtypes = [ctypes.c_int]
        self.lib.hci_close_dev.restype = ctypes.c_int
        self.lib.hci_read_link_policy.argtypes = [ctypes.c_int, ctypes.c_uint16,
                                                  ctypes.POINTER(ctypes.c_uint16), ctypes.c_int]
        self.lib.hci_read_link_policy.restype = ctypes.c_int
        self.lib.hci_write_link_policy.argtypes = [ctypes.c_int, ctypes.c_uint16,
                                                   ctypes.c_uint16, ctypes.c_int]
        self.lib.hci_write_link_policy.restype = ctypes.c_int
        self.fd = self.lib.hci_open_dev(index)
        self.check(self.fd)

    @staticmethod
    def check(result):
        if result < 0:
            error = ctypes.get_errno()
            raise OSError(error, os.strerror(error))

    def close(self):
        self.lib.hci_close_dev(self.fd)

    def connection(self):
        request = ConnectionRequest()
        request.address[:] = self.address
        request.type = 1  # ACL_LINK, not LE/SCO.
        buffer = bytearray(bytes(request))
        try:
            fcntl.ioctl(self.fd, HCIGETCONNINFO, buffer, True)
        except OSError as error:
            if error.errno in (errno.ENOENT, errno.ENOTCONN):
                return None
            raise
        info = ConnectionRequest.from_buffer_copy(buffer).info
        if bytes(info.address) != self.address or info.type != 1:
            raise RuntimeError('Connection lookup returned a different device; refusing to proceed')
        # HCIGETCONNINFO also exposes pending connections with kernel placeholder
        # handles (0x0f00 and above). They cannot be used in controller commands.
        # Wait for the complete connection, including its configuration phase.
        if info.state != BT_CONNECTED or info.handle > MAX_CONNECTION_HANDLE:
            return None
        return info.handle

    def require(self, handle):
        if not 0 <= handle <= MAX_CONNECTION_HANDLE:
            raise RuntimeError('Refusing a reserved or invalid Bluetooth connection handle')
        if self.connection() != handle:
            raise RuntimeError('The original tablet link no longer exists')

    def read_policy(self, handle):
        self.require(handle)
        value = ctypes.c_uint16()
        self.check(self.lib.hci_read_link_policy(self.fd, to_bluetooth(handle), ctypes.byref(value), 1500))
        return from_bluetooth(value.value)

    def write_policy(self, handle, value):
        self.require(handle)
        self.check(self.lib.hci_write_link_policy(self.fd, to_bluetooth(handle), to_bluetooth(value), 1500))


@contextlib.contextmanager
def without_sniff(controller, handle, emit, *, allow_already_disabled=False):
    original = controller.read_policy(handle)
    emit('original_policy', handle=handle, policy=original)
    if not original & SNIFF and not allow_already_disabled:
        raise RuntimeError('Sniff is already disabled on this link; this test cannot isolate a change')
    try:
        # Enter cleanup even if the controller applies a write but its reply times out.
        if original & SNIFF:
            controller.write_policy(handle, original & ~SNIFF)
        actual = controller.read_policy(handle)
        if actual != original & ~SNIFF:
            raise RuntimeError('Controller did not confirm the requested link policy')
        emit('sniff_disabled' if original & SNIFF else 'sniff_already_disabled', handle=handle, policy=actual)
        yield
    finally:
        # Never restore a former link's policy onto a different connection.
        if controller.connection() != handle:
            emit('restore_not_needed', reason='original connection no longer exists')
        else:
            current = controller.read_policy(handle)
            restored = (current & ~SNIFF) | (original & SNIFF)
            if restored != current:
                controller.write_policy(handle, restored)
            if controller.read_policy(handle) & SNIFF != original & SNIFF:
                raise RuntimeError('Failed to verify restoration of the original sniff setting')
            emit('sniff_restored', handle=handle, policy=restored)


def private_output(path):
    fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_CLOEXEC, 0o600)
    os.fchmod(fd, 0o600)
    if os.environ.get('SUDO_UID', '').isdigit() and os.environ.get('SUDO_GID', '').isdigit():
        os.fchown(fd, int(os.environ['SUDO_UID']), int(os.environ['SUDO_GID']))
    return os.fdopen(fd, 'w', buffering=1)


def stop_monitor(process):
    if process is None:
        return
    for sig, timeout in ((signal.SIGINT, 5), (signal.SIGTERM, 3), (signal.SIGKILL, 3)):
        if process.poll() is not None:
            return
        process.send_signal(sig)
        try:
            process.wait(timeout=timeout)
            return
        except subprocess.TimeoutExpired:
            continue


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--disable-sniff', action='store_true', required=True)
    parser.add_argument('--address', default='E0:9F:2A:1E:E1:41')
    parser.add_argument('--index', type=int, default=0)
    parser.add_argument('--duration', type=float, default=60)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    if not re.fullmatch(r'(?:[0-9a-fA-F]{2}:){5}[0-9a-fA-F]{2}', args.address):
        parser.error('Invalid Bluetooth address')
    if not 10 <= args.duration <= 120 or args.index < 0:
        parser.error('Use duration 10–120 seconds and a nonnegative controller index')
    if os.geteuid() != 0:
        raise SystemExit('Linux requires sudo for per-link Bluetooth policy access. No changes were made.')
    btmon = shutil.which('btmon')
    if btmon is None:
        raise SystemExit('The installed BlueZ btmon executable is required')
    controller = Controller(args.index, args.address)
    trace_path = args.output.with_suffix('.hci.txt')
    if trace_path == args.output:
        controller.close()
        parser.error('Output must have a different name from its companion .hci.txt trace')
    process = None
    try:
        if controller.connection() is not None:
            raise RuntimeError('Start with the tablet disconnected, so policy can be set on a fresh link before sniff entry')
        with private_output(args.output) as output, private_output(trace_path) as trace:
            def emit(kind, **values):
                output.write(json.dumps({'time': time.time(), 'kind': kind, **values}) + '\n')
            emit('test', address=args.address, duration=args.duration, change='disable sniff on one connection',
                 trace=str(trace_path), connection_guard='established state and valid controller handle')
            process = subprocess.Popen([btmon, '--index', str(args.index), '--date', '--no-pager',
                                        '--color', 'never', '--columns', '160'], stdin=subprocess.DEVNULL,
                                       stdout=trace, stderr=subprocess.STDOUT)
            try:
                print('Connect the tablet now, with Folio closed. Waiting up to 60 seconds.', flush=True)
                deadline = time.monotonic() + 60
                handle = None
                while handle is None:
                    if process.poll() is not None:
                        raise RuntimeError('btmon stopped before the test; inspect the companion trace')
                    handle = controller.connection()
                    if time.monotonic() >= deadline:
                        raise RuntimeError('No tablet connection arrived; no policy changes were made')
                    if handle is None:
                        time.sleep(.05)
                with without_sniff(controller, handle, emit):
                    print(f'Sniff disabled for this tablet link for {args.duration:g}s. '
                          'Draw, hover and lift normally; the setting will be restored afterward.', flush=True)
                    deadline = time.monotonic() + args.duration
                    while time.monotonic() < deadline:
                        if controller.connection() != handle:
                            emit('link_lost', handle=handle)
                            break
                        if process.poll() is not None:
                            raise RuntimeError('btmon stopped during the test')
                        time.sleep(.1)
                    else:
                        emit('link_survived', handle=handle, duration=args.duration)
            except BaseException as error:
                emit('error', message=str(error), type=type(error).__name__)
                raise
            finally:
                stop_monitor(process)
                emit('finished', monitor_exit_code=process.returncode)
        print('Saved:', args.output, 'and', trace_path, flush=True)
    finally:
        stop_monitor(process)
        controller.close()


if __name__ == '__main__':
    def interrupted(signum, frame):
        raise KeyboardInterrupt(f'Signal {signum}')
    for sig in (signal.SIGINT, signal.SIGTERM, signal.SIGHUP):
        signal.signal(sig, interrupted)
    try:
        main()
    except FileExistsError as error:
        raise SystemExit(f'Capture already exists: {error.filename}. Use a new output name to preserve earlier evidence.')
    except (OSError, RuntimeError, KeyboardInterrupt) as error:
        raise SystemExit(str(error) or 'Interrupted; inspect the output for restoration status')
