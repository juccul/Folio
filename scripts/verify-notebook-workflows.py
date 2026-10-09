#!/usr/bin/env python3
"""Verify new notebook workflows in a private X11/D-Bus session.

Uses a deterministic OCR worker, throwaway fixtures and window-scoped input.
This exercises native UI routing/persistence, not physical tablet/model accuracy.
"""
import argparse
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
        parser.error('Use an isolated X11 display and D-Bus session')
    args.output.mkdir(parents=True, exist_ok=True)
    processes = []
    app = client = None
    try:
        processes.append(subprocess.Popen(['/usr/libexec/at-spi-bus-launcher', '--launch-immediately'],
                                         stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL))
        time.sleep(.3)
        bus = Gio.bus_get_sync(Gio.BusType.SESSION, None)
        address = bus.call_sync('org.a11y.Bus', '/org/a11y/bus', 'org.a11y.Bus', 'GetAddress', None,
                               GLib.VariantType.new('(s)'), Gio.DBusCallFlags.NONE, 5000, None).unpack()[0]
        os.environ['AT_SPI_BUS_ADDRESS'] = address
        gi.require_version('Atspi', '2.0')
        from gi.repository import Atspi
        processes.append(subprocess.Popen(['/usr/libexec/at-spi2-registryd'],
                                         stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL))
        bus.call_sync('org.a11y.Bus', '/org/a11y/bus', 'org.freedesktop.DBus.Properties', 'Set',
                      GLib.Variant('(ssv)', ('org.a11y.Status', 'IsEnabled', GLib.Variant('b', True))),
                      None, Gio.DBusCallFlags.NONE, 5000, None)
        with tempfile.TemporaryDirectory(prefix='folio-workflow-ui-') as temp:
            root = Path(temp)
            data = root / 'data'
            shutil.copytree(args.fixture, data, ignore=shutil.ignore_patterns('session.lock', '*.sqlite3-wal', '*.sqlite3-shm'))
            pack = root / 'pack'; pack.mkdir()
            (pack / 'worker.py').write_text("import sys,json\nfor line in sys.stdin:\n json.loads(line)\n print(json.dumps({'text':'searchable ink'}),flush=True)\n")
            (pack / 'pack.json').write_text(json.dumps({'python': sys.executable, 'worker': 'worker.py'}))
            database = data / 'notes.sqlite3'
            def rows(sql, parameters=()):
                with sqlite3.connect(database) as db:
                    return db.execute(sql, parameters).fetchall()
            note = next(json.loads(row[0])['id'] for row in rows('SELECT metadata FROM notes')
                        if json.loads(row[0])['title'] == 'Field notes')
            originals = rows('SELECT id,data FROM objects ORDER BY id')
            original_pages = len(rows('SELECT id FROM pages WHERE note_id=?', (note,)))
            env = dict(os.environ, FOLIO_RECOGNITION_CONFIG=str(pack / 'pack.json'))
            with (args.output / 'runtime.log').open('w') as log:
                app = subprocess.Popen([str(args.binary.resolve()), '--data-dir', str(data), '--open-note', note],
                                       env=env, stdout=log, stderr=log)
                client = Client(app.pid)
                for _ in range(5):
                    client.wake_virtual_window()
                target = None
                for _ in range(60):
                    while GLib.MainContext.default().iteration(False):
                        pass
                    desktop = Atspi.get_desktop(0)
                    target = next((desktop.get_child_at_index(i) for i in range(desktop.get_child_count())
                                   if 'folio' in desktop.get_child_at_index(i).get_name().lower()), None)
                    if target:
                        break
                    time.sleep(.1)
                assert target, 'No Folio accessibility tree'
                def walk(node):
                    node.clear_cache(); yield node
                    for i in range(node.get_child_count()):
                        yield from walk(node.get_child_at_index(i))
                def controls():
                    while GLib.MainContext.default().iteration(False):
                        pass
                    return [n for n in walk(target) if n.get_role() in [Atspi.Role.PUSH_BUTTON, Atspi.Role.PUSH_BUTTON_MENU, Atspi.Role.TOGGLE_BUTTON, Atspi.Role.RADIO_BUTTON, Atspi.Role.CHECK_BOX, Atspi.Role.PAGE_TAB, Atspi.Role.LIST_ITEM, Atspi.Role.TREE_ITEM]]
                def wait(label):
                    for _ in range(100):
                        item = next((n for n in controls() if n.get_name() == label), None)
                        if item:
                            return item
                        time.sleep(.1)
                    raise AssertionError(f'Missing {label}: {[n.get_name() for n in controls()]}')
                def click(label):
                    assert wait(label).get_action_iface().do_action(0), label
                    time.sleep(.4)
                def enter(text):
                    client.key('a', 4)
                    for char in text:
                        client.key('space' if char == ' ' else char)
                def action(label):
                    click('Document actions')
                    if label == 'Index page handwriting…':
                        click('Math and handwriting ›')
                    elif label in {'Name page bookmark…', 'Duplicate current page', 'Use current page as cover', 'Move page to document…', 'Save page as template…', 'Add page from template…'}:
                        click('Current page ›')
                    click(label)
                def capture(name):
                    time.sleep(1.)  # Allow asynchronous text/thumbnail raster jobs to settle.
                    subprocess.run([sys.executable, 'scripts/capture-x11.py', str(args.output / f'{name}.png'),
                                    '--pid', str(app.pid), '--virtual-display-root'], check=True)
                    from PIL import Image
                    with Image.open(args.output / f'{name}.png') as image:
                        pixels = image.convert('RGB').resize((120, 80)).tobytes()
                        assert len({pixels[i:i+3] for i in range(0, len(pixels), 3)}) > 40, name
                def pages(identifier):
                    return rows('SELECT id,header FROM pages WHERE note_id=? ORDER BY position', (identifier,))
                def wait_page_counts(expected):
                    for _ in range(100):
                        actual = {identifier: len(pages(identifier)) for identifier in expected}
                        if actual == expected:
                            return
                        time.sleep(.1)
                    assert actual == expected, (actual, expected)
                def metadata(title):
                    return next(json.loads(row[0]) for row in rows('SELECT metadata FROM notes')
                                if json.loads(row[0])['title'] == title)

                action('Index page handwriting…')
                wait('Keep ink and index')
                enter('verified ink'); capture('index-review')
                click('Keep ink and index')
                assert rows('SELECT id,data FROM objects ORDER BY id') == originals, 'Indexing changed ink/typed objects'
                header = json.loads(pages(note)[0][1])
                assert header['ink_text'][0]['text'] == 'verified ink', header
                client.key('f', 4); enter('verified'); click('Search'); time.sleep(.3)
                assert rows("SELECT body FROM search WHERE search MATCH 'verified'")
                capture('indexed-search')
                result = next(n for n in controls() if n.get_name().startswith('Open result: Field notes ·'))
                assert result.get_action_iface().do_action(0)
                time.sleep(.4); capture('indexed-highlight')
                assert not any(n.get_name() == 'Cancel' for n in controls())

                click('Keyboard shortcuts and help'); click('Open starter document')
                tutorial = metadata('Welcome to Folio')['id']
                assert len(pages(tutorial)) == 4
                capture('starter-notebook')
                action('Name page bookmark…'); enter('practice'); click('Save')
                assert json.loads(pages(tutorial)[0][1])['properties']['bookmark'] == 'practice'
                action('Duplicate current page')
                assert len(pages(tutorial)) == 5
                action('Use current page as cover')
                assert metadata('Welcome to Folio')['cover_page'] in {p[0] for p in pages(tutorial)}
                action('Move page to document…'); capture('move-page-picker'); click('Field notes')
                assert len(pages(tutorial)) == 4 and len(pages(note)) == original_pages + 1, (len(pages(tutorial)), len(pages(note)))
                client.key('z', 4)
                wait_page_counts({tutorial: 5, note: original_pages + 1})

                action('Save page as template…'); enter('practice page'); click('Save')
                for _ in range(100):
                    preferences = json.loads(rows("SELECT data FROM settings WHERE key='preferences'")[0][0])
                    if preferences.get('templates'):
                        break
                    time.sleep(.1)
                assert len(preferences['templates']) == 1
                action('Add page from template…'); capture('template-picker'); click('Add page')
                for _ in range(100):
                    if len(pages(tutorial)) == 6:
                        break
                    time.sleep(.1)
                assert len(pages(tutorial)) == 6
                client.key('z', 4); wait_page_counts({tutorial: 5})
                client.key('z', 5); wait_page_counts({tutorial: 6})
                action('Add page from template…'); click('Rename…'); enter('reusable'); click('Save')
                action('Add page from template…'); click('Remove'); click('Close')
                assert not json.loads(rows("SELECT data FROM settings WHERE key='preferences'")[0][0])['templates']

                click('Keyboard shortcuts and help'); click('Start input check')
                wait('Save input report…'); capture('input-check')
                click('Stop check'); wait('Resume check'); click('Reset check'); wait('Stop check')
                click('Close check')
                assert not any(n.get_name() == 'Save input report…' for n in controls())
                click('Settings'); click('Library')
                wait('Back up library…'); wait('Restore backup…')
                capture('backup-controls'); click('Close settings')
                client.key('s', 4)
                print('NOTEBOOK_WORKFLOWS_OK: reviewed ink index/search, starter, bookmarks, duplicate/move/undo, cover, template lifecycle, input-check controls, backup controls', flush=True)
                print('Excluded: physical input, real OCR accuracy, OS file-dialog export/restore (archive roundtrip covered by Rust tests)', flush=True)
    finally:
        if client:
            client.close()
        if app and app.poll() is None:
            app.terminate()
            try:
                app.wait(timeout=5)
            except subprocess.TimeoutExpired:
                app.kill(); app.wait()
        for process in reversed(processes):
            process.terminate(); process.wait(timeout=5)


if __name__ == '__main__':
    main()
