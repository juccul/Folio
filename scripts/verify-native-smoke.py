#!/usr/bin/env python3
"""Run native pen/toolbar smoke replay on an explicitly isolated X11 display.
Example inside the test container:
FOLIO_VIRTUAL_DISPLAY=1 DISPLAY=:95 WAYLAND_DISPLAY= dbus-run-session -- \
  python3 scripts/verify-native-smoke.py --binary artifacts/debian-target/release/folio
No physical input devices or global desktop settings are accessed.
"""
import argparse
import os
from pathlib import Path
import subprocess
import tempfile
from ui_x11 import Client


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--timeout', type=float, default=90,
                        help='Allow software rendering on slow virtual displays (seconds)')
    args = parser.parse_args()
    if os.environ.get('FOLIO_VIRTUAL_DISPLAY') != '1' or os.environ.get('WAYLAND_DISPLAY'):
        parser.error('Use only an isolated X11 test display with FOLIO_VIRTUAL_DISPLAY=1')
    with tempfile.TemporaryDirectory(prefix='folio-native-smoke-') as root:
        app = subprocess.Popen([str(args.binary.resolve()), '--data-dir', root, '--smoke-test'])
        client = None
        try:
            client = Client(app.pid)
            # Bare Xvfb needs the mapped-window notification replayed after
            # Vulkan startup; this targets only the exact child application's PID.
            for _ in range(5):
                if app.poll() is not None:
                    break
                client.wake_virtual_window()
            return app.wait(timeout=args.timeout)
        finally:
            if client is not None:
                client.close()
            if app.poll() is None:
                app.terminate()
                try:
                    app.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    app.kill()
                    app.wait()


if __name__ == '__main__':
    raise SystemExit(main())
