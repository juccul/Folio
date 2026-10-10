#!/usr/bin/env python3
"""Private native X11/AT-SPI verification of solver, guides and live results."""
import argparse
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
from gi.repository import Gio,GLib
from ui_x11 import Client,Event,Key
import ctypes as C

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary',type=Path,required=True)
    parser.add_argument('--fixture',type=Path,required=True)
    parser.add_argument('--output',type=Path,required=True)
    parser.add_argument('--latex',action='store_true',help='Also verify editable LaTeX result workflows')
    parser.add_argument('--edge-cases',action='store_true',help='Check selection reversal and Unicode clipboard editing')
    parser.add_argument('--graph-theme',action='store_true',help='Verify graph colors and live theme changes')
    args=parser.parse_args()
    if os.environ.get('FOLIO_VIRTUAL_DISPLAY')!='1' or os.environ.get('WAYLAND_DISPLAY'):
        parser.error('Use a private X11 display and D-Bus session.')
    args.output.mkdir(parents=True,exist_ok=True)
    processes=[];app=client=None
    try:
        processes.append(subprocess.Popen(['/usr/libexec/at-spi-bus-launcher','--launch-immediately'],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL))
        time.sleep(.3);bus=Gio.bus_get_sync(Gio.BusType.SESSION,None)
        address=bus.call_sync('org.a11y.Bus','/org/a11y/bus','org.a11y.Bus','GetAddress',None,GLib.VariantType.new('(s)'),Gio.DBusCallFlags.NONE,5000,None).unpack()[0]
        os.environ['AT_SPI_BUS_ADDRESS']=address
        gi.require_version('Atspi','2.0');from gi.repository import Atspi
        os.environ['GDK_BACKEND']='x11'
        gi.require_version('Gtk','3.0');from gi.repository import Gtk,Gdk
        assert Gtk.init_check([])[0], 'Private GTK clipboard display unavailable'
        processes.append(subprocess.Popen(['/usr/libexec/at-spi2-registryd'],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL))
        time.sleep(.3)
        bus.call_sync('org.a11y.Bus','/org/a11y/bus','org.freedesktop.DBus.Properties','Set',GLib.Variant('(ssv)',('org.a11y.Status','IsEnabled',GLib.Variant('b',True))),None,Gio.DBusCallFlags.NONE,5000,None)
        with tempfile.TemporaryDirectory(prefix='folio-math-ui-') as temporary:
            root=Path(temporary)
            shutil.copytree(args.fixture,root/'data',ignore=shutil.ignore_patterns('session.lock','*.sqlite3-wal','*.sqlite3-shm'))
            database=root/'data/notes.sqlite3'
            with sqlite3.connect(database) as db:note=db.execute('SELECT id FROM notes LIMIT 1').fetchone()[0]
            if args.graph_theme:
                with sqlite3.connect(database) as db:
                    row=db.execute("SELECT data FROM settings WHERE key='preferences'").fetchone()
                    preferences=json.loads(row[0]) if row else {}
                    preferences.update(dark=False)
                    appearance=preferences.setdefault('appearance',{})
                    appearance.update(canvas_follows_theme=True,adapt_ink=True)
                    appearance.setdefault('light',{})['primary']=0x52876cff
                    appearance.setdefault('dark',{})['primary']=0x77bdabff
                    db.execute("INSERT OR REPLACE INTO settings(key,data) VALUES('preferences',?)",(json.dumps(preferences),))
            with (args.output/'runtime.log').open('w') as log:
                app=subprocess.Popen([str(args.binary.resolve()),'--data-dir',str(root/'data'),'--open-note',note],stdout=log,stderr=log)
                client=Client(app.pid)
                for _ in range(5):client.wake_virtual_window()
                target=None
                for _ in range(60):
                    while GLib.MainContext.default().iteration(False):pass
                    desktop=Atspi.get_desktop(0)
                    target=next((desktop.get_child_at_index(i) for i in range(desktop.get_child_count()) if 'folio' in desktop.get_child_at_index(i).get_name().lower()),None)
                    if target:break
                    time.sleep(.1)
                assert target is not None,'No native accessibility tree'
                def walk(node):
                    node.clear_cache();yield node
                    for i in range(node.get_child_count()):yield from walk(node.get_child_at_index(i))
                def nodes():
                    while GLib.MainContext.default().iteration(False):pass
                    return list(walk(target))
                def click(label):
                    matches=[node for node in nodes() if node.get_role() in (Atspi.Role.PUSH_BUTTON,Atspi.Role.RADIO_BUTTON,Atspi.Role.CHECK_BOX,Atspi.Role.TOGGLE_BUTTON) and node.get_name()==label]
                    item=matches[-1] if matches else None
                    assert item is not None,f'Missing {label}: {[n.get_name() for n in nodes()]}'
                    assert item.get_action_iface().do_action(0);time.sleep(.25)
                def fill(label,text):
                    item=next((node for node in nodes() if node.get_name()==label),None)
                    assert item is not None,f'Missing field {label}'
                    assert item.get_component_iface().grab_focus(),f'Cannot focus {label}'
                    time.sleep(.1);client.key('a',4,delay=.03);client.key('BackSpace',delay=.03)
                    symbols={' ':'space','=':'equal',':':'colon','*':'asterisk','+':'plus','-':'minus','/':'slash','^':'asciicircum','(':'parenleft',')':'parenright','.':'period',',':'comma','\n':'Return'}
                    shifted=':*+^()'
                    for char in text:
                        client.key(symbols.get(char,char.lower()),1 if char in shifted or char.isupper() else 0,delay=.06)
                    time.sleep(.2);client.key('a',4,delay=.03);client.key('c',4,delay=.1)
                    actual=Gtk.Clipboard.get(Gdk.SELECTION_CLIPBOARD).wait_for_text()
                    assert actual==text,f'Native input mismatch: {actual!r} != {text!r}'
                def wait_result():
                    for _ in range(200):
                        if any(n.get_name()=='Copy LaTeX' for n in nodes()):return
                        time.sleep(.05)
                    (args.output/'tree.json').write_text(json.dumps([{'name':n.get_name(),'role':n.get_role_name()} for n in nodes()],indent=2))
                    subprocess.run([sys.executable,'scripts/capture-x11.py',str(args.output/'failure.png'),'--pid',str(app.pid),'--virtual-display-root'],check=True)
                    raise AssertionError('No math result; see runtime log')
                def scroll(ticks):
                    # The assertion at startup confines these XI2 wheel events to this
                    # temporary Xvfb display; never connect to the user's display.
                    assert os.environ['DISPLAY'].startswith(':13') or os.environ['DISPLAY'].startswith(':14')
                    xt=C.CDLL('libXtst.so.6')
                    xt.XTestFakeMotionEvent.argtypes=[C.c_void_p,C.c_int,C.c_int,C.c_int,C.c_ulong]
                    xt.XTestFakeButtonEvent.argtypes=[C.c_void_p,C.c_uint,C.c_int,C.c_ulong]
                    xt.XTestFakeMotionEvent(client.display,-1,1150,650,0)
                    for _ in range(abs(ticks)):
                        for state in (1,0):xt.XTestFakeButtonEvent(client.display,5 if ticks>0 else 4,state,0)
                        client.x.XFlush(client.display);time.sleep(.08)
                    time.sleep(.3)
                def objects():
                    with sqlite3.connect(database) as db:return [json.loads(data) for (data,) in db.execute('SELECT data FROM objects')]
                def paste(label,text):
                    item=next(n for n in nodes() if n.get_name()==label)
                    assert item.get_component_iface().grab_focus()
                    time.sleep(.1)
                    clipboard=Gtk.Clipboard.get(Gdk.SELECTION_CLIPBOARD)
                    clipboard.set_text(text,-1)
                    client.key('a',4,delay=.03);client.key('v',4,delay=.03)
                    for _ in range(30):
                        while GLib.MainContext.default().iteration(False):pass
                        time.sleep(.01)
                def status_contains(text):
                    return any(text in n.get_name() and n.get_role()==Atspi.Role.STATUS_BAR for n in nodes())
                def wait_status(text):
                    for _ in range(150):
                        if status_contains(text):return
                        time.sleep(.04)
                    raise AssertionError(f'Missing status {text}: {[n.get_name() for n in nodes()]}')
                def enabled(label):
                    item=next(n for n in nodes() if n.get_role()==Atspi.Role.PUSH_BUTTON and n.get_name()==label)
                    return item.get_state_set().contains(Atspi.StateType.ENABLED)
                client.key('a',4);click('Size');fill('Font size','nan');click('Save')
                time.sleep(.8)
                assert any(n.get_name()=='Enter a font size between 6 and 180 canvas pixels.' for n in nodes()),'Layout updates cleared the validation error'
                assert any(n.get_name()=='Font size' and n.get_role()==Atspi.Role.ENTRY for n in nodes()),'Invalid input closed the dialog'
                subprocess.run([sys.executable,'scripts/capture-x11.py',str(args.output/'validation.png'),'--pid',str(app.pid),'--virtual-display-root'],check=True)
                fill('Font size','20');click('Save');click('Solve')
                click('Select · L');client.key('a',4);time.sleep(.3)
                labels=[n.get_name() for n in nodes()]
                assert {'Cut','Copy','Delete','Solve'}.issubset(labels),'Math solver hides the selection popup'
                assert 'Real' not in labels and 'Define variable' not in labels
                assert 'Next mathematical line' not in labels and 'Graph x range' not in labels
                if args.edge_cases:
                    clipboard=Gtk.Clipboard.get(Gdk.SELECTION_CLIPBOARD)
                    for text,expected in [('abcdef','f'),('ab👩‍💻é','é')]:
                        paste('Math expression',text)
                        client.key('End');client.key('Left',1);client.key('Left',1);client.key('Right',1)
                        client.key('c',4);time.sleep(.2)
                        assert clipboard.wait_for_text()==expected,(text,clipboard.wait_for_text())
                fill('Math expression','4/2');click('Use selection')
                click('Solve problem');wait_result()
                assert any('Solution: {4}' in n.get_name() for n in nodes()), 'Use selection did not restore the selected problem'
                def bounds(label):
                    item=next(n for n in nodes() if n.get_role()==Atspi.Role.PUSH_BUTTON and n.get_name()==label)
                    rect=item.get_component_iface().get_extents(Atspi.CoordType.WINDOW)
                    return (rect.x,rect.y,rect.width,rect.height)
                primary_before=bounds('Solve problem');copy_before=bounds('Copy LaTeX')
                subprocess.run([sys.executable,'scripts/capture-x11.py',str(args.output/'solution.png'),'--pid',str(app.pid),'--virtual-display-root'],check=True)
                if args.latex:
                    click('Edit LaTeX');click('Copy LaTeX')
                    source=next(n for n in nodes() if n.get_name()=='Result LaTeX source').get_component_iface().get_extents(Atspi.CoordType.WINDOW)
                    assert source.y+source.height<=bounds('Apply changes')[1],'Source editor is covered by its footer'
                    assert Gtk.Clipboard.get(Gdk.SELECTION_CLIPBOARD).wait_for_text()=='x = 4'
                    assert not enabled('Apply changes')
                    latex=r'x = \frac{8}{2}'
                    paste('Result LaTeX source',latex)
                    labels=[n.get_name() for n in nodes()]
                    assert enabled('Apply changes') and 'Add answer' not in labels and 'Math expression' not in labels
                    assert labels.count('Copy LaTeX')==1,'Editing exposes duplicate copy actions'
                    click('Copy LaTeX')
                    assert Gtk.Clipboard.get(Gdk.SELECTION_CLIPBOARD).wait_for_text()==latex
                    click('Apply changes');wait_status('Edited result:')
                    assert status_contains('not been verified')
                    assert enabled('Add answer') and not any(n.get_name()=='Apply changes' for n in nodes())
                    assert not any(n.get_name()=='Result LaTeX source' for n in nodes()),'Apply did not return to the answer'
                    click('Copy LaTeX')
                    assert Gtk.Clipboard.get(Gdk.SELECTION_CLIPBOARD).wait_for_text()==latex
                    assert not any(n.get_name()=='Add options' for n in nodes()),'Manual edit still offers live/verified steps'
                    subprocess.run([sys.executable,'scripts/capture-x11.py',str(args.output/'latex-editor.png'),'--pid',str(app.pid),'--virtual-display-root'],check=True)
                    click('Add answer')
                    assert len(objects())==2
                    equation=next(obj['Equation'] for obj in objects() if 'Equation' in obj)
                    assert equation['latex']==latex and equation.get('math_link') is None and '<path' in equation['rendered_svg']
                    client.key('z',4);time.sleep(.3);assert len(objects())==1,'Applied editor left focus on a hidden field'
                    click('Redo');assert len(objects())==2
                    click('Undo')
                    click('Edit LaTeX');paste('Result LaTeX source',r'\frac{');click('Apply changes')
                    wait_status('Equation rendering:')
                    time.sleep(.6)
                    assert status_contains('Equation rendering:') and not any(n.get_name()=='Add answer' for n in nodes())
                    subprocess.run([sys.executable,'scripts/capture-x11.py',str(args.output/'latex-validation.png'),'--pid',str(app.pid),'--virtual-display-root'],check=True)
                    click('Cancel editing');click('Copy LaTeX')
                    assert Gtk.Clipboard.get(Gdk.SELECTION_CLIPBOARD).wait_for_text()==latex
                    assert enabled('Add answer')
                    # New results must replace the source editor value and restore teaching.
                    click('Solve problem');wait_status('Solution: {4}')
                    click('Edit LaTeX');click('Copy LaTeX')
                    assert Gtk.Clipboard.get(Gdk.SELECTION_CLIPBOARD).wait_for_text()=='x = 4'
                    click('Cancel editing')
                    primary_before=bounds('Solve problem');copy_before=bounds('Copy LaTeX')
                click('Guide me');wait_status('Step 1 of 4')
                assert not enabled('Previous step') and enabled('Next step')
                click('Hint');click('Next step');wait_status('Step 2 of 4')
                assert enabled('Previous step')
                subprocess.run([sys.executable,'scripts/capture-x11.py',str(args.output/'guided.png'),'--pid',str(app.pid),'--virtual-display-root'],check=True)
                click('Previous step');wait_status('Step 1 of 4')
                for _ in range(3):click('Next step')
                wait_status('Step 4 of 4');assert not enabled('Next step') and not enabled('Hint')
                click('Show all steps')
                assert len(objects())==1,'Solving changed the source'
                click('Explain steps');scroll(7)
                assert bounds('Solve problem')==primary_before and bounds('Copy LaTeX')==copy_before,'Problem or result actions scrolled away'
                subprocess.run([sys.executable,'scripts/capture-x11.py',str(args.output/'algebra.png'),'--pid',str(app.pid),'--virtual-display-root'],check=True)
                click('Copy LaTeX')
                click('Check work')
                assert any(n.get_name()=='Next mathematical line' for n in nodes())
                # Native field focus and keyboard input are exercised above.
                fill('Next mathematical line','2x=8');click('Check next step');wait_result()
                click('Solution');click('Solve problem');wait_result();click('Add options');click('Answer and steps')
                count=len(objects());assert count>2,'Worked steps were not inserted'
                assert any('Equation' in obj and '<path' in (obj['Equation'].get('rendered_svg') or '') for obj in objects())
                client.key('z',4);time.sleep(.3);assert len(objects())==1,'Worked solution did not undo atomically'
                client.key('z',5);time.sleep(.3);assert len(objects())==count
                client.key('z',4);time.sleep(.3)
                fill('Math expression','a=5');click('Operation: Automatic');click('Define variable');click('Preview variable');wait_result();click('Add variable')
                assignment=next(obj['Equation'] for obj in objects() if obj.get('Equation',{}).get('math_link',{}).get('operation')=='assign')
                assert assignment['math_link']['expression']=='a:=5'
                click('Operation: Variable');click('Solve automatically')
                fill('Math expression','2a=');client.key('Return',4);wait_result();click('Add options');click('Live answer')
                assert any(obj.get('Equation',{}).get('latex')=='10' and obj['Equation'].get('math_link',{}).get('live') for obj in objects())
                click('Graph');fill('Math expression','y=1/x');click('Plot graph');wait_result()
                subprocess.run([sys.executable,'scripts/capture-x11.py',str(args.output/'graph.png'),'--pid',str(app.pid),'--virtual-display-root'],check=True)
                click('Add options');click('Live graph')
                assert any(obj.get('Equation',{}).get('math_link',{}).get('operation')=='graph' for obj in objects())
                if args.graph_theme:
                    from PIL import Image
                    from collections import Counter
                    saved_graphs=[obj for obj in objects() if obj.get('Equation',{}).get('math_link',{}).get('operation')=='graph']
                    def graph_colors(name,paper,curve):
                        time.sleep(.7)
                        path=args.output/f'graph-{name}.png'
                        subprocess.run([sys.executable,'scripts/capture-x11.py',str(path),'--pid',str(app.pid),'--virtual-display-root'],check=True)
                        picture=Image.open(path).convert('RGB')
                        field=next(n for n in nodes() if n.get_name()=='Math expression').get_component_iface().get_extents(Atspi.CoordType.WINDOW)
                        _,top,_,height=bounds('Plot graph')
                        _,bottom,_,_=bounds('Add graph')
                        region=picture.crop((field.x,top+height,field.x+field.width,bottom))
                        colors=Counter(region.getdata())
                        assert colors[paper]>20000,f'{name} preview does not match paper: {colors.most_common(8)}'
                        assert colors[curve]>30,f'{name} curve does not match accent: {colors.most_common(8)}'
                        assert colors[(36,99,174)]==0,'Graph still uses the old blue curve'
                        canvas_colors=Counter(picture.crop((53,176,847,826)).getdata())
                        assert canvas_colors[(36,99,174)]==0,'Inserted graph still uses the old blue curve'
                        if paper!=(255,255,255):
                            assert canvas_colors[(255,255,255)]<1000,'Inserted graph kept its white background'
                        assert [obj for obj in objects() if obj.get('Equation',{}).get('math_link',{}).get('operation')=='graph']==saved_graphs,'Changing appearance rewrote saved graphs'
                    def global_settings():
                        item=next(n for n in nodes() if n.get_role()==Atspi.Role.PUSH_BUTTON and n.get_name()=='Settings')
                        assert item.get_action_iface().do_action(0);time.sleep(.25)
                    graph_colors('light',(255,255,255),(82,135,108))
                    global_settings();click('Dark appearance');click('Done')
                    graph_colors('dark',(10,10,10),(119,189,171))
                    global_settings();click('Paper follows appearance: On');click('Custom paper color')
                    paste('Paper color','#182522');click('Save');click('Done')
                    graph_colors('custom',(24,37,34),(119,189,171))
                    global_settings();click('Light appearance');click('Done')
                    graph_colors('fixed-light',(24,37,34),(82,135,108))
                click('Solution');fill('Math expression','twice a number plus 3 is 11');click('Solve problem')
                for _ in range(100):
                    if any(n.get_name()=='Solve this equation' for n in nodes()):break
                    time.sleep(.05)
                click('Solve this equation');wait_result()
                subprocess.run([sys.executable,'scripts/capture-x11.py',str(args.output/'word-problem.png'),'--pid',str(app.pid),'--virtual-display-root'],check=True)
                click('Settings');assert any(n.get_name()=='Target variable' for n in nodes());click('Degrees');click('Settings')
                fill('Math expression','sin(30)');client.key('Return',4);wait_result()
                assert any('1/2' in n.get_name() for n in nodes()),'Degree calculation failed'
                click('Graph');fill('Graph x range','10, -10');click('Plot graph')
                assert any('smaller one first' in n.get_name() for n in nodes()),'Invalid graph range was accepted'
                click('Solution');fill('Math expression','20 percent of 50');client.key('Return',4)
                for _ in range(100):
                    if any(n.get_name()=='Solve this equation' for n in nodes()):break
                    time.sleep(.05)
                click('Solve this equation');wait_result()
                assert any('10' in n.get_name() and n.get_role()==Atspi.Role.STATUS_BAR for n in nodes()),'Percentage prose was treated as symbols'
                fill('Math expression','4/2');client.key('Return',4);wait_result()
                assert any('2' in n.get_name() and n.get_role()==Atspi.Role.STATUS_BAR for n in nodes()),'Hidden graph range blocked solving'
                paste('Math expression','')
                assert any(n.get_name()=='Try example' for n in nodes()),'Empty problem offers no starting point'
                subprocess.run([sys.executable,'scripts/capture-x11.py',str(args.output/'empty.png'),'--pid',str(app.pid),'--virtual-display-root'],check=True)
                click('Try example');wait_result();wait_status('Solution: {4}')
                click('Copy LaTeX');assert Gtk.Clipboard.get(Gdk.SELECTION_CLIPBOARD).wait_for_text()=='x = 4'
                # Each workflow's example must run with its own required inputs.
                example_count=len(objects())
                click('Graph');paste('Math expression','');click('Try example');wait_result()
                assert enabled('Add graph'),'Graph example did not produce an insertable graph'
                range_field=next(n for n in nodes() if n.get_name()=='Graph x range')
                assert range_field.get_component_iface().grab_focus();time.sleep(.1)
                client.key('a',4);client.key('c',4)
                assert Gtk.Clipboard.get(Gdk.SELECTION_CLIPBOARD).wait_for_text()=='-10, 10','Graph example kept the invalid range'
                click('Check work');paste('Math expression','');click('Try example');wait_result()
                assert enabled('Add answer'),'Check-work example did not produce a valid next step'
                click('Solution');paste('Math expression','');click('Try example');wait_result();wait_status('Solution: {4}')
                assert len(objects())==example_count,'Examples mutated the page'
                client.resize(980,760);time.sleep(.5)
                for label in ('Solve problem','Copy LaTeX','Add answer'):
                    x,y,w,h=bounds(label);assert x>=0 and y>=0 and x+w<=980 and y+h<=760,f'{label} is clipped at compact size: {(x,y,w,h)}'
                subprocess.run([sys.executable,'scripts/capture-x11.py',str(args.output/'compact.png'),'--pid',str(app.pid),'--virtual-display-root'],check=True)
                if args.latex:
                    click('Edit LaTeX');time.sleep(.3)
                    source=next(n for n in nodes() if n.get_name()=='Result LaTeX source').get_component_iface().get_extents(Atspi.CoordType.WINDOW)
                    assert source.y+source.height<=bounds('Apply changes')[1],'Compact LaTeX editor is covered by its footer'
                    for label in ('Apply changes','Cancel editing','Copy LaTeX'):
                        x,y,w,h=bounds(label);assert x>=0 and y>=0 and x+w<=980 and y+h<=760,f'{label} is clipped'
                    subprocess.run([sys.executable,'scripts/capture-x11.py',str(args.output/'latex-compact.png'),'--pid',str(app.pid),'--virtual-display-root'],check=True)
                result={'native_solver':True,'selection_actions_with_solver':True,'dialog_validation_persists':True,'progressive_disclosure':True,'pinned_problem_and_actions':True,
                    'guided_learning':True,'bidirectional_guide':True,'working_examples':True,'clear_add_options':True,'keyboard_submit':True,'compact_layout':True,'hidden_range_independent':True,'source_preserved':True,'worked_solution_undo_redo':True,
                    'rendered_steps':True,'next_step_check':True,'page_variable':True,'live_result':True,
                    'graph':True,'word_problem_review':True,'private_display':True}
                if args.edge_cases: result.update(selection_reversal=True,unicode_grapheme_clipboard=True)
                if args.latex: result.update(editable_latex=True,latex_copy=True,latex_apply=True,invalid_latex_preserves_result=True,latex_static_equation_undo_redo=True,source_editor_resets=True,focused_editor=True,apply_returns_to_answer=True)
                if args.graph_theme: result.update(graph_light_palette=True,graph_dark_palette=True,graph_custom_paper=True,graph_fixed_paper_theme_switch=True,graph_source_preserved=True)
                (args.output/'result.json').write_text(json.dumps(result,indent=2)+'\n');print(json.dumps(result))
    finally:
        if client:client.close()
        if app and app.poll() is None:
            app.terminate()
            try:app.wait(timeout=5)
            except subprocess.TimeoutExpired:app.kill();app.wait()
        for process in reversed(processes):process.terminate();process.wait(timeout=5)

if __name__=='__main__':main()
