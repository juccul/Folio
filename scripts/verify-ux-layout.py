#!/usr/bin/env python3
"""Check layout and accessibility on a caller-owned private X11/D-Bus session."""
import ctypes as C
import uuid
import argparse,json,os,shutil,sqlite3,subprocess,sys,tempfile,time
from pathlib import Path
import gi
from gi.repository import Gio,GLib
from ui_x11 import Client

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    for name in ('binary','fixture','output'):parser.add_argument('--'+name,type=Path,required=True)
    args=parser.parse_args()
    assert os.environ.get('FOLIO_VIRTUAL_DISPLAY')=='1' and not os.environ.get('WAYLAND_DISPLAY')
    args.output.mkdir(parents=True,exist_ok=True)
    processes=[]
    try:
        processes.append(subprocess.Popen(['/usr/libexec/at-spi-bus-launcher','--launch-immediately'],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL));time.sleep(.3)
        bus=Gio.bus_get_sync(Gio.BusType.SESSION,None)
        address=bus.call_sync('org.a11y.Bus','/org/a11y/bus','org.a11y.Bus','GetAddress',None,GLib.VariantType.new('(s)'),Gio.DBusCallFlags.NONE,5000,None).unpack()[0]
        os.environ['AT_SPI_BUS_ADDRESS']=address
        gi.require_version('Atspi','2.0');from gi.repository import Atspi
        processes.append(subprocess.Popen(['/usr/libexec/at-spi2-registryd'],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL))
        bus.call_sync('org.a11y.Bus','/org/a11y/bus','org.freedesktop.DBus.Properties','Set',GLib.Variant('(ssv)',('org.a11y.Status','IsEnabled',GLib.Variant('b',True))),None,Gio.DBusCallFlags.NONE,5000,None)
        reports=[]
        for width,height,scale in [(1000,620,.8),(1000,620,1.),(1366,768,1.),(1000,620,1.6)]:
            with tempfile.TemporaryDirectory(prefix='folio-ux-layout-') as temporary:
                data=Path(temporary)/'data';shutil.copytree(args.fixture,data,ignore=shutil.ignore_patterns('session.lock','*.sqlite3-wal','*.sqlite3-shm'))
                with sqlite3.connect(data/'notes.sqlite3') as db:
                    row=db.execute("SELECT data FROM settings WHERE key='preferences'").fetchone();prefs=json.loads(row[0]) if row else {};prefs.update(ui_scale=scale,reduce_motion=True)
                    db.execute("INSERT OR REPLACE INTO settings(key,data) VALUES('preferences',?)",(json.dumps(prefs),))
                    note,metadata=db.execute('SELECT id,metadata FROM notes LIMIT 1').fetchone()
                    metadata=json.loads(metadata);metadata['title']='A long document title for equations and lecture preparation';db.execute('UPDATE notes SET metadata=? WHERE id=?',(json.dumps(metadata),note))
                    folder={'id':str(uuid.uuid4()),'name':'Lecture notes for engineering and applied mathematics','parent':None};db.execute('INSERT INTO notebooks(id,data) VALUES(?,?)',(folder['id'],json.dumps(folder)))
                case=f'{width}x{height}-{scale}'
                with (args.output/(case+'.log')).open('w') as log:
                    app=subprocess.Popen([str(args.binary.resolve()),'--data-dir',str(data),'--open-note',note],stdout=log,stderr=log)
                    client=None
                    try:
                        client=Client(app.pid)
                        for _ in range(5):client.wake_virtual_window()
                        client.x.XSetInputFocus.argtypes=[C.c_void_p,C.c_ulong,C.c_int,C.c_ulong]
                        client.x.XSetInputFocus(client.display,client.window,1,0);client.x.XFlush(client.display)
                        client.resize(width,height);time.sleep(.3)
                        target=None
                        for _ in range(50):
                            desktop=Atspi.get_desktop(0)
                            target=next((desktop.get_child_at_index(i) for i in range(desktop.get_child_count()) if desktop.get_child_at_index(i).get_process_id()==app.pid),None)
                            if target:break
                            time.sleep(.1)
                        assert target,'No native accessible app'
                        def walk(node):
                            node.clear_cache();yield node
                            for i in range(node.get_child_count()):yield from walk(node.get_child_at_index(i))
                        def nodes():return list(walk(target))
                        def find(label):
                            matches=[n for n in nodes() if n.get_name()==label];assert matches,('Missing',label,[n.get_name() for n in nodes()]);return matches[-1]
                        def click(label):assert find(label).get_action_iface().do_action(0);time.sleep(.3)
                        def capture(name):subprocess.run([sys.executable,'scripts/capture-x11.py',str(args.output/(case+'-'+name+'.png')),'--pid',str(app.pid),'--virtual-display-root'],check=True)
                        pen=find('Pen · P');assert pen.get_role()==Atspi.Role.RADIO_BUTTON
                        assert pen.get_state_set().contains(Atspi.StateType.CHECKED)
                        for label in ['Pen · P','Eraser · E','Lasso · L','Pan · H','Pen settings and presets']:
                            r=find(label).get_component_iface().get_extents(Atspi.CoordType.WINDOW)
                            assert r.width>0 and 0<=r.x and r.x+r.width<=width+2,(case,label,r.x,r.width)
                        capture('editor')
                        client.key('p',5);time.sleep(.3) # Ctrl+Shift+P
                        click('Document actions');click('Math and handwriting ›');click('Math solver');time.sleep(.6)
                        field=find('Math expression');assert field.get_component_iface().grab_focus();time.sleep(.2)
                        assert find("Math expression").get_state_set().contains(Atspi.StateType.FOCUSED)
                        capture('panels')
                        client.key('Escape');client.key('l',5);time.sleep(.3);capture('library')
                        reports.append({'case':case,'pen_role_and_state':True,'field_focus':True,'toolbar_bounds':True})
                    finally:
                        if client:client.close()
                        app.terminate();app.wait(timeout=10)
        (args.output/'results.json').write_text(json.dumps(reports,indent=2));print(json.dumps(reports))
    finally:
        for process in processes:process.terminate()
if __name__=='__main__':main()
