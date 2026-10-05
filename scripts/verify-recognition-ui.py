#!/usr/bin/env python3
"""Native copy/review/replace verification on a private X11/D-Bus session.

Uses a deterministic local worker to exercise UI/clipboard independently of model
accuracy. Real model recognition is checked by the recognition_fixture example.
"""
import argparse
import ctypes
import json
import os
from pathlib import Path
import shutil
import sqlite3
import subprocess
import sys
import tempfile
import time

import gi
from gi.repository import Gio, GLib
from ui_x11 import Client

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--fixture', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    if os.environ.get('FOLIO_VIRTUAL_DISPLAY') != '1' or os.environ.get('WAYLAND_DISPLAY'):
        parser.error('Use a private X11 display and D-Bus session with FOLIO_VIRTUAL_DISPLAY=1')
    args.output.mkdir(parents=True, exist_ok=True)
    processes = []
    app = client = None
    try:
        processes.append(subprocess.Popen(['/usr/libexec/at-spi-bus-launcher', '--launch-immediately'], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL))
        time.sleep(.3)
        bus = Gio.bus_get_sync(Gio.BusType.SESSION, None)
        address = bus.call_sync('org.a11y.Bus', '/org/a11y/bus', 'org.a11y.Bus', 'GetAddress', None,
            GLib.VariantType.new('(s)'), Gio.DBusCallFlags.NONE, 5000, None).unpack()[0]
        os.environ['AT_SPI_BUS_ADDRESS'] = address
        gi.require_version('Atspi', '2.0')
        from gi.repository import Atspi
        processes.append(subprocess.Popen(['/usr/libexec/at-spi2-registryd'], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL))
        time.sleep(.3)
        bus.call_sync('org.a11y.Bus', '/org/a11y/bus', 'org.freedesktop.DBus.Properties', 'Set',
            GLib.Variant('(ssv)', ('org.a11y.Status', 'IsEnabled', GLib.Variant('b', True))),
            None, Gio.DBusCallFlags.NONE, 5000, None)
        with tempfile.TemporaryDirectory(prefix='folio-recognition-ui-') as temp:
            root = Path(temp)
            shutil.copytree(args.fixture, root / 'data', ignore=shutil.ignore_patterns('session.lock', '*.sqlite3-wal', '*.sqlite3-shm'))
            pack = root / 'pack'; pack.mkdir()
            (pack / 'worker.py').write_text("import sys,json,time\nfor line in sys.stdin:\n json.loads(line)\n time.sleep(.1)\n print(json.dumps({'text':'Recognized suggestion'}),flush=True)\n")
            (pack / 'pack.json').write_text(json.dumps({'python': sys.executable, 'worker': 'worker.py'}))
            database = root / 'data/notes.sqlite3'
            with sqlite3.connect(database) as db:
                note = db.execute('SELECT id FROM notes LIMIT 1').fetchone()[0]
                original_count = db.execute('SELECT count(*) FROM objects').fetchone()[0]
                assert original_count > 1
            env = dict(os.environ, FOLIO_RECOGNITION_CONFIG=str(pack / 'pack.json'))
            with (args.output / 'runtime.log').open('w') as log:
                app = subprocess.Popen([str(args.binary.resolve()), '--data-dir', str(root / 'data'), '--open-note', note], env=env, stdout=log, stderr=log)
                client = Client(app.pid)
                for _ in range(5): client.wake_virtual_window()
                target = None
                for _ in range(50):
                    while GLib.MainContext.default().iteration(False): pass
                    desktop = Atspi.get_desktop(0)
                    target = next((desktop.get_child_at_index(i) for i in range(desktop.get_child_count())
                        if 'folio' in desktop.get_child_at_index(i).get_name().lower()), None)
                    if target: break
                    time.sleep(.1)
                assert target is not None, 'No native accessibility tree'
                def walk(node):
                    node.clear_cache(); yield node
                    for i in range(node.get_child_count()): yield from walk(node.get_child_at_index(i))
                def controls():
                    while GLib.MainContext.default().iteration(False): pass
                    return [n for n in walk(target) if n.get_role() == Atspi.Role.PUSH_BUTTON]
                def click(label):
                    item = next((n for n in controls() if n.get_name() == label), None)
                    assert item is not None, f'Missing {label}: {[n.get_name() for n in controls()]}'
                    assert item.get_action_iface().do_action(0)
                    time.sleep(.4)
                def count():
                    with sqlite3.connect(database) as db: return db.execute('SELECT count(*) FROM objects').fetchone()[0]
                client.key('a', 4)
                click('Recognize text')
                for _ in range(50):
                    if any(n.get_name() == 'Copy text' for n in controls()): break
                    time.sleep(.1)
                assert any(n.get_role() == Atspi.Role.ENTRY and n.get_name() == 'Review recognized writing' for n in walk(target))
                assert count() == original_count, 'Recognition changed the writing'
                assert not any(n.get_name() in {'Undo', 'Delete', 'Duplicate'} for n in controls()), 'Review leaked background actions'
                client.key('a', 4)
                corrected = 'corrected\nsecond line'
                for char in corrected:
                    client.key('Return' if char == '\n' else 'space' if char == ' ' else char)
                click('Copy text')
                assert count() == original_count, 'Copy changed the writing'
                # Clear the review, then paste from the real GPUI native clipboard.
                client.key('a', 4); client.key('BackSpace'); client.key('v', 4)
                subprocess.run([sys.executable, 'scripts/capture-x11.py', str(args.output / 'review.png'),
                    '--pid', str(app.pid), '--virtual-display-root'], check=True)
                click('Replace writing')
                assert count() == 1, 'Replacement did not remove the selected ink'
                with sqlite3.connect(database) as db:
                    stored = json.loads(db.execute('SELECT data FROM objects').fetchone()[0])
                assert stored['Text']['text'] == corrected, f'Clipboard did not contain corrected full text: {stored}'
                client.key('z', 4); time.sleep(.3)
                assert count() == original_count, 'Undo did not restore all source writing'
                client.key('z', 5); time.sleep(.3)
                assert count() == 1, 'Redo did not restore replacement'
                client.key('z', 4); time.sleep(.3); client.key('a', 4)
                click('Recognize math')
                for _ in range(50):
                    if any(n.get_name() == 'Copy text' for n in controls()): break
                    time.sleep(.1)
                click('Cancel')
                assert count() == original_count, 'Cancel changed ink'
                assert not any(n.get_name() == 'Copy text' for n in controls()), 'Cancel reopened review'
                click('Recognize math')
                for _ in range(50):
                    if any(n.get_name() == 'Copy text' for n in controls()): break
                    time.sleep(.1)
                client.key('a', 4)
                latex = r'\frac{x-y}{\sqrt{2}}'
                key_names = {'\\': ('backslash', 0), '{': ('braceleft', 1),
                             '}': ('braceright', 1), '-': ('minus', 0)}
                # GPUI reads characters from the server's XKB state, separately
                # from the modifiers on our window-scoped synthetic event.
                # Lock Shift briefly on this private display for brace input.
                client.x.XkbLockModifiers.argtypes = [ctypes.c_void_p, ctypes.c_uint,
                                                      ctypes.c_uint, ctypes.c_uint]
                for char in latex:
                    name, modifiers = key_names.get(char, (char, 0))
                    if modifiers & 1:
                        client.x.XkbLockModifiers(client.display, 0x100, 1, 1)
                        client.x.XFlush(client.display); time.sleep(.1)
                    try:
                        client.key(name, modifiers)
                    finally:
                        if modifiers & 1:
                            client.x.XkbLockModifiers(client.display, 0x100, 1, 0)
                            client.x.XFlush(client.display); time.sleep(.1)
                click('Copy text')
                assert count() == original_count, 'Copying LaTeX changed ink'
                client.key('a', 4); client.key('BackSpace'); client.key('v', 4)
                click('Replace writing')
                for _ in range(100):
                    if count() == 1: break
                    time.sleep(.1)
                with sqlite3.connect(database) as db:
                    stored = json.loads(db.execute('SELECT data FROM objects').fetchone()[0])
                assert stored['Equation']['latex'] == latex, stored
                assert '<path' in stored['Equation']['rendered_svg'], 'Equation has no rendered glyphs'
                time.sleep(.5)
                subprocess.run([sys.executable, 'scripts/capture-x11.py', str(args.output / 'equation.png'),
                    '--pid', str(app.pid), '--virtual-display-root'], check=True)
                client.key('z', 4); time.sleep(.3)
                assert count() == original_count, 'Equation undo did not restore ink'
                client.key('z', 5); time.sleep(.3)
                assert count() == 1, 'Equation redo did not restore rendering'
                result = {'native_clipboard_corrected_text': True, 'review_before_conversion': True,
                    'replace_undo_redo': True, 'math_cancel_keeps_ink': True,
                    'math_clipboard_source': True, 'math_rendered_equation': True,
                    'math_replace_undo_redo': True, 'original_strokes': original_count}
                (args.output / 'result.json').write_text(json.dumps(result, indent=2) + '\n')
                print('RECOGNITION_UI_OK:', json.dumps(result))
    finally:
        if client: client.close()
        if app and app.poll() is None:
            app.terminate()
            try: app.wait(timeout=5)
            except subprocess.TimeoutExpired: app.kill(); app.wait()
        for process in reversed(processes):
            process.terminate(); process.wait(timeout=5)

if __name__ == '__main__': main()
