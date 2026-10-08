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
    parser.add_argument('--library-only',action='store_true')
    parser.add_argument('--selection-only',action='store_true')
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
        for width,height,scale,fresh in [(1000,620,.8,False),(1000,620,1.,False),(1366,768,1.,False),(1000,620,1.6,False),(1000,620,1.,True),(1000,620,1.6,True)]:
            if (args.library_only or args.selection_only) and fresh:continue
            with tempfile.TemporaryDirectory(prefix='folio-ux-layout-') as temporary:
                data=Path(temporary)/'data';shutil.copytree(args.fixture,data,ignore=shutil.ignore_patterns('session.lock','*.sqlite3-wal','*.sqlite3-shm'))
                with sqlite3.connect(data/'notes.sqlite3') as db:
                    row=db.execute("SELECT data FROM settings WHERE key='preferences'").fetchone();prefs=json.loads(row[0]) if row else {};prefs.update(ui_scale=scale,reduce_motion=True)
                    if args.library_only:prefs['workspace']={'library_open':True}
                    db.execute("INSERT OR REPLACE INTO settings(key,data) VALUES('preferences',?)",(json.dumps(prefs),))
                    note,metadata=db.execute('SELECT id,metadata FROM notes LIMIT 1').fetchone()
                    metadata=json.loads(metadata);metadata['title']='A long document title for equations and lecture preparation';db.execute('UPDATE notes SET metadata=? WHERE id=?',(json.dumps(metadata),note))
                    if fresh:
                        db.execute('PRAGMA foreign_keys=ON');db.execute('DELETE FROM notes');db.execute('DELETE FROM notebooks')
                        prefs.update(workspace={'library_open':True},recent_documents=[])
                        db.execute("UPDATE settings SET data=? WHERE key='preferences'",(json.dumps(prefs),));note=None
                    else:
                        folder={'id':str(uuid.uuid4()),'name':'Lecture notes for engineering and applied mathematics','parent':None};db.execute('INSERT INTO notebooks(id,data) VALUES(?,?)',(folder['id'],json.dumps(folder)))
                        hidden=dict(metadata,id=str(uuid.uuid4()),title='Trashed course note',notebook=folder['id'],trashed=True,favorite=False)
                        db.execute('INSERT INTO notes(id,metadata) VALUES(?,?)',(hidden['id'],json.dumps(hidden)))
                        page,header=db.execute('SELECT id,header FROM pages WHERE note_id=? ORDER BY position LIMIT 1',(note,)).fetchone();header=json.loads(header)
                        for title in ['Untitled note','Lecture_2_homework']:
                            extra=dict(metadata,id=str(uuid.uuid4()),title=title,favorite=False,notebook=None,trashed=False)
                            db.execute('INSERT INTO notes(id,metadata) VALUES(?,?)',(extra['id'],json.dumps(extra)))
                            extra_page=dict(header,id=str(uuid.uuid4()),order=[],groups=[],text='',revision=0)
                            db.execute('INSERT INTO pages(id,note_id,position,header) VALUES(?,?,0,?)',(extra_page['id'],extra['id'],json.dumps(extra_page)))
                        hidden_page=dict(header,id=str(uuid.uuid4()),order=[],groups=[],text='',revision=0)
                        db.execute('INSERT INTO pages(id,note_id,position,header) VALUES(?,?,0,?)',(hidden_page['id'],hidden['id'],json.dumps(hidden_page)))
                        ink=str(uuid.uuid4())
                        stroke={'id':ink,'raw':[{'x':80+i*3,'y':320+(i%5)*2,'pressure':.7,'tilt_x':0,'tilt_y':0,'timestamp':i*8,'buttons':0} for i in range(30)],'path':[{'position':{'x':80+i*3,'y':320+(i%5)*2},'radius':2} for i in range(30)],'style':{'tool':'Ballpoint','color':{'r':43,'g':57,'b':52},'width':3,'opacity':1,'stabilization':.25,'pressure_gamma':.8},'transform':{'a':1,'b':0,'c':0,'d':1,'tx':0,'ty':0},'created_at':1,'refined_path':None,'refinement_enabled':False}
                        header['order'].append(ink);db.execute('UPDATE pages SET header=? WHERE id=?',(json.dumps(header),page));db.execute('INSERT INTO objects(id,page_id,data) VALUES(?,?,?)',(ink,page,json.dumps({'Stroke':stroke})))
                pack=data/'recognition/pack.json';pack.parent.mkdir(exist_ok=True)
                pack.write_text(json.dumps({'backend':'llama-vulkan'})) # Explicit uninstalled native pack; ignore adjacent development-only legacy packs.
                case=f'{width}x{height}-{scale}'+('-fresh' if fresh else '')
                with (args.output/(case+'.log')).open('w') as log:
                    app=subprocess.Popen([str(args.binary.resolve()),'--data-dir',str(data)]+(['--open-note',note] if note and not args.library_only else []),env=dict(os.environ,FOLIO_RECOGNITION_CONFIG=str(pack)),stdout=log,stderr=log)
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
                            if node is None:return # The native tree may update during asynchronous saves.
                            node.clear_cache();yield node
                            for i in range(node.get_child_count()):yield from walk(node.get_child_at_index(i))
                        def nodes():return list(walk(target))
                        def find(label):
                            for _ in range(20):
                                matches=[n for n in nodes() if n.get_name()==label]
                                if matches:return matches[-1]
                                time.sleep(.1)
                            raise AssertionError(('Missing',label,[n.get_name() for n in nodes()]))
                        def click(label):assert find(label).get_action_iface().do_action(0);time.sleep(.3)
                        def fill(label,text):
                            assert find(label).get_component_iface().grab_focus();time.sleep(.1)
                            client.key('a',4,delay=.02);client.key('BackSpace',delay=.02)
                            for char in text:
                                client.key('space' if char==' ' else char.lower(),1 if char.isupper() else 0,delay=.02)
                            time.sleep(.15)
                        def capture(name):subprocess.run([sys.executable,'scripts/capture-x11.py',str(args.output/(case+'-'+name+'.png')),'--pid',str(app.pid),'--virtual-display-root'],check=True)
                        def check_library_rows():
                            click('List view')
                            labels={'Open '+title for title in [metadata['title'],'Untitled note','Lecture_2_homework']}
                            seen=set()
                            for _ in range(5):
                                rows=[]
                                for node in nodes():
                                    if node.get_name() not in labels or node.get_role()!=Atspi.Role.PUSH_BUTTON:continue
                                    r=node.get_component_iface().get_extents(Atspi.CoordType.WINDOW)
                                    assert r.width>width*.4 and r.height>=40*scale and 0<=r.x and r.x+r.width<=width+2,(case,node.get_name(),r.x,r.width,r.height)
                                    rows.append(r);seen.add(node.get_name())
                                rows.sort(key=lambda r:r.y)
                                if any(a.y+a.height>b.y+1 for a,b in zip(rows,rows[1:])):capture('list-overlap-failure')
                                assert all(a.y+a.height<=b.y+1 for a,b in zip(rows,rows[1:])),[(r.y,r.height) for r in rows]
                                if seen==labels:break
                                client.click(width-100,height-90,button=5)
                            assert seen==labels,(case,'Rows unreachable by scrolling',seen,labels)
                            capture('library-list')
                            for _ in range(5):client.click(width-100,height-90,button=4,delay=.04)
                            click('Grid view');capture('library-grid')
                        def check_favorite_clicks():
                            title='Lecture_2_homework'
                            added='Add '+title+' to favorites';removed='Remove '+title+' from favorites'
                            def pointer_click(label):
                                r=find(label).get_component_iface().get_extents(Atspi.CoordType.WINDOW)
                                assert 0<=r.x and r.x+r.width<=width+2 and 0<=r.y and r.y+r.height<=height+2,(case,label,r.x,r.y,r.width,r.height)
                                client.click(r.x+r.width/2,r.y+r.height/2)
                            for mode,activation in [('Grid view','space'),('List view','Return')]:
                                click(mode)
                                # In large-interface compact layouts a requested grid uses rows.
                                # Scroll the chosen document into view if it is virtualized out.
                                for _ in range(5):
                                    if any(n.get_name()==added for n in nodes()):break
                                    client.click(width-100,height-90,button=5)
                                pointer_click(added);find(removed);find('List view')
                                client.click(width-24,height-24) # Leave the star so its hover hint cannot hide the saved state.
                                capture('favorite-'+mode.split()[0].lower())
                                with sqlite3.connect(data/'notes.sqlite3') as db:
                                    assert json.loads(db.execute('SELECT metadata FROM notes WHERE id=?',(note,)).fetchone()[0])['favorite']==metadata['favorite'], 'Favorite changed the editor document'
                                assert find(removed).get_component_iface().grab_focus()
                                for _ in range(20):
                                    if find(removed).get_state_set().contains(Atspi.StateType.FOCUSED):break
                                    time.sleep(.05)
                                assert find(removed).get_state_set().contains(Atspi.StateType.FOCUSED)
                                client.key(activation);find(added)
                            pointer_click(added);find(removed)
                            for _ in range(20):
                                with sqlite3.connect(data/'notes.sqlite3') as db:
                                    record=next(json.loads(row[0]) for row in db.execute('SELECT metadata FROM notes') if json.loads(row[0])['title']==title)
                                if record['favorite']:break
                                time.sleep(.1)
                            assert record['favorite'], 'Favorite was not persisted'
                            assert record['updated_at']==metadata['updated_at'], 'Favorite changed edit time and reordered the library'
                            click('Favorites');find('Open '+title)
                            pointer_click(removed);find('List view')
                            assert not any(n.get_name()=='Open '+title for n in nodes()), 'Removed favorite remained in Favorites'
                            click('Documents');click('Grid view')
                        if args.library_only:
                            check_library_rows()
                            check_favorite_clicks()
                            reports.append({'case':case,'full_width_rows':True,'no_overlap':True,'native_favorite_mouse_and_keyboard':True,'favorite_persistence':True});continue
                        if fresh:
                            for label in ['Start writing','Try the sample notebook']:
                                r=find(label).get_component_iface().get_extents(Atspi.CoordType.WINDOW)
                                assert 0<=r.x and r.x+r.width<=width+2 and 0<=r.y and r.y+r.height<=height-36,(case,label,r.x,r.y,r.width,r.height)
                            with sqlite3.connect(data/'notes.sqlite3') as db:assert db.execute('SELECT COUNT(*) FROM notes').fetchone()[0]==0
                            capture('onboarding')
                            if scale==1.:
                                click('Try the sample notebook');click('Keyboard shortcuts and help');click('Open starter document')
                            else:click('Start writing')
                            assert find('Pen · P').get_state_set().contains(Atspi.StateType.CHECKED)
                            time.sleep(.3)
                            with sqlite3.connect(data/'notes.sqlite3') as db:assert db.execute('SELECT COUNT(*) FROM notes').fetchone()[0]==1
                            capture('first-writing');reports.append({'case':case,'empty_startup':True,'writing_action':True});continue
                        pen=find('Pen · P');assert pen.get_role()==Atspi.Role.RADIO_BUTTON
                        assert pen.get_state_set().contains(Atspi.StateType.CHECKED)
                        for label in ['Pen · P','Eraser · E','Lasso · L','Pan · H','Pen settings and presets']:
                            r=find(label).get_component_iface().get_extents(Atspi.CoordType.WINDOW)
                            assert r.width>0 and 0<=r.x and r.x+r.width<=width+2,(case,label,r.x,r.width)
                        capture('editor')
                        def inside(label):
                            r=find(label).get_component_iface().get_extents(Atspi.CoordType.WINDOW)
                            assert r.width>0 and r.height>0 and 0<=r.x and r.x+r.width<=width+2 and 0<=r.y and r.y+r.height<=height+2,(case,label,r.x,r.y,r.width,r.height)
                        if args.selection_only:
                            client.key('a',4);time.sleep(.3)
                            for label in ['Cut','Copy','Delete','Solve']:inside(label)
                            assert not any(n.get_name() in ['−10% size','+10% size','Rotate 15°','Apply pen','Refine ink','Remove refinement','Duplicate'] for n in nodes()), 'Removed transforms remain in selection toolbar'
                            capture('selection-toolbar')
                            client.click(width-20,height-65,button=3);inside('Paste')
                            assert not find('Paste').get_state_set().contains(Atspi.StateType.ENABLED), 'Empty clipboard should disable Paste'
                            client.key('Escape');inside('Copy')
                            click('Copy');inside('Cut')
                            click('Cut');time.sleep(.2)
                            assert not any(n.get_name()=='Copy' for n in nodes()), 'Cut did not remove the selection'
                            client.key('z',4);time.sleep(.3);client.key('a',4);time.sleep(.2);inside('Copy')
                            client.key('Escape');time.sleep(.2)
                            assert not any(n.get_name()=='Copy' for n in nodes()), 'Selection toolbar did not dismiss'
                            def object_count():
                                client.key('s',4);time.sleep(.35)
                                with sqlite3.connect(data/'notes.sqlite3') as db:return db.execute('SELECT COUNT(*) FROM objects WHERE page_id=?',(page,)).fetchone()[0]
                            before=object_count()
                            client.click(width-20,height-65,button=3);inside('Paste');capture('paste-menu')
                            client.key('Escape');time.sleep(.2)
                            assert not any(n.get_name()=='Paste' for n in nodes()), 'Escape did not dismiss paste menu'
                            client.click(width-20,height-65,button=3);inside('Paste')
                            r=find('Paste').get_component_iface().get_extents(Atspi.CoordType.WINDOW)
                            client.click(r.x+r.width/2,r.y+r.height/2);time.sleep(.3)
                            assert not any(n.get_name()=='Paste' for n in nodes()), 'Paste did not close menu'
                            assert object_count()==before*2, 'Context paste did not insert clipboard objects'
                            client.key('z',4);assert object_count()==before, 'Paste did not undo as one operation'
                            client.click(width-20,height-65,button=3);inside('Paste');client.key('v',4)
                            assert object_count()==before*2, 'Paste shortcut in context menu failed'
                            client.key('z',4);assert object_count()==before
                            client.key('Escape');client.key('a',4);inside('Copy')
                            client.click(width-20,height-65,button=3);inside('Paste')
                            client.key('Escape');inside('Copy') # Menu dismissal must preserve selection.
                            client.click(width-20,height-65,button=3);inside('Paste')
                            client.click(width-10,height-110);time.sleep(.2)
                            assert not any(n.get_name()=='Paste' for n in nodes()), 'Outside click did not dismiss'
                            inside('Copy');assert object_count()==before, 'Dismissal drew through the menu'
                            click('Settings');click('Dark appearance');click('Close settings')
                            client.key('a',4);capture('selection-toolbar-dark')
                            client.click(width-20,height-65,button=3);inside('Paste');capture('paste-menu-dark');client.key('Escape')
                            reports.append({'case':case,'icon_toolbar':True,'cut_copy_undo':True,'context_paste_undo':True,'menu_dismissal_preserves_selection':True});continue
                        click('Settings')
                        for label in ['Appearance','Writing','Library','Accessibility','Close settings','Light appearance','Dark appearance']:
                            inside(label)
                        capture('settings-appearance')
                        click('Dark appearance');capture('settings-dark');click('Light appearance')
                        click('Writing');inside('Scratch to erase: Off');click('Scratch to erase: Off');inside('Scratch to erase: On');capture('settings-writing')
                        click('Scratch to erase: On')
                        click('Accessibility');inside('Reduce motion: On');capture('settings-accessibility')
                        click('Library');inside('Open another library…');capture('settings-library')
                        click('Close settings')
                        if scale==1. and width==1366:
                            click('Pen settings and presets');capture('presets')
                            click('Save current pen as preset…');fill('Name pen preset','Lecture pen');click('Save')
                            # Duplicate styles reuse the existing preset rather than adding one.
                            click('Pen settings and presets')
                            assert any(n.get_name().startswith('Use preset ') for n in nodes())
                            capture('presets-saved');client.key('Escape')
                            click('Document actions');click('Paper and canvas ›');click('Custom page size')
                            click('Letter');click('Inches');capture('physical-page-size');click('Save')
                            click('Document actions');click('Math and handwriting ›');click('Index page handwriting…')
                            assert any(n.get_name()=='Download and recognize' for n in nodes()), 'Missing explicit OCR setup choice'
                            capture('ocr-consent');click('Cancel')
                            assert not (data/'recognition/glm-ocr-q8-b11457').exists(), 'Setup downloaded files without consent'
                            client.key('Escape');click('Pen · P')

                        client.key('p',5);time.sleep(.3) # Ctrl+Shift+P
                        click('Document actions');click('Math and handwriting ›');click('Math solver');time.sleep(.6)
                        field=find('Math expression');assert field.get_component_iface().grab_focus();time.sleep(.2)
                        assert find("Math expression").get_state_set().contains(Atspi.StateType.FOCUSED)
                        capture('panels')
                        client.key('Escape');client.key('l',5);time.sleep(.3);capture('library')
                        click('Settings');click('Appearance');click('Dark appearance');click('Close settings')
                        check_library_rows()
                        check_favorite_clicks()
                        if scale==1. and width==1366:
                            click('Last edited: newest first ▾');click('Name: A–Z');capture('explicit-sort')
                            star=next(n for n in nodes() if n.get_name().startswith('Add ') and n.get_name().endswith(' to favorites'))
                            assert star.get_action_iface().do_action(0);time.sleep(.3)
                            assert any(n.get_name().startswith('Remove ') and n.get_name().endswith(' from favorites') for n in nodes())
                            capture('favorite-toggle')
                            click(folder['name']);assert not find('Delete empty folder').get_state_set().contains(Atspi.StateType.ENABLED)
                            assert '1 trashed document' in find('Delete empty folder').get_description()
                            capture('folder-blocked');click('View 1 in Trash')
                            assert any(n.get_name()=='Back to folder' for n in nodes())
                            assert any('Trashed course note' in n.get_name() for n in nodes())
                            capture('folder-trash');click('Back to folder')
                        reports.append({'case':case,'pen_role_and_state':True,'field_focus':True,'toolbar_bounds':True})
                    finally:
                        if client:client.close()
                        app.terminate();app.wait(timeout=10)
        (args.output/'results.json').write_text(json.dumps(reports,indent=2));print(json.dumps(reports))
    finally:
        for process in processes:process.terminate()
if __name__=='__main__':main()
