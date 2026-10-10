#!/usr/bin/env python3
"""Check resize synchronization on a caller-owned virtual X11 display.

Hide the fixture window to suspend presentation, then request a synchronized
resize. Acknowledgement must wait for rendering after remapping. A resize burst
must settle on its final size and leave Folio responsive.
"""
import argparse
import ctypes as C
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import time

from ui_x11 import Client


class SyncValue(C.Structure):
    _fields_ = [('hi', C.c_int), ('lo', C.c_uint)]


class Message(C.Structure):
    _fields_ = [('type', C.c_int), ('serial', C.c_ulong), ('send_event', C.c_int),
                ('display', C.c_void_p), ('window', C.c_ulong),
                ('message_type', C.c_ulong), ('format', C.c_int),
                ('data', C.c_long * 5)]


class Event(C.Union):
    _fields_ = [('message', Message), ('pad', C.c_long * 24)]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    if os.environ.get('FOLIO_VIRTUAL_DISPLAY') != '1' or os.environ.get('WAYLAND_DISPLAY'):
        parser.error('Use a caller-owned isolated X11 display')
    args.output.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='folio-resize-') as data:
        with (args.output / 'app.log').open('w') as log:
            app = subprocess.Popen([str(args.binary.resolve()), '--data-dir', data, '--new-note'],
                                   stdout=log, stderr=log)
            client = None
            try:
                client = Client(app.pid)
                client.wake_virtual_window()
                time.sleep(.5)
                x = client.x
                x.XMapWindow.argtypes = x.XUnmapWindow.argtypes = [C.c_void_p, C.c_ulong]
                x.XSendEvent.argtypes = [C.c_void_p, C.c_ulong, C.c_int, C.c_long, C.c_void_p]
                ext = C.CDLL('libXext.so.6')
                ext.XSyncQueryCounter.argtypes = [C.c_void_p, C.c_ulong, C.POINTER(SyncValue)]
                atom = x.XInternAtom(client.display, b'_NET_WM_SYNC_REQUEST_COUNTER', 0)
                typ, fmt, count, rest, value = C.c_ulong(), C.c_int(), C.c_ulong(), C.c_ulong(), C.POINTER(C.c_ubyte)()
                x.XGetWindowProperty(client.display, client.window, atom, 0, 1, 0, 0,
                                     C.byref(typ), C.byref(fmt), C.byref(count), C.byref(rest), C.byref(value))
                assert fmt.value == 32 and count.value == 1, 'Missing resize synchronization counter'
                counter = C.cast(value, C.POINTER(C.c_ulong))[0]
                x.XFree(value)

                def sequence():
                    result = SyncValue()
                    assert ext.XSyncQueryCounter(client.display, counter, C.byref(result))
                    return (result.hi << 32) | result.lo

                original = sequence()
                x.XUnmapWindow(client.display, client.window)
                x.XFlush(client.display)
                time.sleep(.3)
                target = original + 1
                event = Event()
                event.message = Message(33, 0, 1, client.display, client.window,
                    x.XInternAtom(client.display, b'WM_PROTOCOLS', 0), 32,
                    (C.c_long * 5)(x.XInternAtom(client.display, b'_NET_WM_SYNC_REQUEST', 0), 0,
                                  target & 0xffffffff, target >> 32, 0))
                assert x.XSendEvent(client.display, client.window, 0, 0, C.byref(event))
                client.resize(1100, 700)
                assert sequence() == original, 'Resize acknowledged before the resized frame was drawn'
                x.XMapWindow(client.display, client.window)
                x.XFlush(client.display)
                client.wake_virtual_window()
                deadline = time.monotonic() + 10
                while sequence() != target and time.monotonic() < deadline:
                    time.sleep(.02)
                assert sequence() == target, 'Rendered resize was never acknowledged'

                started = time.monotonic()
                for index in range(200):
                    x.XResizeWindow(client.display, client.window, 1100 + index % 100, 700 + index % 60)
                x.XResizeWindow(client.display, client.window, 1180, 740)
                x.XFlush(client.display)
                time.sleep(.5)
                assert app.poll() is None, 'Resize burst crashed Folio'
                client.key('s', 4)  # Save after the burst through native event dispatch.
                assert app.poll() is None
                subprocess.run([sys.executable, str(Path(__file__).with_name('capture-x11.py')), str(args.output / 'settled.png'),
                                '--pid', str(app.pid)], check=True)
                from PIL import Image
                with Image.open(args.output / 'settled.png') as image:
                    assert image.size == (1180, 740), image.size
                    assert len(image.getcolors(image.width * image.height)) > 10, 'Settled frame is blank'
                result = {'ack_waits_for_render': True, 'rendered_resize_acknowledged': True,
                          'burst_events': 201, 'final_size': [1180, 740],
                          'responsive_after_burst': True, 'burst_and_capture_seconds': time.monotonic() - started}
                (args.output / 'result.json').write_text(json.dumps(result, indent=2) + '\n')
                print(json.dumps(result))
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


if __name__ == '__main__':
    main()
