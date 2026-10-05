#!/usr/bin/env python3
"""Send deterministic UI verification input only to Folio's X11 client window.
No global input, focus change or interaction with another application. Run with a
throwaway --data-dir. This checks native shortcuts and modal text entry.
"""
import argparse
import ctypes as C
import sqlite3
import time
from pathlib import Path

class Key(C.Structure):
    _fields_=[('type',C.c_int),('serial',C.c_ulong),('send_event',C.c_int),('display',C.c_void_p),('window',C.c_ulong),('root',C.c_ulong),('subwindow',C.c_ulong),('time',C.c_ulong),('x',C.c_int),('y',C.c_int),('x_root',C.c_int),('y_root',C.c_int),('state',C.c_uint),('keycode',C.c_uint),('same_screen',C.c_int)]
class Event(C.Union):
    _fields_=[('key',Key),('pad',C.c_long*24)]

def main():
    parser=argparse.ArgumentParser();parser.add_argument('data_dir');args=parser.parse_args();database=Path(args.data_dir)/'notes.sqlite3'
    x=C.CDLL('libX11.so.6');x.XOpenDisplay.restype=C.c_void_p;x.XDefaultRootWindow.argtypes=[C.c_void_p];x.XDefaultRootWindow.restype=C.c_ulong
    x.XQueryTree.argtypes=[C.c_void_p,C.c_ulong,C.POINTER(C.c_ulong),C.POINTER(C.c_ulong),C.POINTER(C.POINTER(C.c_ulong)),C.POINTER(C.c_uint)]
    x.XFetchName.argtypes=[C.c_void_p,C.c_ulong,C.POINTER(C.c_char_p)];x.XFree.argtypes=[C.c_void_p];x.XStringToKeysym.argtypes=[C.c_char_p];x.XStringToKeysym.restype=C.c_ulong;x.XKeysymToKeycode.argtypes=[C.c_void_p,C.c_ulong];x.XKeysymToKeycode.restype=C.c_uint
    x.XSendEvent.argtypes=[C.c_void_p,C.c_ulong,C.c_int,C.c_long,C.POINTER(Event)];x.XFlush.argtypes=[C.c_void_p];x.XCloseDisplay.argtypes=[C.c_void_p]
    d=x.XOpenDisplay(None)
    if not d:raise SystemExit('No X11 display')
    root=x.XDefaultRootWindow(d);found=[]
    def walk(w,depth):
        title=C.c_char_p();x.XFetchName(d,w,C.byref(title));name=title.value.decode(errors='replace') if title.value else ''
        if title:x.XFree(title)
        if name.startswith('Folio'):found.append((depth,w))
        r,p,k,c=C.c_ulong(),C.c_ulong(),C.POINTER(C.c_ulong)(),C.c_uint()
        if x.XQueryTree(d,w,C.byref(r),C.byref(p),C.byref(k),C.byref(c)):
            children=list(k[:c.value]);x.XFree(k)
            for child in children:walk(child,depth+1)
    walk(root,0)
    if not found:raise SystemExit('No Folio client window')
    deepest=max(v[0] for v in found);clients=[w for depth,w in found if depth==deepest]
    if len(clients)!=1:raise SystemExit('Run exactly one Folio X11 window for this test')
    window=clients[0]
    def key(name,state=0):
        code=x.XKeysymToKeycode(d,x.XStringToKeysym(name.encode()))
        for kind,mask in [(2,1),(3,2)]:
            event=Event();event.key=Key(kind,0,1,d,window,root,0,0,100,100,100,100,state,code,1);assert x.XSendEvent(d,window,0,mask,C.byref(event))
        x.XFlush(d);time.sleep(.15)
    def count(table):
        with sqlite3.connect(database) as db:return db.execute(f'SELECT count(*) FROM {table}').fetchone()[0]
    notes=count('notes');key('n',4);time.sleep(.4);assert count('notes')==notes+1,'Ctrl+N must create a note'
    # Modal text entry must accept letters used by tool shortcuts.
    key('f',4);key('p');key('e');key('l');key('Escape');key('s',4)
    assert count('notes')==notes+1,'Text entry must not trigger app shortcuts'
    print('X11_UI_OK: Ctrl+N, modal tool-letter input, Escape, Ctrl+S')
    x.XCloseDisplay(d)

if __name__=='__main__':main()
