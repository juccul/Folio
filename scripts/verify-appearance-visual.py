#!/usr/bin/env python3
"""Capture Folio on a private X11 display with closed throwaway fixtures.
Requires FOLIO_VIRTUAL_DISPLAY=1, an empty WAYLAND_DISPLAY and a private D-Bus
session. Each variant uses a new temporary copy; no user notes are opened.
"""
import argparse
import json
import os
from pathlib import Path
import shutil
import sqlite3
import subprocess
import tempfile
import time
import sys
import importlib.util
capture_spec=importlib.util.spec_from_file_location('folio_native_capture',Path(__file__).with_name('capture-x11.py'))
native_capture=importlib.util.module_from_spec(capture_spec)
capture_spec.loader.exec_module(native_capture)
from ui_x11 import Client

ROOT = Path(__file__).resolve().parents[1]

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary',type=Path,required=True)
    parser.add_argument('--fixture',type=Path,required=True)
    parser.add_argument('--output',type=Path,required=True)
    args=parser.parse_args()
    if os.environ.get('FOLIO_VIRTUAL_DISPLAY')!='1' or os.environ.get('WAYLAND_DISPLAY'):
        parser.error('Use an isolated X11 display with FOLIO_VIRTUAL_DISPLAY=1')
    args.output.mkdir(parents=True,exist_ok=True)
    from PIL import Image
    with tempfile.TemporaryDirectory(prefix='folio-appearance-visual-') as tmp:
        root=Path(tmp)
        for name,dark,follows,paper,overrides in [
            ('light',False,True,None,{}),('dark',True,True,None,{}),
            ('dark-white-paper',True,False,None,{}),
            ('dark-custom-paper',True,False,{'r':253,'g':245,'b':230},{}),
            ('custom-dark',True,True,None,{'primary':0xff8844ff,'card':0x201f23ff,'sidebar':0x201f23ff}),
            ('large-ui',True,True,None,{}),
        ]:
            data=root/name
            shutil.copytree(args.fixture,data,ignore=shutil.ignore_patterns('session.lock','*.sqlite3-wal','*.sqlite3-shm'))
            with sqlite3.connect(data/'notes.sqlite3') as db:
                row=db.execute("SELECT data FROM settings WHERE key='preferences'").fetchone()
                settings=json.loads(row[0]) if row else {}
                settings.update(dark=dark,appearance={'canvas_follows_theme':follows,'canvas_color':paper,'dark' if dark else 'light':overrides})
                if name=='large-ui':settings['ui_scale']=1.6
                db.execute("INSERT OR REPLACE INTO settings(key,data) VALUES('preferences',?)",(json.dumps(settings),))
            with sqlite3.connect(data/'notes.sqlite3') as db:
                first_note=db.execute('SELECT id FROM notes ORDER BY rowid LIMIT 1').fetchone()[0]
            app=None;client=None
            try:
                with (args.output/(name+'-runtime.log')).open('w') as log:
                    app=subprocess.Popen([str(args.binary.resolve()),'--data-dir',str(data),'--open-note',first_note],stdout=log,stderr=log)
                    client=Client(app.pid);time.sleep(2)
                    for _ in range(5):client.wake_virtual_window()
                    client.resize(1320,860);time.sleep(1)
                    def capture(label):
                        destination=args.output/(label+'.png')
                        arguments=sys.argv
                        try:
                            sys.argv=['capture-x11.py',str(destination),'--pid',str(app.pid),'--virtual-display-root']
                            native_capture.main()
                        finally:sys.argv=arguments
                        with Image.open(destination) as picture:
                            pixels=picture.convert('RGB').resize((120,80)).tobytes()
                            assert len({pixels[i:i+3] for i in range(0,len(pixels),3)})>40,'Blank native capture'
                    capture('editor-'+name)
                    if name in ('light','dark'):
                        client.key('t',4);time.sleep(.3);capture('tab-picker-'+name);client.key('Escape')
                    client.key('p',5);time.sleep(.3);capture('pages-'+name)
                    if name in ('light','dark','large-ui'):
                        client.key('comma',4,delay=0)
                        if name=='dark':
                            for frame in range(6):
                                time.sleep(.03);capture(f'motion-settings-{frame}')
                        time.sleep(.3);capture('settings-'+name)
                        if name=='dark':
                            from PIL import ImageChops
                            with Image.open(args.output/'settings-dark.png') as settled, Image.open(args.output/'pages-dark.png') as baseline:
                                settled=settled.convert('RGB');baseline=baseline.convert('RGB')
                                intermediates=[]
                                for frame in range(6):
                                    with Image.open(args.output/f'motion-settings-{frame}.png') as picture:
                                        picture=picture.convert('RGB')
                                        # Require visible panel content, not just a frame
                                        # before the input reached the application.
                                        difference=ImageChops.difference(picture,settled).getbbox()
                                        visible=picture.getpixel((450,180)) != baseline.getpixel((450,180))
                                        if visible and difference:intermediates.append(frame)
                                assert intermediates, 'No visible intermediate panel animation frame'
                                print('FINITE_MOTION_FRAMES_OK:',intermediates,flush=True)
                            time.sleep(.2);capture('motion-settings-idle')
                            with Image.open(args.output/'motion-settings-idle.png') as idle, Image.open(args.output/'settings-dark.png') as settled:
                                assert ImageChops.difference(idle.convert('RGB'),settled.convert('RGB')).getbbox() is None, 'Animation did not settle'
                        if name=='large-ui':
                            client.key('Escape');client.resize(980,760);client.key('comma',4);time.sleep(.3);capture('settings-compact-large-ui')
                        client.key('Escape');client.key('f',4);time.sleep(.3);capture('search-'+name)
                        client.key('Escape');client.resize(980,760);time.sleep(.3);capture('compact-'+name)
                        client.resize(1320,860);client.key('l',5);time.sleep(.3);capture('library-'+name)
            finally:
                if client:client.close()
                if app and app.poll() is None:
                    app.terminate()
                    try:app.wait(timeout=5)
                    except subprocess.TimeoutExpired:app.kill();app.wait()
    print('APPEARANCE_VISUAL_OK: native editor, library, settings, fields, thumbnails, compact sizing, fixed/custom paper, custom colors, large UI scale and finite animation frames')

if __name__=='__main__':main()
