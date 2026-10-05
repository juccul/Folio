#!/usr/bin/env python3
"""Temporary Intuos-only Bluetooth compatibility helper; requires sudo.

Keeps sniff disabled on this tablet's connections while running, then restores
the original sniff setting on the current link. Does not connect, scan, reset,
pair, change adapter defaults or install a service. Logs policy metadata only.
One successful 60-second trial supports this workaround; longer use is unverified.
"""
import argparse
import contextlib
import fcntl
import importlib.util
import json
import os
from pathlib import Path
import signal
import stat
import time

spec = importlib.util.spec_from_file_location('tablet_sniff', Path(__file__).with_name('test-tablet-sniff.py'))
tablet = importlib.util.module_from_spec(spec)
spec.loader.exec_module(tablet)
ADDRESS = 'E0:9F:2A:1E:E1:41'


@contextlib.contextmanager
def single_instance(index):
    path = f'/run/lock/folio-intuos-{index}-{ADDRESS.replace(":", "")}.lock'
    fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_CLOEXEC | os.O_NOFOLLOW, 0o600)
    try:
        info = os.fstat(fd)
        if info.st_uid != 0 or not stat.S_ISREG(info.st_mode):
            raise RuntimeError('The helper lock is not a root-owned regular file')
        try:
            fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError:
            raise RuntimeError('A compatibility helper is already running for this tablet') from None
        yield
    finally:
        os.close(fd)


def guard_link(controller, handle, deadline, emit):
    with tablet.without_sniff(controller, handle, emit, allow_already_disabled=True):
        print('Intuos compatibility active. Ctrl+C stops the helper and restores the sniff setting.', flush=True)
        next_check = time.monotonic() + 2
        while time.monotonic() < deadline:
            if controller.connection() != handle:
                emit('link_ended', handle=handle)
                return
            if time.monotonic() >= next_check:
                current = controller.read_policy(handle)
                if current & tablet.SNIFF:
                    controller.write_policy(handle, current & ~tablet.SNIFF)
                    if controller.read_policy(handle) & tablet.SNIFF:
                        raise RuntimeError('Controller did not retain the compatibility setting')
                    emit('sniff_disabled_again', handle=handle)
                next_check = time.monotonic() + 2
            time.sleep(.25)


def run(controller, duration, emit):
    deadline = time.monotonic() + duration
    while time.monotonic() < deadline:
        handle = controller.connection()
        if handle is None:
            time.sleep(.05)
            continue
        guard_link(controller, handle, deadline, emit)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--index', type=int, default=0)
    parser.add_argument('--duration', type=float, default=3600)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    if args.index < 0 or not 60 <= args.duration <= 28800:
        parser.error('Use a nonnegative controller index and duration 60–28800 seconds')
    if os.geteuid() != 0:
        raise SystemExit('Linux requires sudo for this temporary per-tablet link policy. No changes were made.')
    with single_instance(args.index):
        controller = tablet.Controller(args.index, ADDRESS)
        try:
            if controller.connection() is not None:
                raise RuntimeError('Start with the tablet disconnected so a fresh link can be configured before sniff entry')
            with tablet.private_output(args.output) as output:
                def emit(kind, **values):
                    output.write(json.dumps({'time': time.time(), 'kind': kind, **values}) + '\n')
                emit('helper_started', address=ADDRESS, duration=args.duration, scope='per-connection sniff bit only')
                print(f'Intuos helper waiting for your connection, for up to {args.duration:g}s. '
                      'No automatic connection or Bluetooth reset. Keep this terminal open.', flush=True)
                try:
                    run(controller, args.duration, emit)
                except KeyboardInterrupt:
                    emit('helper_interrupted')
                except BaseException as error:
                    emit('error', message=str(error), type=type(error).__name__)
                    raise
                finally:
                    emit('helper_finished')
        finally:
            controller.close()
    print('Helper stopped. Inspect the log for the per-link restoration result.', flush=True)


if __name__ == '__main__':
    def interrupted(signum, frame):
        raise KeyboardInterrupt(f'Signal {signum}')
    for sig in (signal.SIGINT, signal.SIGTERM, signal.SIGHUP):
        signal.signal(sig, interrupted)
    try:
        main()
    except FileExistsError as error:
        raise SystemExit(f'Log already exists: {error.filename}. Use a new filename.')
    except (OSError, RuntimeError, KeyboardInterrupt) as error:
        raise SystemExit(str(error) or 'Interrupted; inspect the log for restoration status')
