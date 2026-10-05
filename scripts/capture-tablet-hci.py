#!/usr/bin/env python3
"""Record a bounded, local, read-only BlueZ btmon capture of one controller.

This captures adapter-wide Bluetooth traffic, not just tablet traffic. It never
connects, pairs, scans, resets, configures or injects events. Output is private
and owned by the invoking sudo user. Keep Folio closed during diagnosis.
"""
import argparse
import json
import os
from pathlib import Path
import shutil
import signal
import subprocess
import time


def stop(process):
    if process.poll() is not None:
        return
    for sig, wait in ((signal.SIGINT, 5), (signal.SIGTERM, 3), (signal.SIGKILL, 3)):
        try:
            process.send_signal(sig)
            process.wait(timeout=wait)
            return
        except subprocess.TimeoutExpired:
            continue
        except ProcessLookupError:
            return


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--duration', type=float, default=60)
    parser.add_argument('--index', type=int, default=0)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    if not 1 <= args.duration <= 120 or args.index < 0:
        parser.error('Use duration 1–120 seconds and a nonnegative controller index')
    if os.geteuid() != 0:
        raise SystemExit('Linux restricts Bluetooth monitor access. Run with sudo; no permissions or settings are changed.')
    executable = shutil.which('btmon')
    if executable is None:
        raise SystemExit('The system BlueZ btmon executable is not installed')
    fd = os.open(args.output, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_CLOEXEC, 0o600)
    with os.fdopen(fd, 'w', buffering=1) as output:
        os.fchmod(output.fileno(), 0o600)
        if os.environ.get('SUDO_UID', '').isdigit() and os.environ.get('SUDO_GID', '').isdigit():
            os.fchown(output.fileno(), int(os.environ['SUDO_UID']), int(os.environ['SUDO_GID']))
        output.write('# ' + json.dumps({'started_at': time.time(), 'duration': args.duration,
                                       'controller': args.index, 'read_only': True,
                                       'scope': 'adapter-wide Bluetooth monitor'}) + '\n')
        print(f'Read-only Bluetooth monitor for {args.duration:g}s. Keep Folio closed. '
              'Captures adapter-wide traffic locally; no connection or configuration changes.', flush=True)
        process = subprocess.Popen([executable, '--index', str(args.index), '--date', '--no-pager',
                                    '--color', 'never', '--columns', '160'],
                                   stdin=subprocess.DEVNULL, stdout=output, stderr=subprocess.STDOUT)
        timed_out = False
        try:
            process.wait(timeout=args.duration)
        except subprocess.TimeoutExpired:
            timed_out = True
        except KeyboardInterrupt:
            pass
        finally:
            stop(process)
            output.write('# ' + json.dumps({'finished_at': time.time(), 'exit_code': process.returncode}) + '\n')
        if not timed_out and process.returncode not in (0, -signal.SIGINT):
            raise SystemExit(f'btmon exited early ({process.returncode}); inspect {args.output}')
    print('Saved:', args.output, flush=True)


if __name__ == '__main__':
    main()
