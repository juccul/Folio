#!/usr/bin/env python3
"""Replay real mouse Circle-to-select gestures on a private X11/D-Bus desktop.

Requires Python GI/AT-SPI, Xvfb, libXtst and a Folio native binary. No physical
input devices, desktop preferences or existing libraries are accessed.
"""
import argparse
import ctypes as C
import json
import math
import os
from pathlib import Path
import shutil
import sqlite3
import subprocess
import sys
import tempfile
import time

ROOT = Path(__file__).resolve().parents[1]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, default=ROOT / 'target/debug/folio')
    parser.add_argument('--output', type=Path, default=ROOT / 'artifacts/circle-selection')
    parser.add_argument('--xvfb', type=Path, help='Path to Xvfb; otherwise use PATH or the local build tool')
    parser.add_argument('--worker', action='store_true', help=argparse.SUPPRESS)
    args = parser.parse_args()
    args.binary = args.binary.resolve()
    args.output = args.output.resolve()
    args.output.mkdir(parents=True, exist_ok=True)
    if not args.binary.is_file():
        parser.error(f'Native Folio binary does not exist: {args.binary}')
    if args.worker:
        if os.environ.get('FOLIO_VIRTUAL_DISPLAY') != '1' or os.environ.get('WAYLAND_DISPLAY'):
            parser.error('The worker requires a caller-owned private X11 display')
        return worker(args)
    xvfb = args.xvfb or shutil.which('Xvfb') or ROOT / 'artifacts/build-tools/Xvfb'
    with tempfile.TemporaryDirectory(prefix='folio-circle-display-') as runtime:
        with (args.output / 'xvfb.log').open('w') as log:
            server = subprocess.Popen([str(xvfb), '-displayfd', '1', '-screen', '0',
                                       '1600x1000x24', '-nolisten', 'tcp', '-ac'],
                                      stdout=subprocess.PIPE, stderr=log, text=True)
            try:
                display = server.stdout.readline().strip()
                if not display:
                    raise RuntimeError('Private Xvfb did not start; inspect xvfb.log')
                env = dict(os.environ, DISPLAY=':' + display, WAYLAND_DISPLAY='',
                           XDG_RUNTIME_DIR=runtime, FOLIO_VIRTUAL_DISPLAY='1')
                software_vulkan = Path('/usr/share/vulkan/icd.d/lvp_icd.x86_64.json')
                if software_vulkan.is_file():
                    env['VK_ICD_FILENAMES'] = str(software_vulkan)
                command = ['dbus-run-session', '--', sys.executable, str(Path(__file__).resolve()),
                           '--worker', '--binary', str(args.binary), '--output', str(args.output)]
                with (args.output / 'native-mouse.log').open('w') as log:
                    result = subprocess.run(command, env=env, stdout=log,
                                            stderr=subprocess.STDOUT, timeout=180)
                print((args.output / 'native-mouse.log').read_text())
                return result.returncode
            finally:
                server.terminate()
                server.wait(timeout=10)


def worker(args):
    import gi
    gi.require_version('Atspi', '2.0')
    from gi.repository import Gio, GLib, Atspi
    from ui_x11 import Client

    launcher = subprocess.Popen(['/usr/libexec/at-spi-bus-launcher', '--launch-immediately'],
                                stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    registry = None
    try:
        time.sleep(.3)
        bus = Gio.bus_get_sync(Gio.BusType.SESSION, None)
        address = bus.call_sync('org.a11y.Bus', '/org/a11y/bus', 'org.a11y.Bus', 'GetAddress',
                                None, GLib.VariantType.new('(s)'), Gio.DBusCallFlags.NONE,
                                5000, None).unpack()[0]
        os.environ['AT_SPI_BUS_ADDRESS'] = address
        registry = subprocess.Popen(['/usr/libexec/at-spi2-registryd'],
                                    stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        time.sleep(.3)
        bus.call_sync('org.a11y.Bus', '/org/a11y/bus', 'org.freedesktop.DBus.Properties', 'Set',
                      GLib.Variant('(ssv)', ('org.a11y.Status', 'IsEnabled', GLib.Variant('b', True))),
                      None, Gio.DBusCallFlags.NONE, 5000, None)
        results = []
        for name, high_zoom, thin_oval in [('normal-zoom', False, False),
                                          ('high-zoom-small-circle', True, False),
                                          ('held-oval-before-scratch', False, True)]:
            with tempfile.TemporaryDirectory(prefix='folio-circle-mouse-') as data:
                with (args.output / (name + '-app.log')).open('w') as log:
                    app = subprocess.Popen([str(args.binary), '--data-dir', data, '--new-note'],
                                           stdout=log, stderr=log)
                    client = None
                    try:
                        client = Client(app.pid)
                        time.sleep(2)
                        for _ in range(5):
                            client.wake_virtual_window()
                        target = None
                        for _ in range(60):
                            while GLib.MainContext.default().iteration(False):
                                pass
                            desktop = Atspi.get_desktop(0)
                            for i in range(desktop.get_child_count()):
                                child = desktop.get_child_at_index(i)
                                if child.get_process_id() == app.pid:
                                    target = child
                                    break
                            if target is not None:
                                break
                            if app.poll() is not None:
                                raise RuntimeError('Folio exited; inspect the case app log')
                            time.sleep(.1)
                        assert target is not None, 'Folio did not register its AT-SPI application'

                        def nodes(node=target):
                            node.clear_cache()
                            yield node
                            for i in range(node.get_child_count()):
                                yield from nodes(node.get_child_at_index(i))

                        def labels():
                            return [node.get_name() for node in nodes()]

                        def click(label):
                            item = next((node for node in nodes() if node.get_name() == label), None)
                            assert item is not None, f'Missing control: {label}; found {labels()}'
                            assert item.get_action_iface().do_action(0), f'Action failed: {label}'
                            time.sleep(.2)

                        def objects():
                            with sqlite3.connect(Path(data) / 'notes.sqlite3') as db:
                                return list(db.execute('SELECT id,data FROM objects ORDER BY id'))

                        def await_objects(predicate, message):
                            for _ in range(60):
                                rows = objects()
                                if predicate(rows):
                                    return rows
                                time.sleep(.05)
                            raise AssertionError(message + f': {objects()}')

                        click('Settings')
                        click('Writing')
                        toggle = next(label for label in labels() if label.startswith('Circle to select:'))
                        if toggle.endswith(': Off'):
                            click(toggle)
                        click('Close settings')
                        if high_zoom:
                            for _ in range(14):
                                click('Zoom in')
                        zoom_label = next(label for label in labels() if label.endswith('%'))
                        if high_zoom:
                            assert int(zoom_label[:-1]) >= 400, f'Test did not reach high zoom: {zoom_label}'
                        canvas = next(node for node in nodes() if node.get_name() == 'Page 1')
                        rect = canvas.get_component_iface().get_extents(Atspi.CoordType.WINDOW)
                        center_x = rect.x + rect.width * .48
                        center_y = rect.y + rect.height * .46
                        xtest = C.CDLL('libXtst.so.6')
                        xtest.XTestFakeMotionEvent.argtypes = [C.c_void_p, C.c_int, C.c_int, C.c_int, C.c_ulong]
                        xtest.XTestFakeButtonEvent.argtypes = [C.c_void_p, C.c_uint, C.c_int, C.c_ulong]
                        client.x.XTranslateCoordinates.argtypes = [C.c_void_p, C.c_ulong, C.c_ulong,
                            C.c_int, C.c_int, C.POINTER(C.c_int), C.POINTER(C.c_int), C.POINTER(C.c_ulong)]

                        def move(x, y, delay=.008):
                            root_x, root_y, child = C.c_int(), C.c_int(), C.c_ulong()
                            assert client.x.XTranslateCoordinates(client.display, client.window, client.root,
                                round(x), round(y), C.byref(root_x), C.byref(root_y), C.byref(child))
                            assert xtest.XTestFakeMotionEvent(client.display, -1, root_x.value, root_y.value, 0)
                            client.x.XFlush(client.display)
                            time.sleep(delay)

                        def button(pressed):
                            assert xtest.XTestFakeButtonEvent(client.display, 1, pressed, 0)
                            client.x.XFlush(client.display)
                            time.sleep(.02)

                        if thin_oval:
                            start = (center_x + 13, center_y - 5)
                            end = (center_x + 13, center_y + 5)
                            radius_x, radius_y, turns = 15, 50, 1.08
                        else:
                            start = (center_x - 14, center_y - 6)
                            end = (center_x + 14, center_y + 6)
                            radius_x, radius_y, turns = 35, 25, 1
                        move(*start)
                        button(1)
                        for i in range(1, 20):
                            t = i / 19
                            move(start[0] + (end[0] - start[0]) * t,
                                 start[1] + (end[1] - start[1]) * t)
                        button(0)
                        original = await_objects(lambda rows: len(rows) == 1, 'Initial ink did not persist')
                        move(center_x + radius_x, center_y)
                        button(1)
                        for i in range(1, 121):
                            angle = i / 120 * math.tau * turns
                            move(center_x + radius_x * math.cos(angle), center_y + radius_y * math.sin(angle))
                        # This verifies the live native 16 ms timer, rather than
                        # advancing Controller's internal Instant in a unit test.
                        time.sleep(.75)
                        assert 'Copy' in labels(), f'{name}: endpoint hold did not select before mouse-up'
                        subprocess.run([sys.executable, str(ROOT / 'scripts/capture-x11.py'),
                            str(args.output / (name + '.png')), '--pid', str(app.pid),
                            '--virtual-display-root'], check=True)
                        button(0)
                        assert objects() == original, f'{name}: selection loop changed or erased original ink'
                        # A point inside the selected object must move it with the
                        # next contact, without drawing another stroke.
                        drag_x = center_x + (13 if thin_oval else 0)
                        move(drag_x, center_y)
                        button(1)
                        move(drag_x + 25, center_y + 15, .05)
                        button(0)
                        moved = await_objects(lambda rows: len(rows) == 1 and rows != original,
                                              f'{name}: selection did not move')
                        assert moved[0][0] == original[0][0], f'{name}: move replaced the ink object'
                        client.key('z', 4)
                        await_objects(lambda rows: rows == original, f'{name}: undo did not restore original ink')
                        client.key('z', 5)
                        await_objects(lambda rows: rows == moved, f'{name}: redo did not restore the move')
                        click('Close window')
                        assert app.wait(timeout=10) == 0
                        assert objects() == moved, f'{name}: clean close lost the move'
                        results.append({'case': name, 'zoom': zoom_label, 'passed': True})
                        print(f'MOUSE_CIRCLE_OK: {name}, {zoom_label}; actual hold before lift, loop consumed, move, undo/redo, durable close', flush=True)
                    finally:
                        if client:
                            client.close()
                        if app.poll() is None:
                            app.terminate()
                            try:
                                app.wait(timeout=5)
                            except subprocess.TimeoutExpired:
                                app.kill()
                                app.wait()
        (args.output / 'results.json').write_text(json.dumps(results, indent=2) + '\n')
        return 0
    finally:
        if registry:
            registry.terminate()
            registry.wait(timeout=10)
        launcher.terminate()
        launcher.wait(timeout=10)


if __name__ == '__main__':
    raise SystemExit(main())
