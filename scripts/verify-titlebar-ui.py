#!/usr/bin/env python3
"""Exercise Folio window controls on an isolated Mutter/Xwayland desktop."""
import argparse
import ctypes as C
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
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary',type=Path,required=True)
    parser.add_argument('--fixture',type=Path,required=True)
    parser.add_argument('--output',type=Path,required=True)
    parser.add_argument('--wayland',action='store_true',help='Verify native Wayland controls on the same private compositor')
    args=parser.parse_args()
    assert os.environ.get('FOLIO_PRIVATE_COMPOSITOR')=='1' and os.environ.get('FOLIO_VIRTUAL_DISPLAY')=='1'
    # The launcher owns this compositor, session bus and runtime directory.
    assert os.environ['XDG_RUNTIME_DIR'].startswith('/tmp/folio-titlebar-')
    wayland_display=os.environ['WAYLAND_DISPLAY']
    os.environ['WAYLAND_DISPLAY']=''
    os.environ['GDK_BACKEND']='x11'
    os.environ['GTK_USE_PORTAL']='0'
    os.environ['GIO_USE_VFS']='local'
    args.output.mkdir(parents=True,exist_ok=True)
    processes=[];app=client=session=bus=None
    try:
        processes.append(subprocess.Popen(['/usr/libexec/at-spi-bus-launcher','--launch-immediately'],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL))
        time.sleep(.3)
        bus=Gio.bus_get_sync(Gio.BusType.SESSION,None)
        session=bus.call_sync('org.gnome.Mutter.RemoteDesktop','/org/gnome/Mutter/RemoteDesktop','org.gnome.Mutter.RemoteDesktop','CreateSession',None,None,Gio.DBusCallFlags.NONE,5000,None).unpack()[0]
        def input_call(method,parameters=None):
            return bus.call_sync('org.gnome.Mutter.RemoteDesktop',session,'org.gnome.Mutter.RemoteDesktop.Session',method,parameters,None,Gio.DBusCallFlags.NONE,5000,None)
        input_call('Start')
        input_call('NotifyPointerMotionRelative',GLib.Variant('(dd)',(-10000.,-10000.)))
        address=bus.call_sync('org.a11y.Bus','/org/a11y/bus','org.a11y.Bus','GetAddress',None,GLib.VariantType.new('(s)'),Gio.DBusCallFlags.NONE,5000,None).unpack()[0]
        os.environ['AT_SPI_BUS_ADDRESS']=address
        gi.require_version('Atspi','2.0');from gi.repository import Atspi
        gi.require_version('Gtk','3.0');from gi.repository import Gtk,Gdk
        assert Gtk.init_check([])[0]
        processes.append(subprocess.Popen(['/usr/libexec/at-spi2-registryd'],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL))
        bus.call_sync('org.a11y.Bus','/org/a11y/bus','org.freedesktop.DBus.Properties','Set',GLib.Variant('(ssv)',('org.a11y.Status','IsEnabled',GLib.Variant('b',True))),None,Gio.DBusCallFlags.NONE,5000,None)
        with tempfile.TemporaryDirectory(prefix='folio-titlebar-data-') as temporary:
            data=Path(temporary)/'data'
            shutil.copytree(args.fixture,data,ignore=shutil.ignore_patterns('session.lock','*.sqlite3-wal','*.sqlite3-shm'))
            database=data/'notes.sqlite3'
            with sqlite3.connect(database) as db: note=db.execute('SELECT id FROM notes LIMIT 1').fetchone()[0]
            with (args.output/'runtime.log').open('w') as log:
                env=dict(os.environ,WAYLAND_DISPLAY=wayland_display if args.wayland else '')
                app=subprocess.Popen([str(args.binary.resolve()),'--data-dir',str(data),'--open-note',note],stdout=log,stderr=log,env=env)
                if not args.wayland: client=Client(app.pid)
                time.sleep(.8)
                target=None
                for _ in range(100):
                    desktop=Atspi.get_desktop(0)
                    target=next((desktop.get_child_at_index(i) for i in range(desktop.get_child_count()) if 'folio' in desktop.get_child_at_index(i).get_name().lower()),None)
                    if target:break
                    time.sleep(.1)
                assert target
                def walk(node):
                    node.clear_cache();yield node
                    for i in range(node.get_child_count()):yield from walk(node.get_child_at_index(i))
                def nodes():
                    while GLib.MainContext.default().iteration(False):pass
                    return list(walk(target))
                def button(label):
                    return next(n for n in nodes() if n.get_role()==Atspi.Role.PUSH_BUTTON and n.get_name()==label)
                def click(label):
                    assert button(label).get_action_iface().do_action(0);time.sleep(.4)
                def bounds(label):
                    r=button(label).get_component_iface().get_extents(Atspi.CoordType.WINDOW)
                    return (r.x,r.y,r.width,r.height)
                def wait(label):
                    for _ in range(80):
                        if any(n.get_name()==label for n in nodes()):return
                        time.sleep(.05)
                    raise AssertionError(f'Missing {label}: {[n.get_name() for n in nodes()]}')
                def picker_follows_tabs():
                    tab_ends=[]
                    for node in nodes():
                        if node.get_role()==Atspi.Role.PUSH_BUTTON and node.get_name()=='Close tab':
                            r=node.get_component_iface().get_extents(Atspi.CoordType.WINDOW)
                            tab_ends.append(r.x+r.width)
                    assert tab_ends
                    picker=bounds('Open or create a document · Ctrl+T')
                    assert 8 <= picker[0]-max(tab_ends) <= 24,(picker,tab_ends)
                def library_starts_with_navigation():
                    assert bounds('Documents')[1] < 80,bounds('Documents')
                if args.wayland:
                    wait('Maximize window')
                    order=[bounds(label) for label in ['Open or create a document · Ctrl+T','Minimize window','Maximize window','Close window']]
                    assert all(r[1]<40 for r in order) and all(a[0]+a[2]<b[0] for a,b in zip(order,order[1:]))
                    picker_follows_tabs()
                    click('Maximize window');wait('Restore window');click('Restore window');wait('Maximize window')
                    click('Open or create a document · Ctrl+T');wait('Cancel')
                    assert button('Close window').get_state_set().contains(Atspi.StateType.ENABLED)
                    click('Cancel');click('Library · Ctrl+Shift+L');wait('New document')
                    assert bounds('Close window')[1]<40
                    library_starts_with_navigation()
                    click('New document');click('Close window')
                    assert app.wait(timeout=5)==0
                    with sqlite3.connect(database) as db:assert db.execute('SELECT count(*) FROM notes').fetchone()[0]>=2
                    result={name:True for name in ['native_wayland','picker_follows_tabs','sidebar_navigation','control_order','maximize_restore','controls_available_in_dialog','library_titlebar','close_flushes_and_exits','durable_note']}
                    (args.output/'result.json').write_text(json.dumps(result,indent=2)+'\n');print(json.dumps(result))
                    return
                x=client.x
                x.XTranslateCoordinates.argtypes=[C.c_void_p,C.c_ulong,C.c_ulong,C.c_int,C.c_int,C.POINTER(C.c_int),C.POINTER(C.c_int),C.POINTER(C.c_ulong)]
                x.XGetGeometry.argtypes=[C.c_void_p,C.c_ulong,C.POINTER(C.c_ulong),C.POINTER(C.c_int),C.POINTER(C.c_int),C.POINTER(C.c_uint),C.POINTER(C.c_uint),C.POINTER(C.c_uint),C.POINTER(C.c_uint)]
                x.XMapWindow.argtypes=[C.c_void_p,C.c_ulong]
                def geometry():
                    root,left,top,w,h,b,d=C.c_ulong(),C.c_int(),C.c_int(),C.c_uint(),C.c_uint(),C.c_uint(),C.c_uint()
                    assert x.XGetGeometry(client.display,client.window,C.byref(root),C.byref(left),C.byref(top),C.byref(w),C.byref(h),C.byref(b),C.byref(d))
                    rx,ry,child=C.c_int(),C.c_int(),C.c_ulong()
                    assert x.XTranslateCoordinates(client.display,client.window,client.root,0,0,C.byref(rx),C.byref(ry),C.byref(child))
                    return (rx.value,ry.value,w.value,h.value)
                def property32(name):
                    atom=x.XInternAtom(client.display,name.encode(),0)
                    typ,fmt,count,rest,data=C.c_ulong(),C.c_int(),C.c_ulong(),C.c_ulong(),C.POINTER(C.c_ubyte)()
                    x.XGetWindowProperty(client.display,client.window,atom,0,64,0,0,C.byref(typ),C.byref(fmt),C.byref(count),C.byref(rest),C.byref(data))
                    values=list(C.cast(data,C.POINTER(C.c_ulong))[:count.value]) if fmt.value==32 else []
                    x.XFree(data);return values
                pointer=[0.,0.]
                def move(px,py):
                    input_call('NotifyPointerMotionRelative',GLib.Variant('(dd)',(float(px)-pointer[0],float(py)-pointer[1])))
                    pointer[:]=[float(px),float(py)];time.sleep(.05)
                def press(state):
                    input_call('NotifyPointerButton',GLib.Variant('(ib)',(272,bool(state))));time.sleep(.08)
                def drag(px,py,dx,dy):
                    gx,gy,_,_=geometry();move(gx+px,gy+py);press(1)
                    for step in range(1,9):move(gx+px+dx*step/8,gy+py+dy*step/8)
                    press(0);time.sleep(.3)
                def capture(name):
                    subprocess.run([sys.executable,'scripts/capture-x11.py',str(args.output/f'{name}.png'),'--pid',str(app.pid)],check=True)
                def object_rows():
                    with sqlite3.connect(database) as db:return list(db.execute('SELECT id,data FROM objects ORDER BY id'))
                wait('Minimize window')
                order=[bounds(label) for label in ['Open or create a document · Ctrl+T','Minimize window','Maximize window','Close window']]
                assert all(r[1]<40 and r[3]>=28 for r in order),order
                assert all(a[0]+a[2]<b[0] for a,b in zip(order,order[1:])),order
                picker_follows_tabs()
                motif=property32('_MOTIF_WM_HINTS');assert motif[0]&2 and motif[2]==0,motif
                initial_objects=object_rows()
                original=geometry();capture('editor')
                drag(600,20,-25,-20)
                moved=geometry();assert moved[:2]!=original[:2],(original,moved)
                assert object_rows()==initial_objects,'Moving titlebar edited notes'
                # A tab press/drag must not hand movement to the compositor.
                tab=next(n for n in nodes() if n.get_name().startswith('Open Math solver verification'))
                r=tab.get_component_iface().get_extents(Atspi.CoordType.WINDOW)
                before=geometry();drag(r.x+r.width/2,r.y+r.height/2,30,0)
                assert geometry()==before,'Dragging a tab moved the window'
                click('Open or create a document · Ctrl+T');click('＋  Create new document')
                for char in 'untitled note':client.key('space' if char==' ' else char.lower(),1 if char.isupper() else 0)
                click('Create notebook');wait('Open untitled note')
                source=bounds('Open Math solver verification');destination=bounds('Open untitled note')
                before=geometry()
                drag(source[0]+source[2]/2,source[1]+source[3]/2,
                     destination[0]+destination[2]/2-source[0]-source[2]/2,0)
                assert bounds('Open Math solver verification')[0]>bounds('Open untitled note')[0],'Tabs did not reorder'
                assert geometry()==before,'Reordering tabs moved the window'
                picker_follows_tabs()
                capture('reordered-tabs')
                # Overflow must leave the picker visible and usable, while the
                # window controls remain anchored to the right.
                for _ in range(6):
                    click('Open or create a document · Ctrl+T');click('＋  Create new document')
                    for char in 'untitled note':client.key('space' if char==' ' else char.lower(),1 if char.isupper() else 0)
                    click('Create notebook')
                picker=bounds('Open or create a document · Ctrl+T')
                controls=bounds('Minimize window');close=bounds('Close window')
                assert 0 < picker[0] and picker[0]+picker[2] < controls[0],(picker,controls)
                assert 0 <= geometry()[2]-close[0]-close[2] <= 20,(geometry(),close)
                closing=[n for n in nodes() if n.get_role()==Atspi.Role.PUSH_BUTTON and n.get_name()=='Close tab']
                active_close=closing[-1].get_component_iface().get_extents(Atspi.CoordType.WINDOW)
                assert 0 <= active_close.x and active_close.x+active_close.width < picker[0],(active_close,picker)
                capture('overflow-tabs')
                click('Open or create a document · Ctrl+T');wait('Cancel');click('Cancel')
                for _ in range(6):
                    closing=[n for n in nodes() if n.get_role()==Atspi.Role.PUSH_BUTTON and n.get_name()=='Close tab']
                    assert closing[-1].get_action_iface().do_action(0);time.sleep(.4)
                picker_follows_tabs()
                click('Maximize window');wait('Restore window');maximized=geometry()
                assert maximized[2]>before[2] or maximized[3]>before[3],(before,maximized)
                capture('maximized');click('Restore window');wait('Maximize window')
                restored=geometry();assert restored[2:]==before[2:],(before,restored)
                gx,gy,_,_=geometry();move(gx+600,gy+20)
                press(1);press(0);press(1);press(0);wait('Restore window')
                click('Restore window');wait('Maximize window')
                before=geometry();drag(before[2]-2,400,45,0)
                assert geometry()[2]>before[2],'Client edge did not resize the window'
                click('Minimize window');time.sleep(.3)
                state=property32('WM_STATE');assert state and state[0]==3,state
                x.XMapWindow(client.display,client.window);x.XFlush(client.display);time.sleep(.5)
                click('Open or create a document · Ctrl+T');wait('Cancel')
                assert all(button(label).get_state_set().contains(Atspi.StateType.ENABLED) for label in ['Minimize window','Maximize window','Close window'])
                assert bounds('Close window')[1]<40
                capture('dialog');click('Cancel')
                click('Library · Ctrl+Shift+L');wait('New document')
                assert bounds('Close window')[1]<40
                library_starts_with_navigation();capture('library')
                # New-note creation is durable even when the custom close button
                # is invoked immediately afterwards.
                click('New document');time.sleep(.15)
                with sqlite3.connect(database) as db: count=db.execute('SELECT count(*) FROM notes').fetchone()[0]
                click('Close window');assert app.wait(timeout=5)==0
                with sqlite3.connect(database) as db: assert db.execute('SELECT count(*) FROM notes').fetchone()[0]==count
                result={name:True for name in ['client_decorations','picker_follows_tabs','picker_visible_with_overflow','active_tab_visible_with_overflow','sidebar_navigation','control_order','native_window_drag','tabs_do_not_move_window','native_tab_reorder','maximize_restore','background_double_click','edge_resize','minimize','controls_available_in_dialog','library_titlebar','close_flushes_and_exits','private_compositor']}
                (args.output/'result.json').write_text(json.dumps(result,indent=2)+'\n');print(json.dumps(result))
    finally:
        if bus and session:
            try:input_call('Stop')
            except GLib.Error:pass
        if client:client.close()
        if app and app.poll() is None:
            app.terminate()
            try:app.wait(timeout=5)
            except subprocess.TimeoutExpired:app.kill();app.wait()
        for process in reversed(processes):
            if process.poll() is None:process.terminate();process.wait(timeout=5)

if __name__=='__main__':main()
