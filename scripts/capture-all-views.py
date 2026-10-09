#!/usr/bin/env python3
"""Capture paired light/dark native Folio views using a disposable demo library.

Owns its Xvfb and private D-Bus session. Never opens the user's notes or sends
input to their desktop. The JSON inventory records every captured view and any
condition-dependent view that was unavailable in the demo fixture.
"""
import argparse
import ctypes as C
import html
import hashlib
import json
import os
from pathlib import Path
import shutil
import sqlite3
import subprocess
import sys
import tempfile
import time
import uuid

ROOT = Path(__file__).resolve().parents[1]


def gallery(output, inventory):
    views = {}
    for record in inventory:
        views.setdefault(record['view'], {})[record['mode']] = record
    body = []
    for name, modes in views.items():
        body.append('<section><h2>' + html.escape(name.replace('-', ' ').title()) + '</h2><div class="pair">')
        for mode in ('light', 'dark'):
            record = modes.get(mode, {})
            body.append('<figure><figcaption>' + mode.title() + '</figcaption>')
            if record.get('file'):
                file = html.escape(record['file'], quote=True)
                body.append(f'<a href="{file}"><img loading="lazy" src="{file}" alt="{html.escape(name)} — {mode}"></a>')
            else:
                body.append('<p>' + html.escape(record.get('error', 'Not captured')) + '</p>')
            body.append('</figure>')
        body.append('</div></section>')
    (output / 'index.html').write_text('''<!doctype html><meta charset="utf-8"><title>Folio · all views</title>
<style>body{font:16px system-ui;background:#171717;color:#eee;margin:32px}h1{margin-bottom:8px}h2{font-size:19px}section{margin:40px 0}.pair{display:grid;grid-template-columns:1fr 1fr;gap:20px}figure{margin:0;min-width:0}figcaption{margin:8px 0;color:#aaa}img{width:100%;display:block;border:1px solid #444}p{color:#bbb}@media(max-width:800px){.pair{grid-template-columns:1fr}}</style>
<h1>Folio · paired light and dark views</h1><p>Actual native app captures from an isolated demo library. Click any image for the full-size screenshot. Unavailable condition-dependent screens are recorded below.</p>''' + ''.join(body))


def report(args):
    inventory = json.loads((args.output / 'inventory.json').read_text())
    summary = {
        'binary': str(args.binary.resolve()),
        'version': subprocess.check_output([str(args.binary.resolve()), '--version'], text=True).strip(),
        'binary_sha256': hashlib.sha256(args.binary.read_bytes()).hexdigest(),
        'source_commit': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip(),
        'resolution': [1440, 1000],
        'captured': sum('file' in item for item in inventory),
        'views_per_mode': {mode: sum('file' in item and item['mode'] == mode for item in inventory) for mode in ('light', 'dark')},
        'unavailable': [item for item in inventory if 'error' in item],
        'isolated_demo_library': True,
    }
    (args.output / 'capture-summary.json').write_text(json.dumps(summary, indent=2) + '\n')


def capture_session(args):
    assert os.environ.get('FOLIO_SCREENSHOT_SESSION') == '1'
    assert os.environ.get('FOLIO_VIRTUAL_DISPLAY') == '1' and not os.environ.get('WAYLAND_DISPLAY')
    import gi
    from gi.repository import Gio, GLib
    from ui_x11 import Client
    processes = []
    inventory = json.loads((args.output / 'inventory.json').read_text()) if (args.pdf_only or args.media_only) and (args.output / 'inventory.json').is_file() else []
    update_unavailable = 'A newer published release is required; this source build is already newer than public 0.1.3'
    for record in inventory:
        if record['view'] == 'update-download-restart':
            record['error'] = update_unavailable
    try:
        processes.append(subprocess.Popen(['/usr/libexec/at-spi-bus-launcher', '--launch-immediately'], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL))
        time.sleep(.3)
        bus = Gio.bus_get_sync(Gio.BusType.SESSION, None)
        address = bus.call_sync('org.a11y.Bus', '/org/a11y/bus', 'org.a11y.Bus', 'GetAddress', None, GLib.VariantType.new('(s)'), Gio.DBusCallFlags.NONE, 5000, None).unpack()[0]
        os.environ['AT_SPI_BUS_ADDRESS'] = address
        gi.require_version('Atspi', '2.0')
        from gi.repository import Atspi
        processes.append(subprocess.Popen(['/usr/libexec/at-spi2-registryd'], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL))
        bus.call_sync('org.a11y.Bus', '/org/a11y/bus', 'org.freedesktop.DBus.Properties', 'Set', GLib.Variant('(ssv)', ('org.a11y.Status', 'IsEnabled', GLib.Variant('b', True))), None, Gio.DBusCallFlags.NONE, 5000, None)
        portal = subprocess.Popen([sys.executable, str(ROOT / 'scripts/ui_theme_portal.py')], stdout=subprocess.PIPE, text=True)
        processes.append(portal)
        assert portal.stdout.readline().strip() == 'READY', 'Private settings portal failed'
        for mode in ('light', 'dark'):
            with tempfile.TemporaryDirectory(prefix='folio-screenshot-demo-') as temporary:
                data = Path(temporary) / 'data'
                shutil.copytree(args.fixture, data, ignore=shutil.ignore_patterns('session.lock', '*.sqlite3-wal', '*.sqlite3-shm'))
                with sqlite3.connect(data / 'notes.sqlite3') as db:
                    row = db.execute("SELECT data FROM settings WHERE key='preferences'").fetchone()
                    preferences = json.loads(row[0]) if row else {}
                    preferences.update(dark=mode == 'dark', follow_system_theme=False, reduce_motion=True, ui_scale=1, workspace={'library_open': True})
                    db.execute("INSERT OR REPLACE INTO settings(key,data) VALUES('preferences',?)", (json.dumps(preferences),))
                    note, metadata = db.execute('SELECT id,metadata FROM notes LIMIT 1').fetchone()
                    metadata = json.loads(metadata)
                    trash = dict(metadata, id=str(uuid.uuid4()), title='Archived ideas', trashed=True, favorite=False)
                    db.execute('INSERT INTO notes(id,metadata) VALUES(?,?)', (trash['id'], json.dumps(trash)))
                    # Extra condition-dependent views use generated demo media,
                    # not any imported files from the user's library.
                    from PIL import Image, ImageDraw, ImageFont
                    assets = data / 'assets'
                    assets.mkdir(exist_ok=True)
                    image = Image.new('RGB', (640, 360), '#e7e9e2')
                    drawing = ImageDraw.Draw(image)
                    drawing.rectangle((48, 48, 592, 312), outline='#445348', width=4)
                    drawing.line((64, 266, 210, 140, 364, 208, 572, 86), fill='#3265a8', width=6)
                    drawing.text((72, 64), 'FIELD OBSERVATIONS / DEMO IMAGE', fill='#273448')
                    image.save(assets / 'screenshot-demo.png')
                    pdf = Image.new('RGB', (794, 1123), 'white')
                    pdf_draw = ImageDraw.Draw(pdf)
                    pdf_draw.text((70, 80), 'FOLIO / DEMO READING PAGE', font=ImageFont.load_default(size=28), fill='#273448')
                    pdf_draw.multiline_text((70, 150), 'A PDF page keeps its original paper.\nThe workspace follows your selected theme.\n\nTry adding notes, highlights or equations.', font=ImageFont.load_default(size=22), spacing=14, fill='#273448')
                    pdf_draw.line((70, 320, 724, 320), fill='#3265a8', width=3)
                    pdf.save(assets / 'screenshot-demo.pdf', 'PDF', resolution=96)
                    pdf.save(assets / 'screenshot-demo-pdf.png')
                    encrypted_pdf = Path(temporary) / 'Encrypted demo.pdf'
                    if shutil.which('gs'):
                        subprocess.run(['gs', '-q', '-sDEVICE=pdfwrite', '-dBATCH', '-dNOPAUSE', '-dEncryptionR=3', '-dKeyLength=128', '-sOwnerPassword=folio-owner', '-sUserPassword=folio-demo', '-sOutputFile=' + str(encrypted_pdf), str(assets / 'screenshot-demo.pdf')], check=True, stdout=subprocess.DEVNULL)
                    page, header = db.execute('SELECT id,header FROM pages WHERE note_id=? ORDER BY position LIMIT 1', (note,)).fetchone()
                    template_header = json.loads(header)
                    identity = {'a': 1, 'b': 0, 'c': 0, 'd': 1, 'tx': 0, 'ty': 0}
                    for title, object_type in [('Demo image', 'Image'), ('Demo equation', 'Equation'), ('Demo PDF', 'PDF')]:
                        demo = dict(metadata, id=str(uuid.uuid4()), title=title, trashed=False, favorite=False, notebook=None)
                        demo_page = dict(template_header, id=str(uuid.uuid4()), order=[], groups=[], text='', revision=0)
                        demo_page['properties'] = dict(demo_page['properties'], paper='Blank')
                        if object_type == 'PDF':
                            demo_page['properties']['pdf'] = {'asset': 'screenshot-demo.pdf', 'page': 1, 'preview_asset': 'screenshot-demo-pdf.png'}
                        else:
                            object_id = str(uuid.uuid4())
                            rect = {'min': {'x': 64, 'y': 120}, 'max': {'x': 704, 'y': 480}}
                            content = {'id': object_id, 'rect': rect, 'transform': identity}
                            if object_type == 'Image':
                                content.update(asset='screenshot-demo.png', crop=None)
                            else:
                                content['rect'] = {'min': {'x': 64, 'y': 120}, 'max': {'x': 704, 'y': 280}}
                                content.update(latex='x^2 + 2x + 1', rendered_svg='<svg xmlns="http://www.w3.org/2000/svg" width="320" height="80" viewBox="0 0 320 80"><g fill="none" stroke="#3265a8" stroke-width="3" stroke-linecap="round" stroke-linejoin="round"><path d="M10 30L30 55M30 30L10 55M36 18Q42 10 47 18Q49 22 36 28H49M65 42H87M76 31V53M106 33Q116 23 125 33Q130 42 108 54H128M144 30L164 55M164 30L144 55M183 42H205M194 31V53M223 34L233 28V55M225 55H242"/></g></svg>', source_strokes=[], math_link=None)
                            demo_page['order'] = [object_id]
                        db.execute('INSERT INTO notes(id,metadata) VALUES(?,?)', (demo['id'], json.dumps(demo)))
                        db.execute('INSERT INTO pages(id,note_id,position,header) VALUES(?,?,0,?)', (demo_page['id'], demo['id'], json.dumps(demo_page)))
                        if object_type != 'PDF':
                            db.execute('INSERT INTO objects(id,page_id,data) VALUES(?,?,?)', (object_id, demo_page['id'], json.dumps({object_type: content})))
                with (args.output / (mode + '-runtime.log')).open('w') as log:
                    command = [str(args.binary.resolve()), '--data-dir', str(data)]
                    if encrypted_pdf.is_file():
                        command.append(str(encrypted_pdf))
                    app = subprocess.Popen(command, stdout=log, stderr=log)
                    client = None
                    try:
                        client = Client(app.pid)
                        for _ in range(5):
                            client.wake_virtual_window()
                        client.resize(1440, 1000)
                        client.x.XSetInputFocus.argtypes = [C.c_void_p, C.c_ulong, C.c_int, C.c_ulong]
                        client.x.XSetInputFocus(client.display, client.window, 1, 0)
                        client.x.XFlush(client.display)
                        target = None
                        for _ in range(60):
                            desktop = Atspi.get_desktop(0)
                            target = next((desktop.get_child_at_index(i) for i in range(desktop.get_child_count()) if desktop.get_child_at_index(i).get_process_id() == app.pid), None)
                            if target:
                                break
                            if app.poll() is not None:
                                raise RuntimeError('Folio exited; inspect runtime log')
                            time.sleep(.1)
                        assert target, 'No Folio accessibility tree'

                        def walk(node):
                            if node is None:
                                return
                            node.clear_cache()
                            yield node
                            for i in range(node.get_child_count()):
                                yield from walk(node.get_child_at_index(i))

                        def nodes():
                            while GLib.MainContext.default().iteration(False):
                                pass
                            for attempt in range(4):
                                try:
                                    return list(walk(target))
                                except GLib.Error:
                                    if attempt == 3:
                                        raise
                                    # An asynchronous load may replace a native
                                    # subtree halfway through the traversal.
                                    time.sleep(.1)

                        def controls():
                            roles = {Atspi.Role.PUSH_BUTTON, Atspi.Role.PUSH_BUTTON_MENU, Atspi.Role.RADIO_BUTTON, Atspi.Role.CHECK_BOX, Atspi.Role.TOGGLE_BUTTON, Atspi.Role.PAGE_TAB, Atspi.Role.LIST_ITEM, Atspi.Role.TREE_ITEM}
                            return [node for node in nodes() if node.get_role() in roles]

                        def click(label, prefix=False):
                            for _ in range(12):
                                found = [n for n in controls() if (n.get_name().startswith(label) if prefix else n.get_name() == label or n.get_name().startswith(label + ' ·')) and n.get_action_iface() and n.get_action_iface().get_n_actions()]
                                if found:
                                    assert found[-1].get_action_iface().do_action(0), label
                                    time.sleep(.28)
                                    return
                                time.sleep(.08)
                            raise RuntimeError('Missing action: ' + label)

                        def fill(text):
                            client.key('a', 4, delay=.03)
                            client.key('BackSpace', delay=.03)
                            symbols = {' ': 'space', '=': 'equal', '+': 'plus', '-': 'minus', '^': 'asciicircum', '(': 'parenleft', ')': 'parenright'}
                            for char in text:
                                client.key(symbols.get(char, char.lower()), 1 if char.isupper() or char in '+^()' else 0, delay=.03)

                        def dismiss():
                            # Closing overlays does not navigate or modify demo content.
                            for label in ('Cancel', 'Close', 'Close settings', 'Got it'):
                                if any(n.get_name() == label for n in nodes()):
                                    click(label)
                            client.key('Escape', delay=.15)
                            client.key('Escape', delay=.15)

                        def library():
                            dismiss()
                            if any(n.get_name().startswith('Library ·') for n in nodes()):
                                click('Library ·', True)
                            click('Documents')

                        def editor():
                            library()
                            click('Open Field notes')

                        def more(section=None):
                            editor()
                            click('Document actions')
                            if section:
                                click(section + ' ›')

                        def capture(name, actions):
                            record = {'view': name, 'mode': mode}
                            try:
                                actions()
                                time.sleep(.25)
                                filename = mode + '-' + name + '.png'
                                from PIL import Image
                                for attempt in range(8):
                                    if name in ('editor-pdf', 'editor-image'):
                                        # Bare Xvfb may miss the image-loader repaint
                                        # transition. Replay mapping only on this
                                        # owned virtual window, then verify actual
                                        # demo content rather than chrome entropy.
                                        client.wake_virtual_window()
                                    subprocess.run([sys.executable, str(ROOT / 'scripts/capture-x11.py'), str(args.output / filename), '--pid', str(app.pid), '--virtual-display-root'], check=True, stdout=subprocess.DEVNULL)
                                    with Image.open(args.output / filename) as image:
                                        assert len(set(image.convert('RGB').resize((120, 80)).getdata())) > 40, 'Blank capture'
                                        if name not in ('editor-pdf', 'editor-image'):
                                            break
                                        pixels = image.convert('RGB').crop((300, 210, 1300, 920)).getdata()
                                        blue = sum(red < 130 and green - red > 20 and blue - green > 30 for red, green, blue in pixels)
                                        if blue > 80:
                                            break
                                    time.sleep(.5)
                                else:
                                    raise RuntimeError('Demo media did not render in the main canvas')
                                record['file'] = filename
                                try:
                                    record['controls'] = [n.get_name() for n in controls() if n.get_action_iface() and n.get_action_iface().get_n_actions()]
                                except GLib.Error:
                                    record['accessibility_warning'] = 'Native tree changed during post-capture inventory traversal'
                                print(mode, name, 'OK', flush=True)
                            except Exception as error:
                                record['error'] = str(error)
                                print(mode, name, str(error), flush=True)
                            inventory[:] = [item for item in inventory if (item['view'], item['mode']) != (name, mode)]
                            inventory.append(record)
                            (args.output / 'inventory.json').write_text(json.dumps(inventory, indent=2) + '\n')
                            gallery(args.output, inventory)

                        def sequence(*actions):
                            for action in actions:
                                if callable(action):
                                    action()
                                else:
                                    click(action)

                        def require(label):
                            assert any(n.get_name() == label for n in nodes()), 'Expected screen: ' + label

                        if args.pdf_only or args.media_only:
                            capture('editor-pdf', lambda: sequence(library, 'Open Demo PDF', lambda: time.sleep(2)))
                            if args.media_only:
                                capture('editor-image', lambda: sequence(library, 'Open Demo image', lambda: client.key('a', 4), lambda: time.sleep(2)))
                                capture('image-crop', lambda: sequence(library, 'Open Demo image', lambda: client.key('a', 4), 'Crop…'))
                                capture('image-crop-coordinates', lambda: sequence(library, 'Open Demo image', lambda: client.key('a', 4), 'Crop with numbers…'))
                                capture('edit-equation', lambda: sequence(library, 'Open Demo equation', lambda: client.key('a', 4), lambda: time.sleep(2), 'Edit equation'))
                            continue

                        if encrypted_pdf.is_file():
                            capture('pdf-password', lambda: require('Unlock PDF'))

                        for view, actions in [
                            ('library-grid', lambda: sequence(library, 'Grid view')),
                            ('library-list', lambda: sequence(library, 'List view')),
                            ('library-favorites', lambda: sequence(library, 'Favorites')),
                            ('library-recent', lambda: sequence(library, 'Recent')),
                            ('library-trash', lambda: sequence(library, 'Trash')),
                            ('library-folder', lambda: sequence(library, 'Projects')),
                            ('library-sort', lambda: sequence(library, 'Last edited: newest first ▾')),
                            ('new-folder', lambda: sequence(library, 'New folder')),
                            ('folder-rename', lambda: sequence(library, 'Projects', 'Rename folder…')),
                            ('folder-move', lambda: sequence(library, 'Projects', 'Move folder…')),
                            ('new-subfolder', lambda: sequence(library, 'Projects', 'New subfolder…')),
                            ('document-menu', lambda: sequence(library, 'Manage Reading list')),
                            ('document-rename', lambda: sequence(library, 'Manage Reading list', 'Rename…')),
                            ('document-tags', lambda: sequence(library, 'Manage Reading list', 'Edit tags…')),
                            ('document-move', lambda: sequence(library, 'Manage Reading list', 'Move to folder…')),
                            ('new-document', lambda: sequence(library, 'New document')),
                            ('search', lambda: sequence(library, 'Search all documents · Ctrl+F')),
                            ('help', lambda: sequence(library, 'Keyboard shortcuts and help')),
                            ('starter-document', lambda: sequence(library, 'Keyboard shortcuts and help', 'Open starter document')),
                            ('input-check', lambda: sequence(library, 'Keyboard shortcuts and help', 'Start input check')),
                        ]:
                            capture(view, actions)
                        if any(n.get_name() == 'Close check' for n in nodes()):
                            click('Close check')
                        for section in ('Appearance', 'Writing', 'Library', 'Accessibility', 'Updates'):
                            capture('settings-' + section.lower(), lambda section=section: sequence(library, 'Settings', section))
                            if section in ('Library', 'Accessibility'):
                                capture('settings-' + section.lower() + '-bottom', lambda: [client.click(1020, 700, button=5, delay=.04) for _ in range(12)])
                        capture('settings-custom-palette', lambda: sequence(library, 'Settings', 'Appearance', 'Edit colors…'))
                        capture('settings-custom-palette-bottom', lambda: [client.click(1020, 700, button=5, delay=.04) for _ in range(16)])
                        capture('settings-theme-color', lambda: sequence(library, 'Settings', 'Appearance', lambda: click('Edit colors…') if any(n.get_name() == 'Edit colors…' for n in nodes()) else None, 'Customize Primary'))
                        capture('settings-paper-color', lambda: sequence(library, 'Settings', 'Appearance', 'Paper follows appearance: On', 'Custom paper color'))
                        client.key('Escape')
                        click('Appearance')
                        click('Paper follows appearance: Off')
                        capture('settings-pad-buttons', lambda: sequence(library, 'Settings', 'Writing', 'Configure…'))
                        capture('editor', editor)
                        capture('editor-pages', lambda: sequence(editor, lambda: click('Page thumbnails', True) if not any(n.get_name() == 'Hide pages' for n in nodes()) else None))
                        capture('editor-page-two', lambda: sequence(editor, 'Go to page 2'))
                        capture('editor-tabs', lambda: sequence(library, 'Open Reading list'))
                        capture('open-document', lambda: sequence(editor, 'Open or create a document · Ctrl+T'))
                        capture('editor-selection', lambda: sequence(editor, lambda: client.key('a', 4)))
                        capture('text-font', lambda: sequence(editor, lambda: client.key('a', 4), 'sans-serif'))
                        capture('text-size', lambda: sequence(editor, lambda: client.key('a', 4), lambda: click(next(n.get_name() for n in nodes() if n.get_name().endswith(' pt')))))
                        capture('text-color', lambda: sequence(editor, lambda: client.key('a', 4), 'Text color'))
                        capture('editor-pdf', lambda: sequence(library, 'Open Demo PDF', lambda: time.sleep(2)))
                        capture('editor-image', lambda: sequence(library, 'Open Demo image', lambda: client.key('a', 4), lambda: time.sleep(2)))
                        capture('image-crop', lambda: sequence(library, 'Open Demo image', lambda: client.key('a', 4), 'Crop…'))
                        capture('image-crop-coordinates', lambda: sequence(library, 'Open Demo image', lambda: client.key('a', 4), 'Crop with numbers…'))
                        capture('edit-equation', lambda: sequence(library, 'Open Demo equation', lambda: client.key('a', 4), 'Edit equation'))
                        for section in (None, 'Current page', 'Paper and canvas', 'Math and handwriting', 'Page layout'):
                            capture('editor-actions-' + ('document' if section is None else section.lower().replace(' ', '-')), lambda section=section: more(section))
                        for name, section, label in [
                            ('page-bookmark', 'Current page', 'Name page bookmark…'),
                            ('page-move', 'Current page', 'Move page to document…'),
                            ('page-save-template', 'Current page', 'Save page as template…'),
                            ('page-templates', 'Current page', 'Add page from template…'),
                            ('page-size', 'Paper and canvas', 'Custom page size'),
                            ('insert-equation', 'Math and handwriting', 'Insert LaTeX equation'),
                        ]:
                            capture(name, lambda section=section, label=label: sequence(lambda: more(section), label))
                        capture('editor-custom-ink', lambda: sequence(editor, 'Custom ink color'))
                        capture('pen-settings', lambda: sequence(editor, 'Pen · P', 'Pen settings and presets'))
                        capture('pen-save-preset', lambda: sequence(editor, 'Pen · P', 'Pen settings and presets', 'Save current pen as preset…'))
                        sequence(lambda: fill('Demo pen'), 'Save')
                        capture('pen-presets', lambda: sequence(editor, 'Pen · P', 'Pen settings and presets'))
                        capture('pen-rename-preset', lambda: sequence(editor, 'Pen · P', 'Pen settings and presets', 'Rename'))
                        capture('eraser-settings', lambda: sequence(editor, 'Eraser · E', 'Eraser modes and size'))
                        capture('shape-tool', lambda: sequence(editor, 'Shapes · S'))
                        for name, label in [('export-original', 'Original colors'), ('export-visible', 'Visible appearance'), ('export-print', 'Light paper for printing')]:
                            capture(name, lambda label=label: sequence(editor, 'Export document', label))
                        capture('math-solution', lambda: sequence(lambda: more('Math and handwriting'), 'Math solver'))
                        if any(n.get_name() == 'Solution' for n in nodes()):
                            capture('math-graph', lambda: click('Graph'))
                            capture('math-check', lambda: click('Check work'))
                            capture('math-options', lambda: sequence('Solution', 'Settings', 'Read options'))
                            click('Close')
                        else:
                            for name in ('math-graph', 'math-check', 'math-options'):
                                inventory.append({'view': name, 'mode': mode, 'error': 'Math runtime unavailable in fixture; solver initialization failed'})
                        # These require imported media, recognition output, or a
                        # trusted release newer than this source binary.
                        for name, reason in [
                            *([] if encrypted_pdf.is_file() else [('pdf-password', 'Requires Ghostscript to generate an encrypted demo PDF')]),
                            ('recognition-review', 'Requires installed recognition model and recognized handwriting'),
                            ('update-download-restart', update_unavailable),
                            ('system-file-dialogs', 'File dialogs are desktop portal UI, outside the app client and private demo session'),
                        ]:
                            inventory.append({'view': name, 'mode': mode, 'error': reason})
                    finally:
                        if client:
                            client.close()
                        app.terminate()
                        try:
                            app.wait(timeout=5)
                        except subprocess.TimeoutExpired:
                            app.kill()
                            app.wait()
        (args.output / 'inventory.json').write_text(json.dumps(inventory, indent=2) + '\n')
        gallery(args.output, inventory)
    finally:
        for process in reversed(processes):
            process.terminate()
            process.wait(timeout=5)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, default=ROOT / 'target/debug/folio')
    parser.add_argument('--fixture', type=Path, default=ROOT / 'artifacts/validation/performance-pass/ui-fixture')
    parser.add_argument('--output', type=Path, default=ROOT / 'artifacts/screenshots-all-views')
    parser.add_argument('--xvfb', type=Path, default=ROOT / 'artifacts/build-tools/Xvfb')
    parser.add_argument('--private-session', action='store_true', help=argparse.SUPPRESS)
    parser.add_argument('--pdf-only', action='store_true', help='Refresh only the demo PDF pair in an existing gallery')
    parser.add_argument('--media-only', action='store_true', help='Refresh PDF, image and crop pairs after asynchronous asset loading')
    args = parser.parse_args()
    args.output = args.output.resolve()
    args.output.mkdir(parents=True, exist_ok=True)
    if args.private_session:
        capture_session(args)
        return
    with tempfile.TemporaryDirectory(prefix='folio-screenshot-runtime-') as runtime:
        with (args.output / 'xvfb.log').open('w') as log:
            server = subprocess.Popen([str(args.xvfb), '-displayfd', '1', '-screen', '0', '1600x1200x24', '-nolisten', 'tcp', '-ac'], stdout=subprocess.PIPE, stderr=log, text=True)
            try:
                display = server.stdout.readline().strip()
                assert display, 'Private Xvfb failed to start'
                environment = dict(os.environ, DISPLAY=':' + display, WAYLAND_DISPLAY='', XDG_RUNTIME_DIR=runtime, FOLIO_VIRTUAL_DISPLAY='1', FOLIO_SCREENSHOT_SESSION='1', VK_ICD_FILENAMES='/usr/share/vulkan/icd.d/lvp_icd.x86_64.json', GSETTINGS_BACKEND='memory')
                command = ['dbus-run-session', '--', sys.executable, str(Path(__file__).resolve()), '--private-session', '--binary', str(args.binary.resolve()), '--fixture', str(args.fixture.resolve()), '--output', str(args.output)]
                if args.pdf_only:
                    command.append('--pdf-only')
                if args.media_only:
                    command.append('--media-only')
                subprocess.run(command, env=environment, check=True)
                report(args)
            finally:
                server.terminate()
                server.wait(timeout=5)


if __name__ == '__main__':
    main()
