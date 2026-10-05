"""Native UI verification helpers, scoped to one Folio process on a virtual display.

No global focus, XTest injection, tablet access, or other application interaction.
"""
import ctypes as C
import os
import time

class Key(C.Structure):
    _fields_ = [('type', C.c_int), ('serial', C.c_ulong), ('send_event', C.c_int), ('display', C.c_void_p), ('window', C.c_ulong), ('root', C.c_ulong), ('subwindow', C.c_ulong), ('time', C.c_ulong), ('x', C.c_int), ('y', C.c_int), ('x_root', C.c_int), ('y_root', C.c_int), ('state', C.c_uint), ('keycode', C.c_uint), ('same_screen', C.c_int)]

class Map(C.Structure):
    _fields_ = [('type', C.c_int), ('serial', C.c_ulong), ('send_event', C.c_int), ('display', C.c_void_p), ('event', C.c_ulong), ('window', C.c_ulong), ('override_redirect', C.c_int)]

class Event(C.Union):
    _fields_ = [('key', Key), ('map', Map), ('pad', C.c_long * 24)]

class Client:
    def __init__(self, pid):
        self.x = x = C.CDLL('libX11.so.6')
        x.XOpenDisplay.restype = C.c_void_p
        x.XDefaultRootWindow.argtypes = [C.c_void_p]; x.XDefaultRootWindow.restype = C.c_ulong
        x.XQueryTree.argtypes = [C.c_void_p, C.c_ulong, C.POINTER(C.c_ulong), C.POINTER(C.c_ulong), C.POINTER(C.POINTER(C.c_ulong)), C.POINTER(C.c_uint)]
        x.XFetchName.argtypes = [C.c_void_p, C.c_ulong, C.POINTER(C.c_char_p)]
        x.XInternAtom.argtypes = [C.c_void_p, C.c_char_p, C.c_int]; x.XInternAtom.restype = C.c_ulong
        x.XGetWindowProperty.argtypes = [C.c_void_p, C.c_ulong, C.c_ulong, C.c_long, C.c_long, C.c_int, C.c_ulong, C.POINTER(C.c_ulong), C.POINTER(C.c_int), C.POINTER(C.c_ulong), C.POINTER(C.c_ulong), C.POINTER(C.POINTER(C.c_ubyte))]
        x.XSendEvent.argtypes = [C.c_void_p, C.c_ulong, C.c_int, C.c_long, C.POINTER(Event)]
        x.XStringToKeysym.argtypes = [C.c_char_p]; x.XStringToKeysym.restype = C.c_ulong
        x.XKeysymToKeycode.argtypes = [C.c_void_p, C.c_ulong]; x.XKeysymToKeycode.restype = C.c_uint
        x.XFlush.argtypes = [C.c_void_p]; x.XFree.argtypes = [C.c_void_p]
        x.XResizeWindow.argtypes = [C.c_void_p, C.c_ulong, C.c_uint, C.c_uint]
        x.XCloseDisplay.argtypes = [C.c_void_p]
        self.display = x.XOpenDisplay(None)
        if not self.display: raise RuntimeError('No virtual X11 display')
        self.root = x.XDefaultRootWindow(self.display)
        atom = x.XInternAtom(self.display, b'_NET_WM_PID', 0)
        def find(window):
            name = C.c_char_p(); x.XFetchName(self.display, window, C.byref(name))
            label = name.value.decode(errors='replace') if name.value else ''; x.XFree(name)
            if label.startswith('Folio'):
                typ, fmt, count, remaining, data = C.c_ulong(), C.c_int(), C.c_ulong(), C.c_ulong(), C.POINTER(C.c_ubyte)()
                x.XGetWindowProperty(self.display, window, atom, 0, 1, 0, 0, C.byref(typ), C.byref(fmt), C.byref(count), C.byref(remaining), C.byref(data))
                matches = fmt.value == 32 and count.value == 1 and C.cast(data, C.POINTER(C.c_ulong))[0] == pid
                x.XFree(data)
                if matches: return window
            root, parent, children, count = C.c_ulong(), C.c_ulong(), C.POINTER(C.c_ulong)(), C.c_uint()
            if x.XQueryTree(self.display, window, C.byref(root), C.byref(parent), C.byref(children), C.byref(count)):
                values = list(children[:count.value]); x.XFree(children)
                for child in values:
                    match = find(child)
                    if match: return match
        self.window = None
        for _ in range(50):
            self.window = find(self.root)
            if self.window: break
            time.sleep(.1)
        if not self.window:
            self.close(); raise RuntimeError(f'No Folio window owned by PID {pid}')

    def wake_virtual_window(self):
        # Some bare servers miss GPUI's initial refresh-loop transition. Replay
        # the notification only in an explicitly authorized synthetic session.
        if os.environ.get('FOLIO_VIRTUAL_DISPLAY') != '1':
            raise RuntimeError('Map replay requires FOLIO_VIRTUAL_DISPLAY=1')
        event = Event(); event.map = Map(19, 0, 1, self.display, self.window, self.window, 0)
        self.x.XSendEvent(self.display, self.window, 0, 1 << 17, C.byref(event))
        self.x.XFlush(self.display); time.sleep(.2)

    def key(self, name, modifiers=0, delay=.12):
        code = self.x.XKeysymToKeycode(self.display, self.x.XStringToKeysym(name.encode()))
        for kind, mask in [(2, 1), (3, 2)]:
            event = Event(); event.key = Key(kind, 0, 1, self.display, self.window, self.root, 0, 0, 20, 20, 20, 20, modifiers, code, 1)
            assert self.x.XSendEvent(self.display, self.window, 0, mask, C.byref(event))
        self.x.XFlush(self.display); time.sleep(delay)

    def resize(self, width, height):
        self.x.XResizeWindow(self.display, self.window, width, height)
        self.x.XFlush(self.display); time.sleep(.3)

    def close(self):
        if self.display:
            self.x.XCloseDisplay(self.display); self.display = None
