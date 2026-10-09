#!/usr/bin/env python3
"""Verify Folio's AT-SPI tree/actions in an isolated D-Bus desktop session.
Run: dbus-run-session -- python3 scripts/verify-accessibility.py
Does not change the user's desktop accessibility settings.
"""
import argparse,json,os,shutil,subprocess,tempfile,time
from pathlib import Path
import gi
from gi.repository import Gio,GLib
ROOT=Path(__file__).resolve().parents[1]
def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary',type=Path,default=ROOT/'target/release/folio')
    parser.add_argument('--virtual-display',action='store_true',help='Replay the mapped-window notification on an isolated test display')
    parser.add_argument('--fixture',type=Path,help='Copy a closed throwaway fixture into the temporary test directory')
    parser.add_argument('--navigation',action='store_true',help='Exercise document management menus and the tab picker on throwaway notes')
    parser.add_argument('--polish',action='store_true',help='Check modal isolation, selection preservation, validation and motion preferences')
    parser.add_argument('--appearance',action='store_true',help='Exercise theme customization and themed/fixed paper on throwaway notes')
    parser.add_argument('--system-theme',action='store_true',help='Verify live system-theme events through a private desktop portal')
    parser.add_argument('--shortcuts',action='store_true',help='Verify native Ctrl+N, modal typing and Ctrl+S on an isolated test display')
    parser.add_argument('--screenshots',type=Path,help='Save only the test application client window')
    args=parser.parse_args()
    if (args.appearance or args.navigation or args.polish or args.system_theme) and not (args.fixture and args.virtual_display):parser.error('These checks require a throwaway --fixture and --virtual-display')
    if args.shortcuts and not args.virtual_display:parser.error('--shortcuts requires an isolated --virtual-display')
    if (args.fixture or args.screenshots) and not args.virtual_display:parser.error('Fixture screenshots require --virtual-display')
    launcher=subprocess.Popen(['/usr/libexec/at-spi-bus-launcher','--launch-immediately'],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
    time.sleep(.3)
    bus=Gio.bus_get_sync(Gio.BusType.SESSION,None)
    address=bus.call_sync('org.a11y.Bus','/org/a11y/bus','org.a11y.Bus','GetAddress',None,GLib.VariantType.new('(s)'),Gio.DBusCallFlags.NONE,5000,None).unpack()[0]
    os.environ['AT_SPI_BUS_ADDRESS']=address
    gi.require_version('Atspi','2.0')
    from gi.repository import Atspi
    registry=subprocess.Popen(['/usr/libexec/at-spi2-registryd'],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
    time.sleep(.3)
    bus.call_sync('org.a11y.Bus','/org/a11y/bus','org.freedesktop.DBus.Properties','Set',GLib.Variant('(ssv)',('org.a11y.Status','IsEnabled',GLib.Variant('b',True))),None,Gio.DBusCallFlags.NONE,5000,None)
    portal=None
    if args.system_theme:
        portal=subprocess.Popen(['python3',str(ROOT/'scripts/ui_theme_portal.py')],stdout=subprocess.PIPE,text=True)
        assert portal.stdout.readline().strip()=='READY', 'Private theme portal did not start'
    with tempfile.TemporaryDirectory(prefix='folio-a11y-') as root:
        if args.fixture:shutil.copytree(args.fixture,root,dirs_exist_ok=True,ignore=shutil.ignore_patterns('session.lock','*.sqlite3-wal','*.sqlite3-shm'))
        env=os.environ.copy();env['WAYLAND_DISPLAY']=''
        with open('/tmp/folio-a11y-runtime.log','w') as log:
            app=subprocess.Popen([str(args.binary.resolve()),'--data-dir',root],env=env,stdout=log,stderr=log)
            try:
                if args.virtual_display:
                    from ui_x11 import Client
                    os.environ['FOLIO_VIRTUAL_DISPLAY']='1'
                    client=Client(app.pid)
                    # Wait until bare-server mapping and Vulkan startup finish.
                    time.sleep(2)
                    for _ in range(5):client.wake_virtual_window()
                target=None
                for _ in range(60):
                    while GLib.MainContext.default().iteration(False):pass
                    desktop=Atspi.get_desktop(0)
                    for i in range(desktop.get_child_count()):
                        child=desktop.get_child_at_index(i)
                        if 'folio' in child.get_name().lower():target=child;break
                    if target is not None:break
                    if app.poll() is not None:raise RuntimeError('Folio exited; inspect /tmp/folio-a11y-runtime.log')
                    time.sleep(.15)
                if target is None:raise RuntimeError('No Folio AT-SPI application registered')
                def walk(node):
                    # Older AT-SPI clients cannot decode the newest cache signal.
                    # Query current properties rather than using stale children.
                    node.clear_cache()
                    yield node
                    for i in range(node.get_child_count()):yield from walk(node.get_child_at_index(i))
                def controls():
                    while GLib.MainContext.default().iteration(False):pass
                    # Native radio, switch, and tab controls expose the same Action
                    # interface as buttons. Preserve their correct semantic roles.
                    return [n for n in walk(target) if n.get_role() in [Atspi.Role.PUSH_BUTTON,Atspi.Role.PUSH_BUTTON_MENU,Atspi.Role.RADIO_BUTTON,Atspi.Role.CHECK_BOX,Atspi.Role.TOGGLE_BUTTON,Atspi.Role.PAGE_TAB,Atspi.Role.LIST_ITEM,Atspi.Role.TREE_ITEM]]
                def click(label, prefix=False):
                    buttons=controls()
                    item=next((n for n in buttons if (n.get_name().startswith(label) if prefix else n.get_name()==label)),None)
                    assert item is not None, f'Missing {label}: {[n.get_name() for n in buttons]}'
                    assert item.get_action_iface().do_action(0), f'Native action failed: {label}'
                    time.sleep(.5)
                    while GLib.MainContext.default().iteration(False):pass
                def capture(name):
                    if args.screenshots or args.system_theme:
                        directory=args.screenshots or Path(root)/'captures'
                        directory.mkdir(parents=True,exist_ok=True)
                        destination=directory/(name+'.png')
                        subprocess.run(['python3',str(ROOT/'scripts/capture-x11.py'),str(destination),'--pid',str(app.pid),'--virtual-display-root'],check=True,env=env)
                        from PIL import Image
                        with Image.open(destination) as image:
                            pixels=image.convert('RGB').resize((120,80)).tobytes()
                            assert len({pixels[i:i+3] for i in range(0,len(pixels),3)})>40, 'Blank UI capture: '+name
                        return destination
                time.sleep(.5);buttons=controls()
                assert {'Documents','Favorites','Trash','Grid view','List view','New folder'}.issubset({n.get_name() for n in buttons}), 'Expected distinct labeled library controls'
                import sqlite3
                with sqlite3.connect(Path(root)/'notes.sqlite3') as db:
                    initial_notes=db.execute('SELECT count(*) FROM notes').fetchone()[0]
                    initial_objects=db.execute('SELECT count(*) FROM objects').fetchone()[0]
                assert not any(n.get_name()=='New document with options…' for n in buttons), 'Creation options must be on the main button, without adjacent sliders'
                capture('library-light')
                if args.fixture:
                    click('List view');capture('library-list')
                    click('Grid view');click('Favorites');capture('library-favorites');click('Documents')
                    click('Open Field notes')
                    assert sum(n.get_name()=='Library · Ctrl+Shift+L' for n in controls())==1, 'Editor must expose exactly one Library/Home button'
                    click('Page thumbnails',True);capture('editor-light')
                    assert {'Go to page 1','Go to page 2'}.issubset({n.get_name() for n in controls()})
                    click('Go to page 2');capture('editor-page-two');click('Go to page 1')
                    # Separate document key context: Delete/Ctrl+A cannot affect
                    # the current document while its library is displayed.
                    click('Library ·',True)
                    if args.virtual_display:
                        client.key('a',4);client.key('Delete');client.key('Escape')
                        with sqlite3.connect(Path(root)/'notes.sqlite3') as db:assert db.execute('SELECT count(*) FROM objects').fetchone()[0]==initial_objects, 'Library shortcuts edited hidden content'
                    click('Open Reading list');capture('editor-tabs')
                    client.resize(980,760);capture('editor-compact');client.resize(1320,860)
                    click('Library ·',True);click('Settings');click('Appearance');click('Dark appearance',True)
                    if args.virtual_display:client.key('Escape')
                    capture('library-dark');click('Open Field notes');capture('editor-dark');click('Library ·',True)
                if args.system_theme:
                    def preferences():
                        with sqlite3.connect(Path(root)/'notes.sqlite3') as db:
                            return json.loads(db.execute("SELECT data FROM settings WHERE key='preferences'").fetchone()[0])
                    def system_scheme(value):
                        bus.call_sync('org.freedesktop.portal.Desktop','/org/freedesktop/portal/desktop','org.folio.TestTheme','SetColorScheme',GLib.Variant('(u)',(value,)),None,Gio.DBusCallFlags.NONE,5000,None)
                        time.sleep(.5)
                    with sqlite3.connect(Path(root)/'notes.sqlite3') as db:objects_before=list(db.execute('SELECT id,data FROM objects ORDER BY id'))
                    click('Open Field notes');click('Settings');click('Appearance');click('System appearance')
                    assert preferences()['follow_system_theme'], 'System choice was not saved'
                    assert next(n for n in controls() if n.get_name()=='System appearance').get_state_set().contains(Atspi.StateType.CHECKED), 'System theme must expose its selected radio state'
                    capture('appearance-system-dark')
                    system_scheme(2);capture('appearance-system-light');click('Close settings');light_capture=capture('editor-system-light')
                    system_scheme(1);dark_capture=capture('editor-system-dark')
                    from PIL import Image,ImageStat
                    with Image.open(light_capture) as light,Image.open(dark_capture) as dark:
                        light_mean=sum(ImageStat.Stat(light.convert('RGB')).mean)/3
                        dark_mean=sum(ImageStat.Stat(dark.convert('RGB')).mean)/3
                        assert light_mean>dark_mean+40, 'System signal did not redraw the light/dark workspace and paper'
                    click('Settings');click('Appearance');click('Light appearance')
                    system_scheme(2);system_scheme(1)
                    assert not preferences()['dark'] and not preferences()['follow_system_theme'], 'System event replaced manual Light override'
                    capture('appearance-manual-light')
                    click('Dark appearance');system_scheme(2)
                    assert preferences()['dark'] and not preferences()['follow_system_theme'], 'System event replaced manual Dark override'
                    click('System appearance')
                    assert not preferences()['dark'] and preferences()['follow_system_theme'], 'System choice did not immediately resolve current appearance'
                    system_scheme(0);capture('appearance-no-preference')
                    click('Close settings');click('Settings');click('Updates');capture('settings-updates');click('Close settings')
                    with sqlite3.connect(Path(root)/'notes.sqlite3') as db:assert list(db.execute('SELECT id,data FROM objects ORDER BY id'))==objects_before,'Theme events changed document content'
                    click('Library ·',True)
                    click('New document');click('Cancel')
                    before=capture('library-system-after-dialog-light');system_scheme(1);after=capture('library-system-after-dialog-dark')
                    with Image.open(before) as light,Image.open(after) as dark:
                        assert sum(ImageStat.Stat(light.convert('RGB')).mean)>sum(ImageStat.Stat(dark.convert('RGB')).mean)+120, 'Opening or dismissing a dialog lost the system theme observer'
                    system_scheme(2)
                    print('SYSTEM_THEME_OK: live dark/light/no-preference, immediate selection, manual overrides, native radio state, saved policy, dialog lifecycle, document preservation',flush=True)
                if args.polish:
                    click('Documents');click('Open Field notes')
                    client.key('a',4);time.sleep(.2)
                    assert any(n.get_name()=='Copy' for n in controls()), 'Select-all must expose editing controls'
                    click('Settings')
                    assert not any(n.get_name() in {'Undo','Redo','Delete','Copy','Cut','Add page'} for n in controls()), 'Background controls leaked through the settings overlay'
                    client.key('Delete');client.key('z',4)
                    for _ in range(8):client.key('Tab')
                    with sqlite3.connect(Path(root)/'notes.sqlite3') as db:
                        assert db.execute('SELECT count(*) FROM objects').fetchone()[0]==initial_objects, 'Settings shortcuts edited the document'
                    click('Accessibility');click('Reduce motion: Off');click('Close settings')
                    assert any(n.get_name()=='Copy' for n in controls()), 'Closing settings lost the selection'
                    size_label=next((n.get_name() for n in controls() if n.get_name().endswith(' pt')),None)
                    assert size_label, 'Missing selected-text font-size control'
                    click(size_label);client.key('a',4)
                    for char in 'nan':client.key(char)
                    click('Save')
                    assert any(n.get_role()==Atspi.Role.ENTRY and n.get_name()=='Font size' for n in walk(target)), 'Invalid size closed its dialog'
                    assert not any(n.get_name() in {'Copy','Cut','Undo','Settings'} for n in controls()), 'Background controls leaked through a dialog'
                    client.key('a',4)
                    for char in '24':client.key(char)
                    click('Save')
                    assert not any(n.get_role()==Atspi.Role.ENTRY for n in walk(target)), 'Correcting an invalid value must work immediately'
                    client.key('z',4);time.sleep(.2)
                    click('Document actions');client.key('Escape');time.sleep(.2)
                    assert any(n.get_name()=='Copy' for n in controls()), 'Closing a menu lost the selection'
                    client.key('Escape');time.sleep(.2)
                    assert not any(n.get_name()=='Copy' for n in controls()), 'Escape on the canvas must still clear the selection'
                    click('Library ·',True);click('Keyboard shortcuts and help')
                    help_controls={n.get_name() for n in controls()}
                    assert {'Got it','Open starter document','Start input check'}.issubset(help_controls)
                    assert not help_controls-{'Got it','Open starter document','Start input check','Minimize window','Maximize window','Restore window','Close window'}, f'Help leaked background controls: {help_controls}'
                    client.key('Escape')
                    with sqlite3.connect(Path(root)/'notes.sqlite3') as db:
                        prefs=json.loads(db.execute("SELECT data FROM settings WHERE key='preferences'").fetchone()[0])
                        assert prefs['reduce_motion'] is True, 'Motion preference was not saved'
                        assert db.execute('SELECT count(*) FROM objects').fetchone()[0]==initial_objects
                    print('POLISH_UI_OK: modal focus isolation, shortcut safety, selection preservation, inline validation, help and persisted reduced motion',flush=True)
                if args.navigation:
                    def rename_dialog(value):
                        inputs=[n for n in walk(target) if n.get_role()==Atspi.Role.ENTRY]
                        assert inputs, 'Missing editable dialog field'
                        client.key('a',4)
                        for char in value:client.key('space' if char==' ' else ('comma' if char==',' else char.lower()),1 if char.isupper() else 0)
                        click('Save')
                    click('Documents');click('Manage Reading list');click('Rename…');rename_dialog('reading archive')
                    assert any(n.get_name()=='Manage reading archive' for n in controls()), [n.get_name() for n in controls()]
                    click('Manage reading archive');click('Add to favorites')
                    click('Manage reading archive');click('Edit tags…');rename_dialog('reference, papers')
                    click('Manage reading archive');click('Move to folder…');click('Projects');click('Move')
                    click('Manage reading archive');click('Duplicate')
                    assert any(n.get_name()=='Manage reading archive (copy)' for n in controls())
                    click('Manage reading archive');click('Move to trash');click('Trash')
                    click('Manage reading archive');click('Restore document');click('Documents')
                    click('Open Field notes');click('Open or create a document · Ctrl+T')
                    click('Open reading archive');assert not any(n.get_role()==Atspi.Role.ENTRY for n in walk(target))
                    click('Open or create a document · Ctrl+T');click('＋  Create new document')
                    assert not any(n.get_role()==Atspi.Role.ENTRY for n in walk(target)), 'Quick creation unexpectedly opened a setup dialog'
                    click('Rename document');rename_dialog('untitled note')
                    assert any(n.get_name()=='Open untitled note' for n in controls())
                    click('Library ·',True)
                    with sqlite3.connect(Path(root)/'notes.sqlite3') as db:
                        rows=[json.loads(row[0]) for row in db.execute('SELECT metadata FROM notes')]
                        modified=next(n for n in rows if n['title']=='reading archive')
                        assert modified['favorite'] and not modified['trashed'] and modified['tags']==['reference','papers']
                        assert next(n for n in rows if n['title']=='Field notes')['title']=='Field notes'
                    initial_notes+=2
                    capture('library-document-menus-complete')
                    print('NAVIGATION_A11Y_OK: targeted rename, tags, favorite, move, duplicate, trash, restore, tab picker and create',flush=True)
                if args.appearance:
                    def preferences():
                        with sqlite3.connect(Path(root)/'notes.sqlite3') as db:
                            return json.loads(db.execute("SELECT data FROM settings WHERE key='preferences'").fetchone()[0])
                    def edit_hex(value):
                        client.key('a',4)
                        for char in value:client.key(char)
                        click('Save')
                    def snapshot_objects():
                        with sqlite3.connect(Path(root)/'notes.sqlite3') as db:
                            return db.execute('SELECT * FROM objects ORDER BY id').fetchall()
                    objects_before=snapshot_objects()
                    click('Open Field notes');click('Settings');click('Appearance');capture('settings-dark')
                    click('Paper follows appearance: On');click('Close settings');capture('editor-dark-white-paper')
                    assert preferences()['appearance']['canvas_follows_theme'] is False
                    click('Settings');click('Appearance');click('Custom paper color');edit_hex('fdf5e6');click('Close settings');capture('editor-dark-custom-paper')
                    assert preferences()['appearance']['canvas_color']=={'r':253,'g':245,'b':230}
                    click('Settings');click('Appearance');click('Paper follows appearance: Off')
                    click('Edit colors…');click('Customize Primary');edit_hex('ff8844')
                    click('Customize Borders');edit_hex('ffffff33');capture('custom-colors-dark')
                    assert preferences()['appearance']['dark']['primary']==int('ff8844ff',16)
                    assert preferences()['appearance']['dark']['border']==int('ffffff33',16)
                    click('Light appearance');click('Customize Primary');edit_hex('223344')
                    assert preferences()['appearance']['light']['primary']==int('223344ff',16)
                    assert preferences()['appearance']['dark']['primary']==int('ff8844ff',16)
                    click('Customize Background');edit_hex('xyz');capture('invalid-theme-color')
                    assert any(n.get_name()=='Save' for n in controls()), 'Invalid color must keep the dialog open'
                    edit_hex('ffffff')
                    click('Increase corner radius');assert preferences()['appearance']['radius']==12
                    capture('custom-colors-light')
                    click('Reset appearance');click('Close settings');capture('editor-light-reset')
                    prefs=preferences()['appearance']
                    assert prefs['light']=={} and prefs['dark']=={} and prefs['canvas_follows_theme'] and prefs['radius']==10
                    assert snapshot_objects()==objects_before,'Appearance controls changed document objects'
                    click('Settings');click('Appearance');click('Dark appearance');click('Close settings');capture('editor-dark-reset');click('Library ·',True)
                    print('APPEARANCE_OK: light/dark customization, alpha, validation, radius, fixed/custom/themed paper, reset, document preservation')
                click('New document')
                assert any(n.get_name()=='A5 · 148 × 210 mm' for n in controls()), 'Main New document must expose page setup'
                with sqlite3.connect(Path(root)/'notes.sqlite3') as db:
                    assert db.execute('SELECT count(*) FROM notes').fetchone()[0]==initial_notes, 'Opening setup created a hidden document'
                click('Cancel')
                with sqlite3.connect(Path(root)/'notes.sqlite3') as db:
                    assert db.execute('SELECT count(*) FROM notes').fetchone()[0]==initial_notes, 'Cancel created a document'
                click('New document')
                for char in 'new notebook':client.key('space' if char==' ' else char.lower(),1 if char.isupper() else 0)
                click('A5 · 148 × 210 mm');click('Landscape');click('Grid')
                capture('creation-options')
                if args.system_theme:
                    client.resize(1320,980);capture('creation-options-full')
                    client.resize(1000,620)
                    for _ in range(12):
                        create=next((n for n in controls() if n.get_name()=='Create document'),None)
                        rect=create.get_component_iface().get_extents(Atspi.CoordType.WINDOW) if create else None
                        if rect and rect.height>0 and rect.y>=0 and rect.y+rect.height<=620:break
                        client.click(660,400,button=5,delay=.12)
                    assert rect and rect.height>0 and rect.y>=0 and rect.y+rect.height<=620, 'Creation footer is unreachable in a compact window'
                    capture('creation-options-compact-scrolled');client.resize(1320,860)
                click('Create document')
                with sqlite3.connect(Path(root)/'notes.sqlite3') as db:
                    assert db.execute('SELECT count(*) FROM notes').fetchone()[0]==initial_notes+1
                    created=next(id_ for id_,metadata in db.execute('SELECT id,metadata FROM notes') if json.loads(metadata)['title']=='new notebook')
                    header=json.loads(db.execute('SELECT header FROM pages WHERE note_id=?',(created,)).fetchone()[0])
                    props=header['properties']
                    assert props['paper']=='Grid' and abs(props['width']-210/25.4*96)<1 and abs(props['height']-148/25.4*96)<1,props
                buttons=controls()
                assert sum(n.get_name()=='Library · Ctrl+Shift+L' for n in buttons)==1
                capture('single-home-editor')
                assert any(n.get_name()=='Undo' for n in buttons), 'Editor controls must follow notebook creation'
                undo=next(n for n in buttons if n.get_name()=='Undo')
                assert not undo.get_state_set().contains(Atspi.StateType.ENABLED), 'Empty-document Undo must be disabled'
                undo_action=undo.get_action_iface()
                assert undo_action is None or undo_action.get_n_actions()==0, 'Disabled Undo must not offer native actions'
                if args.shortcuts:
                    os.environ['GDK_BACKEND']='x11' # This check runs on the explicitly requested private X11 display.
                    gi.require_version('Gtk','3.0')
                    from gi.repository import Gtk,Gdk
                    assert Gtk.init_check([])[0], 'No private GTK display for clipboard verification'
                    client.key('n',4);time.sleep(.5)
                    assert not any(n.get_role()==Atspi.Role.ENTRY for n in walk(target)), 'Ctrl+N must create immediately without setup'
                    with sqlite3.connect(Path(root)/'notes.sqlite3') as db:
                        assert db.execute('SELECT count(*) FROM notes').fetchone()[0]==initial_notes+2, 'Ctrl+N did not create exactly one document'
                        objects_before=list(db.execute('SELECT id,data FROM objects ORDER BY id'))
                    client.key('f',4);client.key('a',4)
                    for char in 'pel':client.key(char)
                    assert any(n.get_role()==Atspi.Role.ENTRY and n.get_name()=='Search your documents' for n in walk(target)), 'Typing tool shortcuts closed the search field'
                    client.key('a',4);client.key('c',4)
                    clipboard=Gtk.Clipboard.get(Gdk.SELECTION_CLIPBOARD)
                    assert clipboard.wait_for_text()=='pel', 'Tool shortcut letters did not reach the focused field'
                    client.key('Escape');client.key('s',4);time.sleep(.3)
                    assert not any(n.get_role()==Atspi.Role.ENTRY for n in walk(target)), 'Escape did not close search'
                    with sqlite3.connect(Path(root)/'notes.sqlite3') as db:
                        assert db.execute('SELECT count(*) FROM notes').fetchone()[0]==initial_notes+2, 'Modal typing created a document'
                        assert list(db.execute('SELECT id,data FROM objects ORDER BY id'))==objects_before, 'Modal typing or saving mutated document objects'
                    initial_notes+=1
                    print('SHORTCUTS_OK: Ctrl+N, actual modal text/clipboard, Escape, Ctrl+S, document preservation',flush=True)
                if not any(n.get_name()=='Hide pages' for n in controls()):click('Page thumbnails',True)
                click('Add page')
                assert any(n.get_name()=='Go to page 2' for n in controls()),'Virtual page controls must be accessible'
                click('Go to page 1');click('Library ·',True)
                assert not any(n.get_role()==Atspi.Role.DOCUMENT_FRAME for n in walk(target)), 'Hidden canvas must not remain in the library accessibility tree'
                with sqlite3.connect(Path(root)/'notes.sqlite3') as db:assert db.execute('SELECT count(*) FROM notes').fetchone()[0]==initial_notes+1
                buttons=controls()
                result={'application':target.get_name(),'accessible_controls':len(buttons),'labels':[n.get_name() for n in buttons],'native_click_created_note':True,'native_page_sidebar_add_navigate':True,'native_return_to_library':True}
                if args.shortcuts:result['native_shortcuts_and_modal_typing']=True
                destination=ROOT/'artifacts/validation/accessibility.json';destination.parent.mkdir(parents=True,exist_ok=True);destination.write_text(json.dumps(result,indent=2)+'\n');print(json.dumps(result))
            finally:
                if args.virtual_display and 'client' in locals():client.close()
                app.terminate()
                try:app.wait(timeout=5)
                except subprocess.TimeoutExpired:app.kill();app.wait()
                registry.terminate();registry.wait(timeout=5);launcher.terminate();launcher.wait(timeout=5)
                if portal is not None:portal.terminate();portal.wait(timeout=5)
if __name__=='__main__':main()
