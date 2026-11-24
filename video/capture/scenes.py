#!/usr/bin/env python3
"""Record new launch-video footage from the real Folio app (headless, real time).

Reuses the tour recorder's hand (eased strokes, glyphs.rs handwriting) from
scripts/record-tour.py, but keeps the whole window: library, toolbar, tabs, menus.

    python3 video/capture/scenes.py fixture library editor dark
"""
import importlib.util
import json
import math
import shutil
import sqlite3
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location("rt", ROOT / "scripts/record-tour.py")
rt = importlib.util.module_from_spec(spec)
spec.loader.exec_module(rt)

BIN = ROOT / "target/perf/folio"
WORK = ROOT / "artifacts/capture-work"
OUT = ROOT / "video/public/clips/own"
SIZE = (1280, 800)
SCALE = 2
FPS = 30

Script, handwriting, loop_around, rough_circle = rt.Script, rt.handwriting, rt.loop_around, rt.rough_circle


def click_key(s, key, wait=0.5):
    s.ev("click", key=key)
    s.wait(wait)


DOCS = [
    # name, pattern key, paper key, handwriting lines (text, x, baseline, height, seed)
    ("Calculus · week 6", "paper-grid", "paper-white", [("2x+3=7", 320, 260, 72, 4), ("y=3x+1", 320, 400, 72, 6)]),
    ("Lab notebook", "paper-dots", "paper-sage", [("next", 320, 250, 76, 13), ("0.25+1.5", 320, 390, 70, 3)]),
    ("Reading notes", "paper-lines", "paper-cream", [("neat text", 320, 252, 74, 11), ("exact", 320, 380, 74, 9)]),
    ("Physics · kinematics", "paper-grid", "paper-blue", [("x=at+5", 320, 260, 74, 21), ("t=12", 320, 400, 74, 7)]),
    ("Ideas", "paper-blank", "paper-white", []),
]


def fixture(theme):
    """A library of a few real handwritten documents, in `theme`."""
    base = WORK / f"fixture-{theme}"
    if (base / "notes.sqlite3").exists():
        return base
    shutil.rmtree(base, ignore_errors=True)
    base.mkdir(parents=True)
    s = Script()
    s.wait(0.5)
    click_key(s, "tour-skip", 0.6)
    for i, (name, pattern, paper, lines) in enumerate(DOCS):
        if i:
            click_key(s, "tab-library", 0.8)
        click_key(s, "library-new", 0.7)
        s.type(name, every=0.02)
        s.wait(0.2)
        click_key(s, pattern, 0.3)
        if paper:
            click_key(s, paper, 0.3)
        click_key(s, "submit-modal", 1.2)
        for text, x, base_y, h, seed in lines:
            s.write(handwriting(text, x, base_y, h, seed=seed), speed=2400, pause=0.005)
            s.wait(0.3)
        if name == "Ideas":
            s.stroke(rough_circle(420, 300, 80), speed=900, hold=0.9)
            s.wait(0.6)
            s.stroke([(580 + i * 2.6, 420 - i * 1.4) for i in range(90)], speed=800, hold=0.9)
            s.wait(0.6)
        s.wait(1.5)
    click_key(s, "tab-library", 1.5)
    s.mark("end")
    (WORK / f"fixture-{theme}.json").write_text(json.dumps(s.events))
    subprocess.run([str(BIN), "--data-dir", str(base), "--script", str(WORK / f"fixture-{theme}.json"), "--realtime",
                    "--snap", str(WORK / f"fixture-{theme}.png"), "--tree", str(WORK / f"fixture-{theme}-tree.json"), "--size", f"{SIZE[0]}x{SIZE[1]}", "--scale", "1",
                    "--at", str(s.t + 0.5)], check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    (base / "session.lock").unlink(missing_ok=True)
    db = sqlite3.connect(base / "notes.sqlite3")
    prefs = json.loads(db.execute("select data from settings where key='preferences'").fetchone()[0])
    prefs.update(dark=theme == "dark", follow_system_theme=False, tour_version=1)
    db.execute("update settings set data=? where key='preferences'", (json.dumps(prefs),))
    db.commit()
    return base


def record(name, theme, script):
    base = fixture(theme)
    work = WORK / f"rec-{name}"
    shutil.rmtree(work, ignore_errors=True)
    work.mkdir(parents=True)
    shutil.copytree(base, work / "data")
    (work / "script.json").write_text(json.dumps(script.events))
    subprocess.run([str(BIN), "--data-dir", str(work / "data"), "--script", str(work / "script.json"), "--realtime",
                    "--size", f"{SIZE[0]}x{SIZE[1]}", "--scale", str(SCALE), "--fps", str(FPS),
                    "--start", "mark:rec", "--duration", "auto", "--out", str(work / "frames")], check=True,
                   stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    OUT.mkdir(parents=True, exist_ok=True)
    out = OUT / f"{name}.mp4"
    subprocess.run(["ffmpeg", "-y", "-loglevel", "error", "-framerate", str(FPS), "-i", str(work / "frames/frame_%04d.png"),
                    "-vf", "scale=1920:-2:flags=lanczos", "-c:v", "libx264", "-crf", "14", "-preset", "slow",
                    "-pix_fmt", "yuv420p", str(out)], check=True)
    frames = len(list((work / "frames").glob("frame_*.png")))
    print(name, theme, frames, "frames ->", out)


FIRST_TAB = (128, 20)  # "Calculus · week 6", the first tab in both fixtures


def scene_library():
    """Back to the shelf (documents arrive one after another), then the Ctrl+T command palette."""
    s = Script()
    s.at = (900, 600)
    s.wait(0.6)
    s.click(FIRST_TAB)
    s.wait(1.2)
    s.glide((40, 120))
    s.mark("rec")
    s.wait(0.2)
    click_key(s, "tab-library", 0.1)
    s.glide((700, 600))
    s.wait(1.8)
    s.key("t", ctrl=True)
    s.wait(1.0)
    s.key("ArrowDown")
    s.wait(0.7)
    s.key("ArrowDown")
    s.wait(0.7)
    s.key("ArrowDown")
    s.wait(1.2)
    s.key("Enter")
    s.wait(1.8)
    s.mark("end")
    return s


def scene_editor():
    """The whole editor: a new ruled page, a line of writing, a shape that snaps, a sum that answers itself."""
    s = Script()
    s.at = (1000, 300)
    s.wait(0.8)
    s.mark("rec")
    s.wait(0.2)
    s.glide((1176, 80))
    click_key(s, "library-new", 0.7)
    s.type("Notes", every=0.07)
    s.wait(0.2)
    click_key(s, "paper-lines", 0.25)
    click_key(s, "paper-theme", 0.25)
    click_key(s, "submit-modal", 1.0)
    s.write(handwriting("neat text", 330, 252, 60, seed=11), speed=1300)
    s.wait(0.2)
    s.stroke(rough_circle(860, 230, 50), speed=700, hold=0.9)
    s.wait(0.3)
    s.write(handwriting("12+30=", 330, 380, 58, seed=4), speed=1300)
    s.glide((760, 560))
    s.wait(3.4)
    s.mark("end")
    return s


def scene_dark():
    """Dark theme: open the calculus notes, lasso the equation and solve it."""
    s = Script()
    s.at = (700, 600)
    s.wait(0.6)
    s.mark("rec")
    s.wait(0.2)
    s.click(FIRST_TAB)
    s.wait(1.0)
    s.key("l")
    s.stroke(loop_around(480, 232, 220, 70, seed=4), speed=1000)
    s.wait(0.5)
    rt.SIZE, rt.WORK = SIZE, WORK
    button = rt.centre(rt.locate(s, fixture("dark"), s.t, ["solve-selection"], realtime=False)["solve-selection"])
    s.click(button)
    s.glide((button[0] + 200, button[1] + 220))
    s.wait(4.5)
    s.mark("end")
    return s


SCENES = {"library": ("light", scene_library), "editor": ("light", scene_editor), "dark": ("dark", scene_dark)}

if __name__ == "__main__":
    WORK.mkdir(parents=True, exist_ok=True)
    for arg in sys.argv[1:]:
        if arg == "fixture":
            for theme in ("light", "dark"):
                print(fixture(theme))
            continue
        theme, fn = SCENES[arg]
        record(arg, theme, fn())
