#!/usr/bin/env python3
"""Capture only Folio's X11 client window (never the screen). Requires Pillow.
Run Folio with WAYLAND_DISPLAY= on an existing X11/Xwayland display first.
"""
import argparse
import os
import ctypes as C
from ctypes.util import find_library
from PIL import Image

class XImage(C.Structure):
    _fields_ = [("width", C.c_int), ("height", C.c_int), ("xoffset", C.c_int), ("format", C.c_int),
                ("data", C.c_void_p), ("byte_order", C.c_int), ("bitmap_unit", C.c_int),
                ("bitmap_bit_order", C.c_int), ("bitmap_pad", C.c_int), ("depth", C.c_int),
                ("bytes_per_line", C.c_int), ("bits_per_pixel", C.c_int),
                ("red_mask", C.c_ulong), ("green_mask", C.c_ulong), ("blue_mask", C.c_ulong)]

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("output")
    parser.add_argument("--pid", type=int, help="Capture only the window owned by this Folio process")
    parser.add_argument("--virtual-display-root", action="store_true", help="Read the client rectangle from the isolated test display framebuffer")
    args = parser.parse_args()
    if args.virtual_display_root and (os.environ.get("FOLIO_VIRTUAL_DISPLAY") != "1" or not args.pid or os.environ.get("WAYLAND_DISPLAY")):
        parser.error("Framebuffer capture requires an isolated X11 test display and an exact Folio PID")
    x = C.CDLL(find_library("X11"))
    x.XOpenDisplay.restype = C.c_void_p
    x.XDefaultRootWindow.argtypes = [C.c_void_p]
    x.XDefaultRootWindow.restype = C.c_ulong
    x.XQueryTree.argtypes = [C.c_void_p, C.c_ulong, C.POINTER(C.c_ulong), C.POINTER(C.c_ulong), C.POINTER(C.POINTER(C.c_ulong)), C.POINTER(C.c_uint)]
    x.XFetchName.argtypes = [C.c_void_p, C.c_ulong, C.POINTER(C.c_char_p)]
    x.XFree.argtypes = [C.c_void_p]
    x.XGetGeometry.argtypes = [C.c_void_p, C.c_ulong, C.POINTER(C.c_ulong), C.POINTER(C.c_int), C.POINTER(C.c_int), C.POINTER(C.c_uint), C.POINTER(C.c_uint), C.POINTER(C.c_uint), C.POINTER(C.c_uint)]
    x.XGetImage.argtypes = [C.c_void_p, C.c_ulong, C.c_int, C.c_int, C.c_uint, C.c_uint, C.c_ulong, C.c_int]
    x.XGetImage.restype = C.POINTER(XImage)
    x.XDestroyImage.argtypes = [C.POINTER(XImage)]
    x.XCloseDisplay.argtypes = [C.c_void_p]
    display = x.XOpenDisplay(None)
    if not display:
        raise SystemExit("No X11 display")
    def find(window):
        title = C.c_char_p()
        x.XFetchName(display, window, C.byref(title))
        name = title.value.decode(errors="replace") if title.value else ""
        if title:
            x.XFree(title)
        if name.startswith("Folio"):
            return window
        root, parent, children, count = C.c_ulong(), C.c_ulong(), C.POINTER(C.c_ulong)(), C.c_uint()
        if x.XQueryTree(display, window, C.byref(root), C.byref(parent), C.byref(children), C.byref(count)):
            values = [children[i] for i in range(count.value)]
            if children:
                x.XFree(children)
            for child in values:
                found = find(child)
                if found:
                    return found
        return None
    owned = None
    if args.pid:
        from ui_x11 import Client
        owned = Client(args.pid)
        window = owned.window
    else:
        window = find(x.XDefaultRootWindow(display))
    if not window:
        raise SystemExit("No Folio X11 window")
    root, left, top, width, height, border, depth = C.c_ulong(), C.c_int(), C.c_int(), C.c_uint(), C.c_uint(), C.c_uint(), C.c_uint()
    x.XGetGeometry(display, window, C.byref(root), C.byref(left), C.byref(top), C.byref(width), C.byref(height), C.byref(border), C.byref(depth))
    drawable, capture_x, capture_y = window, 0, 0
    if args.virtual_display_root:
        x.XTranslateCoordinates.argtypes = [C.c_void_p, C.c_ulong, C.c_ulong, C.c_int, C.c_int, C.POINTER(C.c_int), C.POINTER(C.c_int), C.POINTER(C.c_ulong)]
        drawable = x.XDefaultRootWindow(display)
        target_x, target_y, child = C.c_int(), C.c_int(), C.c_ulong()
        assert x.XTranslateCoordinates(display, window, drawable, 0, 0, C.byref(target_x), C.byref(target_y), C.byref(child))
        capture_x, capture_y = target_x.value, target_y.value
    image = x.XGetImage(display, drawable, capture_x, capture_y, width, height, C.c_ulong(-1), 2)
    if not image:
        raise SystemExit("Folio window capture failed")
    p = image.contents
    if p.bits_per_pixel != 32:
        raise SystemExit("Capture expects a 32-bit X11 drawable")
    data = C.string_at(p.data, p.bytes_per_line * p.height)
    screenshot = Image.frombytes("RGB", (p.width, p.height), data, "raw", "BGRX", p.bytes_per_line)
    screenshot.save(args.output)
    x.XDestroyImage(image)
    x.XCloseDisplay(display)
    if owned: owned.close()
    print(f"Captured Folio's {width.value} × {height.value} client window: {args.output}")

if __name__ == "__main__":
    main()
