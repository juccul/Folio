#!/usr/bin/env python3
"""Verify the shared native color picker in an owned Xvfb/D-Bus demo session.

Checks real mouse drags, keyboard controls, grayscale hue memory, transparency,
save/cancel semantics and ink/text/paper/theme persistence in both appearances.
Never opens user notes or sends input to the physical desktop.
"""
import argparse
import colorsys
import ctypes as C
import json
import os
import re
from pathlib import Path
import shutil
import sqlite3
import subprocess
import sys
import tempfile
import time

ROOT = Path(__file__).resolve().parents[1]


def session(args):
    assert os.environ.get('FOLIO_COLOR_PICKER_SESSION') == '1'
    assert os.environ.get('FOLIO_VIRTUAL_DISPLAY') == '1' and not os.environ.get('WAYLAND_DISPLAY')
    import gi
    from gi.repository import Gio, GLib
    from ui_x11 import Client
    processes = []
    results = []
    try:
        processes.append(subprocess.Popen(['/usr/libexec/at-spi-bus-launcher', '--launch-immediately'], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL))
        time.sleep(.3)
        bus = Gio.bus_get_sync(Gio.BusType.SESSION, None)
        for _ in range(100):
            if bus.call_sync('org.freedesktop.DBus', '/org/freedesktop/DBus', 'org.freedesktop.DBus', 'NameHasOwner', GLib.Variant('(s)', ('org.a11y.Bus',)), GLib.VariantType.new('(b)'), Gio.DBusCallFlags.NONE, 5000, None).unpack()[0]:
                break
            time.sleep(.1)
        else:
            raise AssertionError('Private accessibility bus did not become ready')
        address = bus.call_sync('org.a11y.Bus', '/org/a11y/bus', 'org.a11y.Bus', 'GetAddress', None, GLib.VariantType.new('(s)'), Gio.DBusCallFlags.NONE, 5000, None).unpack()[0]
        os.environ['AT_SPI_BUS_ADDRESS'] = address
        gi.require_version('Atspi', '2.0')
        from gi.repository import Atspi
        os.environ['GDK_BACKEND'] = 'x11'
        gi.require_version('Gtk', '3.0')
        from gi.repository import Gtk, Gdk
        assert Gtk.init_check([])[0], 'Private GTK clipboard display unavailable'
        clipboard = Gtk.Clipboard.get(Gdk.SELECTION_CLIPBOARD)
        processes.append(subprocess.Popen(['/usr/libexec/at-spi2-registryd'], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL))
        bus.call_sync('org.a11y.Bus', '/org/a11y/bus', 'org.freedesktop.DBus.Properties', 'Set', GLib.Variant('(ssv)', ('org.a11y.Status', 'IsEnabled', GLib.Variant('b', True))), None, Gio.DBusCallFlags.NONE, 5000, None)
        portal = subprocess.Popen([sys.executable, str(ROOT / 'scripts/ui_theme_portal.py')], stdout=subprocess.PIPE, text=True)
        processes.append(portal)
        assert portal.stdout.readline().strip() == 'READY'
        for mode in ('light', 'dark'):
            with tempfile.TemporaryDirectory(prefix='folio-color-picker-') as temporary:
                data = Path(temporary) / 'data'
                shutil.copytree(args.fixture, data, ignore=shutil.ignore_patterns('session.lock', '*.sqlite3-wal', '*.sqlite3-shm'))
                database = data / 'notes.sqlite3'
                with sqlite3.connect(database) as db:
                    row = db.execute("SELECT data FROM settings WHERE key='preferences'").fetchone()
                    preferences = json.loads(row[0]) if row else {}
                    preferences.update(dark=mode == 'dark', follow_system_theme=False, reduce_motion=True, ui_scale=1)
                    db.execute("INSERT OR REPLACE INTO settings(key,data) VALUES('preferences',?)", (json.dumps(preferences),))
                    note = next(id_ for id_, metadata in db.execute('SELECT id,metadata FROM notes') if json.loads(metadata)['title'] == 'Field notes')
                    original_objects = dict(db.execute('SELECT id,data FROM objects ORDER BY id'))
                with (args.output / (mode + '-runtime.log')).open('w') as log:
                    app = subprocess.Popen([str(args.binary.resolve()), '--data-dir', str(data), '--open-note', note], stdout=log, stderr=log)
                    client = None
                    try:
                        client = Client(app.pid)
                        for _ in range(5):
                            client.wake_virtual_window()
                        client.resize(1320, 1000)
                        client.x.XSetInputFocus.argtypes = [C.c_void_p, C.c_ulong, C.c_int, C.c_ulong]
                        client.x.XSetInputFocus(client.display, client.window, 1, 0)
                        client.x.XFlush(client.display)
                        target = None
                        for _ in range(60):
                            desktop = Atspi.get_desktop(0)
                            target = next((desktop.get_child_at_index(i) for i in range(desktop.get_child_count()) if desktop.get_child_at_index(i).get_process_id() == app.pid), None)
                            if target:
                                break
                            time.sleep(.1)
                        assert target, 'No native Folio accessibility tree'

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
                                    time.sleep(.1)

                        def find(label, prefix=False, entry=False):
                            for _ in range(15):
                                matches = [n for n in nodes() if (n.get_name().startswith(label) if prefix else n.get_name() == label) and (not entry or n.get_role() == Atspi.Role.ENTRY)]
                                if matches:
                                    return matches[-1]
                                time.sleep(.08)
                            raise AssertionError('Missing control: ' + label)

                        def click(label, prefix=False):
                            item = find(label, prefix)
                            assert item.get_action_iface().do_action(0), label
                            time.sleep(.2)

                        def preferences_now():
                            with sqlite3.connect(database) as db:
                                return json.loads(db.execute("SELECT data FROM settings WHERE key='preferences'").fetchone()[0])

                        def objects_now():
                            with sqlite3.connect(database) as db:
                                return dict(db.execute('SELECT id,data FROM objects ORDER BY id'))

                        def draft(label):
                            # AccessKit's text input currently exposes a native
                            # editable value but no AT-SPI Text interface. Read
                            # actual field text through this private clipboard.
                            clipboard.set_text('folio-private-field-read-pending', -1)
                            while GLib.MainContext.default().iteration(False):
                                pass
                            rect = find(label, entry=True).get_component_iface().get_extents(Atspi.CoordType.WINDOW)
                            client.click(rect.x + rect.width / 2, rect.y + rect.height / 2)
                            # Repainting a picker can queue an earlier field
                            # focus request. Wait for the clipboard to match
                            # the current independently exposed HSV/alpha.
                            for _ in range(12):
                                client.wake_virtual_window()
                                client.key('a', 4, delay=.08)
                                client.key('c', 4, delay=.2)
                                value = clipboard.wait_for_text()
                                if not value or not re.fullmatch(r'#?[0-9a-fA-F]{6}([0-9a-fA-F]{2})?', value):
                                    continue
                                saturation, brightness = map(int, re.findall(r'(\d+)%', find('Saturation and brightness:', prefix=True).get_name())[:2])
                                hue = int(re.search(r'Hue: (\d+)', find('Hue:', prefix=True).get_name())[1])
                                expected = colorsys.hsv_to_rgb(hue / 360, saturation / 100, brightness / 100)
                                if max(abs(channel - reference * 255) for channel, reference in zip(rgb(value), expected)) > 5:
                                    continue
                                alpha_nodes = [n for n in nodes() if n.get_name().startswith('Opacity:')]
                                if alpha_nodes:
                                    expected_alpha = int(re.search(r'Opacity: (\d+)', alpha_nodes[-1].get_name())[1]) * 2.55
                                    if len(value.lstrip('#')) != 8 or abs(int(value[-2:], 16) - expected_alpha) > 2:
                                        continue
                                return value.lower()
                            raise AssertionError('Private clipboard did not expose the current color draft: ' + str(value))

                        def rgb(value):
                            return tuple(int(value.lstrip('#')[i:i + 2], 16) for i in (0, 2, 4))

                        def fill(label, value):
                            rect = find(label, entry=True).get_component_iface().get_extents(Atspi.CoordType.WINDOW)
                            client.click(rect.x + rect.width / 2, rect.y + rect.height / 2)
                            client.key('a', 4, delay=.03)
                            client.key('BackSpace', delay=.03)
                            for character in value.lstrip('#'):
                                client.key(character, delay=.03)
                            time.sleep(.15)

                        def screenshot(name):
                            client.wake_virtual_window()
                            destination = args.output / (mode + '-' + name + '.png')
                            subprocess.run([sys.executable, str(ROOT / 'scripts/capture-x11.py'), str(destination), '--pid', str(app.pid), '--virtual-display-root'], check=True, stdout=subprocess.DEVNULL)

                        def focus(prefix):
                            assert find(prefix, prefix=True).get_component_iface().grab_focus()
                            for _ in range(30):
                                client.wake_virtual_window()
                                if find(prefix, prefix=True).get_state_set().contains(Atspi.StateType.FOCUSED):
                                    break
                            else:
                                raise AssertionError('Native control did not receive focus: ' + prefix)

                        xtst = C.CDLL('libXtst.so.6')
                        xtst.XTestFakeMotionEvent.argtypes = [C.c_void_p, C.c_int, C.c_int, C.c_int, C.c_ulong]
                        xtst.XTestFakeButtonEvent.argtypes = [C.c_void_p, C.c_uint, C.c_int, C.c_ulong]
                        client.x.XTranslateCoordinates.argtypes = [C.c_void_p, C.c_ulong, C.c_ulong, C.c_int, C.c_int, C.POINTER(C.c_int), C.POINTER(C.c_int), C.POINTER(C.c_ulong)]

                        def motion(x, y):
                            root_x, root_y, child = C.c_int(), C.c_int(), C.c_ulong()
                            assert client.x.XTranslateCoordinates(client.display, client.window, client.root, round(x), round(y), C.byref(root_x), C.byref(root_y), C.byref(child))
                            assert xtst.XTestFakeMotionEvent(client.display, -1, root_x.value, root_y.value, 0)
                            client.x.XFlush(client.display)
                            time.sleep(.03)

                        def drag(prefix, start, end):
                            bounds = find(prefix, prefix=True).get_component_iface().get_extents(Atspi.CoordType.WINDOW)
                            assert bounds.width > 20 and bounds.height > 15
                            points = [(bounds.x + bounds.width * u, bounds.y + bounds.height * v) for u, v in (start, end)]
                            motion(*points[0])
                            assert xtst.XTestFakeButtonEvent(client.display, 1, 1, 0)
                            client.x.XFlush(client.display)
                            for step in range(1, 17):
                                motion(*(a + (b - a) * step / 16 for a, b in zip(*points)))
                            assert xtst.XTestFakeButtonEvent(client.display, 1, 0, 0)
                            client.x.XFlush(client.display)
                            time.sleep(.2)

                        def choose_hsv(label, hue, saturation, value):
                            drag('Hue:', (.1, .5), (hue / 360, .5))
                            drag('Saturation and brightness:', (.25, .5), (saturation, 1 - value))
                            actual = rgb(draft(label))
                            observed = colorsys.rgb_to_hsv(*(channel / 255 for channel in actual))
                            expected_hue = hue % 360
                            distance = abs(observed[0] * 360 - expected_hue)
                            assert min(distance, 360 - distance) < 7, (label, hue, actual, observed)
                            assert abs(observed[1] - saturation) < .04 and abs(observed[2] - value) < .04, (label, actual, observed)
                            return actual

                        click('Custom ink color')
                        before = preferences_now()
                        screenshot('ink-initial')
                        ink = choose_hsv('Custom ink color', 210, .75, .8)
                        screenshot('ink-hue-saturation')
                        assert preferences_now()['default_pen'] == before['default_pen'], 'Draft drag saved ink without Save'
                        click('Cancel')
                        assert preferences_now()['default_pen'] == before['default_pen'], 'Cancel changed ink'
                        assert objects_now() == original_objects, 'Picker interactions changed document objects'

                        click('Custom ink color')
                        click('Choose #FFFFFF')
                        drag('Hue:', (.1, .5), (240 / 360, .5))
                        color = rgb(draft('Custom ink color'))
                        assert color == (255, 255, 255), ('Hue changed white RGB', color)
                        drag('Saturation and brightness:', (.25, .5), (1, .2))
                        color = rgb(draft('Custom ink color'))
                        assert color[0] < 12 and color[1] < 12 and 185 < color[2] < 220, ('White hue memory', color)
                        screenshot('ink-white-hue-memory')
                        focus('Saturation and brightness:')
                        client.key('End')
                        color = rgb(draft('Custom ink color'))
                        assert color == (0, 0, 0), ('End should choose black', color)
                        drag('Hue:', (.2, .5), (60 / 360, .5))
                        drag('Saturation and brightness:', (.5, .5), (1, .2))
                        color = rgb(draft('Custom ink color'))
                        assert color[0] > 185 and color[1] > 185 and color[2] < 12, ('Black hue memory', color)
                        screenshot('ink-black-hue-memory')
                        focus('Saturation and brightness:')
                        client.key('Home')
                        assert rgb(draft('Custom ink color')) == (255, 255, 255)
                        focus('Saturation and brightness:')
                        client.key('Right', 1)
                        client.key('Down', 1)
                        assert rgb(draft('Custom ink color')) != (255, 255, 255), 'SV keyboard did not change draft'
                        focus('Hue:')
                        client.key('Home')
                        client.key('Right', 1)
                        hue_name = find('Hue:', prefix=True).get_name()
                        assert '10' in hue_name, ('Hue Shift+Arrow', hue_name)
                        screenshot('ink-keyboard')
                        # Mouse-down must focus each track itself. These keys
                        # deliberately bypass AT-SPI grab_focus(), so a pointer
                        # focus regression cannot hide behind accessibility.
                        drag('Hue:', (.1, .5), (.4, .5))
                        client.key('Home')
                        client.key('Right', 1)
                        assert find('Hue:', prefix=True).get_name().startswith('Hue: 10 degrees'), 'Pointer Hue keyboard focus lost'
                        drag('Saturation and brightness:', (.5, .5), (.6, .4))
                        client.key('Home')
                        assert find('Saturation and brightness:', prefix=True).get_name().startswith('Saturation and brightness: 0%, 100%'), 'Pointer SV keyboard focus lost'
                        client.key('Right', 1)
                        client.key('Down', 1)
                        assert find('Saturation and brightness:', prefix=True).get_name().startswith('Saturation and brightness: 10%, 90%'), 'Pointer SV Shift+Arrow failed'
                        # Continue dragging beyond the box, then release over
                        # its padding. The draft clamps to the visible edge.
                        drag('Saturation and brightness:', (.5, .5), (1.04, -.04))
                        assert find('Saturation and brightness:', prefix=True).get_name().startswith('Saturation and brightness: 100%, 100%'), 'Drag outside did not clamp'
                        after_release = draft('Custom ink color')
                        rect = find('Saturation and brightness:', prefix=True).get_component_iface().get_extents(Atspi.CoordType.WINDOW)
                        motion(rect.x + rect.width * .2, rect.y + rect.height * .8)
                        assert draft('Custom ink color') == after_release, 'Moving after release changed draft'
                        saved_ink = choose_hsv('Custom ink color', 210, .75, .8)
                        # Re-select the same SV point to leave the picker track
                        # pointer focused. Enter confirms without refocusing
                        # the HEX field or clicking the Save button.
                        drag('Saturation and brightness:', (.25, .5), (.75, .2))
                        client.key('Return')
                        assert not any(n.get_role() == Atspi.Role.DIALOG and n.get_name() == 'Custom ink color' for n in nodes()), 'Enter did not confirm the pointer-focused picker'
                        time.sleep(.3)
                        assert tuple(preferences_now()['default_pen']['color'][key] for key in 'rgb') == saved_ink
                        assert objects_now() == original_objects, 'Saving pen color rewrote existing ink'

                        print('COLOR_PICKER_INK_OK:', mode, flush=True)
                        click('Settings')
                        click('Appearance')
                        click('Paper follows appearance: On')
                        click('Custom paper color')
                        saved_paper = choose_hsv('Paper color', 35, .25, .95)
                        screenshot('paper-color')
                        click('Save')
                        time.sleep(.3)
                        assert tuple(preferences_now()['appearance']['canvas_color'][key] for key in 'rgb') == saved_paper
                        click('Paper follows appearance: Off')
                        click('Edit colors…')
                        click('Customize Borders')
                        fill('Theme color', '80808080')
                        alpha_before = draft('Theme color')[-2:]
                        choose_hsv('Theme color', 160, .6, .75)
                        assert draft('Theme color')[-2:] == alpha_before, 'Hue/SV drag changed alpha'
                        drag('Opacity:', (.1, .5), (.4, .5))
                        client.key('Home')
                        assert find('Opacity:', prefix=True).get_name().startswith('Opacity: 0%'), 'Pointer opacity keyboard focus lost'
                        assert draft('Theme color').endswith('00')
                        focus('Opacity:')
                        client.key('Right', 1)
                        assert 20 <= int(draft('Theme color')[-2:], 16) <= 30
                        drag('Opacity:', (.1, .5), (.5, .5))
                        theme_draft = draft('Theme color')
                        assert 120 <= int(theme_draft[-2:], 16) <= 135
                        screenshot('theme-opacity')
                        click('Save')
                        time.sleep(.3)
                        assert preferences_now()['appearance'][mode]['border'] == int(theme_draft.lstrip('#'), 16)
                        theme_before_cancel = preferences_now()['appearance'][mode].copy()
                        click('Customize Primary')
                        assert not any(n.get_name().startswith('Opacity:') for n in nodes()), 'Opaque theme token exposed opacity'
                        theme_color = choose_hsv('Theme color', 280, .7, .85)
                        screenshot('theme-color')
                        click('Cancel')
                        assert preferences_now()['appearance'][mode] == theme_before_cancel, 'Cancel changed theme token'
                        print('COLOR_PICKER_PAPER_THEME_OK:', mode, flush=True)
                        assert objects_now() == original_objects, 'Paper/theme edits rewrote document content'
                        click('Close settings')

                        if not any(n.get_name() == 'Go to page 2' for n in nodes()):
                            click('Page thumbnails', prefix=True)
                        click('Go to page 2')
                        client.key('a', 4)
                        click('Text color')
                        saved_text = choose_hsv('Text color', 320, .7, .8)
                        screenshot('text-color')
                        click('Save')
                        client.key('s', 4)
                        time.sleep(.5)
                        changed = objects_now()
                        modified = [id_ for id_, value in changed.items() if value != original_objects[id_]]
                        assert modified, 'Text color was not persisted'
                        for id_ in modified:
                            current = json.loads(changed[id_])
                            original = json.loads(original_objects[id_])
                            assert set(current) == {'Text'}, 'Text picker changed non-text objects'
                            assert tuple(current['Text']['color'][key] for key in 'rgb') == saved_text
                            current['Text']['color'] = original['Text']['color']
                            assert current == original, 'Text color edit changed unrelated text attributes'
                        client.key('z', 4)
                        client.key('s', 4)
                        time.sleep(.5)
                        assert objects_now() == original_objects, 'Undo did not restore original text colors'

                        print('COLOR_PICKER_TEXT_OK:', mode, flush=True)
                        client.resize(1000, 620)
                        click('Custom ink color')
                        for _ in range(12):
                            save = find('Save').get_component_iface().get_extents(Atspi.CoordType.WINDOW)
                            if 0 <= save.y and save.y + save.height <= 620:
                                break
                            client.click(730, 390, button=5, delay=.06)
                        assert 0 <= save.y and save.y + save.height <= 620, 'Compact picker Save is unreachable'
                        screenshot('picker-compact')
                        click('Cancel')
                        assert objects_now() == original_objects
                        results.append({'mode': mode, 'mouse_hue_sv': True, 'grayscale_hue_memory': True, 'keyboard_controls': True, 'pointer_keyboard_focus': True, 'drag_outside_clamp_release': True, 'alpha_preserved': True, 'save_cancel': True, 'pointer_enter_confirmation': True, 'ink_paper_theme_text_persistence': True, 'document_preservation': True, 'compact_footer_reachable': True})
                        (args.output / (mode + '-failure.png')).unlink(missing_ok=True)
                        (args.output / (mode + '-failure-tree.json')).unlink(missing_ok=True)
                        print('COLOR_PICKER_NATIVE_OK:', mode, flush=True)
                    except Exception:
                        if client:
                            screenshot('failure')
                            (args.output / (mode + '-failure-tree.json')).write_text(json.dumps([{'name': n.get_name(), 'role': n.get_role_name()} for n in nodes()], indent=2))
                        raise
                    finally:
                        if client:
                            client.close()
                        app.terminate()
                        try:
                            app.wait(timeout=5)
                        except subprocess.TimeoutExpired:
                            app.kill()
                            app.wait()
        (args.output / 'results.json').write_text(json.dumps(results, indent=2) + '\n')
    finally:
        for process in reversed(processes):
            process.terminate()
            process.wait(timeout=5)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, default=ROOT / 'target/debug/folio')
    parser.add_argument('--fixture', type=Path, default=ROOT / 'artifacts/validation/performance-pass/ui-fixture')
    parser.add_argument('--output', type=Path, default=ROOT / 'artifacts/color-picker/native')
    parser.add_argument('--xvfb', type=Path, default=ROOT / 'artifacts/build-tools/Xvfb')
    parser.add_argument('--private-session', action='store_true', help=argparse.SUPPRESS)
    args = parser.parse_args()
    args.output = args.output.resolve()
    args.output.mkdir(parents=True, exist_ok=True)
    if args.private_session:
        session(args)
        return
    with tempfile.TemporaryDirectory(prefix='folio-color-picker-display-') as runtime:
        with (args.output / 'xvfb.log').open('w') as log:
            server = subprocess.Popen([str(args.xvfb), '-displayfd', '1', '-screen', '0', '1600x1200x24', '-nolisten', 'tcp', '-ac'], stdout=subprocess.PIPE, stderr=log, text=True)
            try:
                display = server.stdout.readline().strip()
                assert display, 'Private Xvfb failed to start'
                environment = dict(os.environ, DISPLAY=':' + display, WAYLAND_DISPLAY='', XDG_RUNTIME_DIR=runtime, FOLIO_VIRTUAL_DISPLAY='1', FOLIO_COLOR_PICKER_SESSION='1', VK_ICD_FILENAMES='/usr/share/vulkan/icd.d/lvp_icd.x86_64.json', GSETTINGS_BACKEND='memory')
                subprocess.run(['dbus-run-session', '--', sys.executable, str(Path(__file__).resolve()), '--private-session', '--binary', str(args.binary.resolve()), '--fixture', str(args.fixture.resolve()), '--output', str(args.output)], env=environment, check=True)
            finally:
                server.terminate()
                server.wait(timeout=5)


if __name__ == '__main__':
    main()
