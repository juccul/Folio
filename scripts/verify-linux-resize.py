#!/usr/bin/env python3
"""Exercise continuous native Linux window resizing on private Mutter desktops.

Example: /usr/bin/python3 scripts/verify-linux-resize.py --binary target/release/folio
         --output artifacts/validation/linux-resize/current

Requires Mutter, PipeWire, WirePlumber, GI Atspi/GStreamer, and Pillow. Creates
private compositor, session bus, runtime directory, software Vulkan renderer and
fresh Folio library. No physical desktop input or existing notes are accessed.

Captures actual compositor frames, preserves vector-ink pixel dimensions and
placement, and checks that six continuous edge/corner grow/shrink drags leave
notes saved and UI responsive on native Wayland and Xwayland. Sampled timing
includes injected input and capture overhead; it is not physical input latency.
"""
import argparse
import hashlib
import json
import os
import sqlite3
import subprocess
import tempfile
import threading
import time
from pathlib import Path

def run_child(a):
    import gi
    from gi.repository import Gio, GLib
    from PIL import Image, ImageChops
    assert os.environ.get('FOLIO_PRIVATE_COMPOSITOR') == '1' and os.environ['XDG_RUNTIME_DIR'].startswith('/tmp/folio-resize-compositor-')
    app = None
    pipeline = None
    children = []
    remote = None
    cast = None
    frames = []
    latest = [None]
    phase = ['startup']
    marker_initial = [None]
    lock = threading.Lock()
    node = []
    b = Gio.bus_get_sync(Gio.BusType.SESSION, None)
    ctx = GLib.MainContext.default()

    def pump(duration):
        end = time.monotonic() + duration
        while time.monotonic() < end:
            while ctx.iteration(False):
                pass
            time.sleep(0.003)

    def call(service, path, iface, method, args=None):
        return b.call_sync(service, path, iface, method, args, None, Gio.DBusCallFlags.NONE, 5000, None)
    try:
        children.append(subprocess.Popen(['pipewire'], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL))
        time.sleep(0.3)
        children.append(subprocess.Popen(['wireplumber'], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL))
        time.sleep(0.5)
        remote = call('org.gnome.Mutter.RemoteDesktop', '/org/gnome/Mutter/RemoteDesktop', 'org.gnome.Mutter.RemoteDesktop', 'CreateSession').unpack()[0]

        def inp(method, args=None):
            return call('org.gnome.Mutter.RemoteDesktop', remote, 'org.gnome.Mutter.RemoteDesktop.Session', method, args)
        inp('Start')
        inp('NotifyPointerMotionRelative', GLib.Variant('(dd)', (-10000.0, -10000.0)))
        cursor = [0.0, 0.0]

        def move(x, y):
            inp('NotifyPointerMotionRelative', GLib.Variant('(dd)', (x - cursor[0], y - cursor[1])))
            cursor[:] = [x, y]

        def press(down):
            inp('NotifyPointerButton', GLib.Variant('(ib)', (272, down)))
        children.append(subprocess.Popen(['/usr/libexec/at-spi-bus-launcher', '--launch-immediately'], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL))
        time.sleep(0.3)
        addr = call('org.a11y.Bus', '/org/a11y/bus', 'org.a11y.Bus', 'GetAddress').unpack()[0]
        os.environ['AT_SPI_BUS_ADDRESS'] = addr
        gi.require_version('Atspi', '2.0')
        from gi.repository import Atspi
        children.append(subprocess.Popen(['/usr/libexec/at-spi2-registryd'], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL))
        call('org.a11y.Bus', '/org/a11y/bus', 'org.freedesktop.DBus.Properties', 'Set', GLib.Variant('(ssv)', ('org.a11y.Status', 'IsEnabled', GLib.Variant('b', True))))
        gi.require_version('Gst', '1.0')
        from gi.repository import Gst
        Gst.init(None)
        cast = call('org.gnome.Mutter.ScreenCast', '/org/gnome/Mutter/ScreenCast', 'org.gnome.Mutter.ScreenCast', 'CreateSession', GLib.Variant('(a{sv})', ({},))).unpack()[0]
        stream = call('org.gnome.Mutter.ScreenCast', cast, 'org.gnome.Mutter.ScreenCast.Session', 'RecordMonitor', GLib.Variant('(sa{sv})', ('', {'cursor-mode': GLib.Variant('u', 0)}))).unpack()[0]

        def ready(*args):
            node.extend(args[5].unpack())
        b.signal_subscribe(None, 'org.gnome.Mutter.ScreenCast.Stream', 'PipeWireStreamAdded', stream, None, Gio.DBusSignalFlags.NONE, ready, None)
        call('org.gnome.Mutter.ScreenCast', cast, 'org.gnome.Mutter.ScreenCast.Session', 'Start')
        pump(0.5)
        assert node, 'No private PipeWire stream'
        pipeline = Gst.parse_launch(f'pipewiresrc path={node[0]} do-timestamp=true ! videoconvert ! video/x-raw,format=RGB ! appsink name=sink emit-signals=true max-buffers=2 drop=true sync=false')
        sink = pipeline.get_by_name('sink')

        def frame(sink):
            sample = sink.emit('pull-sample')
            buffer = sample.get_buffer()
            caps = sample.get_caps().get_structure(0)
            w, h = (caps.get_value('width'), caps.get_value('height'))
            ok, mapped = buffer.map(Gst.MapFlags.READ)
            if not ok:
                return Gst.FlowReturn.ERROR
            stamp = time.monotonic()
            frame_phase = phase[0]
            # The 1900px RGB monitor has aligned rows; this capture has no padding.
            image = Image.frombytes('RGB', (w, h), mapped.data)
            buffer.unmap(mapped)
            small = image.resize((w // 4, h // 4), Image.Resampling.NEAREST)
            r, g, bl = small.split()
            # Private Mutter wallpaper is blue; Folio's neutral chrome identifies
            # the actual presented client rectangle without trusting UIA prepaint.
            gray = ImageChops.multiply(ImageChops.difference(r, g).point([255 if i < 4 else 0 for i in range(256)]), ImageChops.difference(g, bl).point([255 if i < 4 else 0 for i in range(256)]))
            gray = ImageChops.multiply(gray, r.point([255 if i > 15 else 0 for i in range(256)]))
            rect = gray.getbbox()
            rect = [v * 4 for v in rect] if rect else None
            digest = hashlib.blake2b(small.tobytes(), digest_size=8).hexdigest()
            visible = None
            if rect and marker_initial[0] and (frame_phase not in ['startup', 'settled']):
                original = marker_initial[0]['rect']
                dx = (rect[2] - rect[0] - original[2] + original[0]) / 2
                roi = (int(rect[0] + 480 + dx), int(rect[1] + 240), int(rect[0] + 780 + dx), int(rect[1] + 450))
                ink = image.crop(roi).convert('L').point([255 if i > 80 else 0 for i in range(256)])
                visible = ink.getbbox() is not None
                if len(frames) % 12 == 0 or not visible:
                    small.save(a.output / f'motion-{frame_phase}-{len(frames):04d}.png')
            with lock:
                frames.append({'t': stamp, 'pts_ns': buffer.pts, 'phase': frame_phase, 'rect': rect, 'hash': digest, 'marker_visible': visible})
                latest[0] = image
            return Gst.FlowReturn.OK
        sink.connect('new-sample', frame)
        pipeline.set_state(Gst.State.PLAYING)
        with tempfile.TemporaryDirectory(prefix='folio-resize-data-') as data, (a.output / 'app.log').open('w') as log:
            # Pin the fresh fixture to dark paper for unambiguous white-ink detection.
            with sqlite3.connect(Path(data) / 'notes.sqlite3') as db:
                db.execute('CREATE TABLE settings(key TEXT PRIMARY KEY,data TEXT NOT NULL)')
                db.execute('INSERT INTO settings VALUES(?, ?)',
                           ('preferences', json.dumps({'dark': True, 'follow_system_theme': False})))
            env = dict(os.environ, WAYLAND_DISPLAY=os.environ['WAYLAND_DISPLAY'] if a.backend == 'wayland' else '', GDK_BACKEND='x11')
            app = subprocess.Popen([str(a.binary.resolve()), '--data-dir', data, '--new-note'], stdout=log, stderr=log, env=env)
            target = None
            for _ in range(100):
                pump(0.1)
                desktop = Atspi.get_desktop(0)
                target = next((desktop.get_child_at_index(i) for i in range(desktop.get_child_count()) if 'folio' in desktop.get_child_at_index(i).get_name().lower()), None)
                if target:
                    break
            assert target, 'Folio has no accessible window'

            def walk(n):
                n.clear_cache()
                yield n
                for i in range(n.get_child_count()):
                    yield from walk(n.get_child_at_index(i))

            def button(label):
                return next((n for n in walk(target) if n.get_name() == label))
            pump(1.5)

            def presented():
                with lock:
                    return next((f['rect'] for f in reversed(frames) if f['rect']))

            def save(name):
                with lock:
                    im = latest[0].copy() if latest[0] else None
                if im:
                    im.save(a.output / f'{name}.png')
            initial = presented()
            x0, y0, x1, y1 = initial
            # Draw vector cross/diagonal markers through native event dispatch.
            for sx, sy, dx, dy in [(x0 + 520, y0 + 300, 180, 0), (x0 + 610, y0 + 260, 0, 80), (x0 + 550, y0 + 340, 120, 70)]:
                move(sx, sy)
                pump(0.05)
                press(True)
                for step in range(1, 21):
                    move(sx + dx * step / 20, sy + dy * step / 20)
                    pump(0.006)
                press(False)
                pump(0.1)
            # Relocate Folio's custom pen cursor before the reference capture.
            move(x0 + 1100, y0 + 500)
            pump(0.15)
            move(100, 100)
            pump(0.5)
            save('initial')

            def marker(rect):
                original = initial
                dx = (rect[2] - rect[0] - original[2] + original[0]) / 2
                roi = (int(rect[0] + 480 + dx), int(rect[1] + 240), int(rect[0] + 780 + dx), int(rect[1] + 450))
                with lock:
                    im = latest[0].copy()
                ink = im.crop(roi).convert('L').point([255 if i > 80 else 0 for i in range(256)])
                bbox = ink.getbbox()
                assert bbox, 'Missing vector ink marker'
                return {'width': bbox[2] - bbox[0], 'height': bbox[3] - bbox[1], 'relative_left': bbox[0] + roi[0] - rect[0] - dx, 'relative_top': bbox[1] + roi[1] - rect[1]}
            ink_before = marker(initial)
            assert ink_before['width'] < 250 and ink_before['height'] < 180, 'Fixture requires dark paper and visible ink, not a uniform ROI'
            marker_initial[0] = {'rect': initial, 'ink': ink_before}
            drags = []
            commands = []
            # Both directions test target-capacity reuse and compositor settles.
            for name, edge, dx, dy in [('right-grow', 'right', 160, 0), ('right-shrink', 'right', -160, 0), ('bottom-grow', 'bottom', 0, 120), ('bottom-shrink', 'bottom', 0, -120), ('corner-grow', 'corner', 120, 80), ('corner-shrink', 'corner', -120, -80)]:
                rect = presented()
                x0, y0, x1, y1 = rect
                x, y = (x1 - 2, (y0 + y1) / 2) if edge == 'right' else ((x0 + x1) / 2, y1 - 2) if edge == 'bottom' else (x1 - 2, y1 - 2)
                move(x, y)
                pump(0.15)
                phase[0] = name
                press(True)
                pump(0.06)
                started = time.monotonic()
                local = []
                for step in range(1, 61):
                    tx, ty = (x + dx * step / 60, y + dy * step / 60)
                    move(tx, ty)
                    stamp = time.monotonic()
                    local.append({'t': stamp, 'target_width': x1 - x0 + dx * step / 60, 'target_height': y1 - y0 + dy * step / 60})
                    pump(1 / 60)
                press(False)
                release = time.monotonic()
                pump(0.5)
                final = presented()
                save(name)
                assert app.poll() is None, 'Resize crashed Folio'
                changes = [f for f in frames if f['phase'] == name and f['t'] >= started and (f['t'] <= release) and f['rect']]
                distinct = []
                for f in changes:
                    if not distinct or f['rect'] != distinct[-1]['rect']:
                        distinct.append(f)
                gaps = [(n['t'] - o['t']) * 1000 for o, n in zip(distinct, distinct[1:])]
                lag = []
                for f in distinct:
                    preceding = next((c for c in reversed(local) if c['t'] <= f['t']), None)
                    if preceding:
                        lag.append(max(abs(f['rect'][2] - f['rect'][0] - preceding['target_width']), abs(f['rect'][3] - f['rect'][1] - preceding['target_height'])))
                changed = (final[2] - final[0], final[3] - final[1]) != (x1 - x0, y1 - y0)
                assert changed, f'Native injected pointer edge drag did not resize: {name}: {rect}->{final}'
                ink = marker(final)
                assert abs(ink['width'] - ink_before['width']) <= 2 and abs(ink['height'] - ink_before['height']) <= 2, f'Ink scaled during resize: {ink_before}->{ink}'
                assert abs(ink['relative_left'] - ink_before['relative_left']) <= 6 and abs(ink['relative_top'] - ink_before['relative_top']) <= 6, f'Ink displaced during resize: {ink_before}->{ink}'
                drags.append({'name': name, 'initial_rect': rect, 'final_rect': final, 'pointer_steps': 60, 'injection_duration_ms': (release - started) * 1000, 'captured_frames': len(changes), 'presented_geometry_changes': len(distinct), 'max_captured_geometry_gap_ms': max(gaps, default=0), 'mean_geometry_error_px': sum(lag) / len(lag) if lag else None, 'max_geometry_error_px': max(lag, default=None), 'frames_without_visible_folio': sum((not f['rect'] for f in frames if f['phase'] == name)), 'ink_marker': ink, 'frames_missing_ink_marker': sum((f['marker_visible'] is False for f in frames if f['phase'] == name))})
                commands.extend(local)
                phase[0] = 'settled'
                pump(0.25)
            # Native accessible actions verify responsiveness and durable closure.
            button('Library · Ctrl+Shift+L').get_action_iface().do_action(0)
            pump(0.5)
            assert any((n.get_name() == 'New document' for n in walk(target)))
            save('responsive-library')
            button('Close window').get_action_iface().do_action(0)
            assert app.wait(timeout=10) == 0
            with sqlite3.connect(Path(data) / 'notes.sqlite3') as db:
                assert db.execute('SELECT count(*) FROM objects').fetchone()[0] >= 3, 'Ink was not durably saved'
            result = {'backend': a.backend, 'binary': str(a.binary.resolve()), 'binary_sha256': hashlib.sha256(a.binary.read_bytes()).hexdigest(), 'private_compositor': True, 'software_renderer': True, 'pointer_steps': 360, 'initial_rect': initial, 'ink_marker_before': ink_before, 'ink_alignment_preserved': True, 'drags': drags, 'responsive_after_resize': True, 'close_flushes': True, 'limits': 'Private software-rendered Mutter, D-Bus injected pointer events and sampled compositor buffers; timing includes automation/capture overhead and is not physical input-to-display latency.'}
            (a.output / 'result.json').write_text(json.dumps(result, indent=2) + '\n')
            (a.output / 'frames.json').write_text(json.dumps(frames, indent=2) + '\n')
            (a.output / 'commands.json').write_text(json.dumps(commands, indent=2) + '\n')
            print(json.dumps(result))
    finally:
        # Preserve sampled evidence even if a later geometry assertion fails.
        (a.output / 'frames.json').write_text(json.dumps(frames, indent=2) + '\n')
        if pipeline:
            pipeline.set_state(Gst.State.NULL)
        if cast:
            try:
                call('org.gnome.Mutter.ScreenCast', cast, 'org.gnome.Mutter.ScreenCast.Session', 'Stop')
            except GLib.Error:
                pass
        if remote:
            try:
                inp('Stop')
            except GLib.Error:
                pass
        if app and app.poll() is None:
            app.terminate()
            app.wait(timeout=5)
        for p in reversed(children):
            if p.poll() is None:
                p.terminate()
                p.wait(timeout=5)

def launch(args):
    import shutil
    import signal

    for command in ['dbus-run-session', 'mutter', 'pipewire', 'wireplumber', 'fusermount3']:
        if not shutil.which(command):
            raise RuntimeError(f'Missing required Linux validation tool: {command}')
    if args.output.exists() and any(args.output.iterdir()):
        raise RuntimeError('Use a fresh empty evidence directory')
    args.output.mkdir(parents=True, exist_ok=True)
    backends = ['wayland', 'xwayland'] if args.backend == 'all' else [args.backend]
    for backend in backends:
        with tempfile.TemporaryDirectory(prefix='folio-resize-compositor-') as runtime:
            os.chmod(runtime, 0o700)
            env = dict(os.environ, XDG_RUNTIME_DIR=runtime, PIPEWIRE_RUNTIME_DIR=runtime,
                       XDG_DATA_HOME=str(Path(runtime) / 'data'),
                       XDG_CONFIG_HOME=str(Path(runtime) / 'config'),
                       XDG_CACHE_HOME=str(Path(runtime) / 'cache'),
                       DISPLAY='', WAYLAND_DISPLAY='', AT_SPI_BUS_ADDRESS='',
                       GIO_USE_VFS='local', GTK_USE_PORTAL='0',
                       FOLIO_VIRTUAL_DISPLAY='1', FOLIO_PRIVATE_COMPOSITOR='1',
                       LIBGL_ALWAYS_SOFTWARE='1', GALLIUM_DRIVER='llvmpipe',
                       VK_ICD_FILENAMES='/usr/share/vulkan/icd.d/lvp_icd.x86_64.json')
            with (args.output / f'{backend}-compositor.log').open('w') as log:
                process = subprocess.Popen([
                    'dbus-run-session', '--', 'mutter', '--headless', '--wayland',
                    '--virtual-monitor', '1900x1200', '--', '/usr/bin/python3',
                    str(Path(__file__).resolve()), '--child', '--binary', str(args.binary.resolve()),
                    '--output', str((args.output / backend).resolve()), '--backend', backend,
                ], env=env, stdout=log, stderr=log, start_new_session=True)
                try:
                    code = process.wait(timeout=120)
                    if code:
                        raise RuntimeError(f'{backend} resize test failed; see {log.name}')
                finally:
                    try:
                        os.killpg(process.pid, signal.SIGTERM)
                    except ProcessLookupError:
                        pass
                    if process.poll() is None:
                        process.wait(timeout=5)
                    # Private portal services may leave disconnected FUSE mounts.
                    # Unmount only mounts below this explicitly owned runtime.
                    for mount in ['doc', 'gvfs']:
                        subprocess.run(['fusermount3', '-uz', str(Path(runtime) / mount)],
                                       stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        print(f'Completed {backend} resize validation')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--backend', choices=['wayland', 'xwayland', 'all'], default='all')
    parser.add_argument('--child', action='store_true', help=argparse.SUPPRESS)
    args = parser.parse_args()
    if args.child:
        args.output.mkdir(parents=True, exist_ok=True)
        run_child(args)
    else:
        launch(args)


if __name__ == '__main__':
    main()
