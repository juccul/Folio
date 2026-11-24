#!/usr/bin/env python3
"""Record the tour's clips from the real app, in light and dark.

Each clip scripts an optimized tooling build of Folio headlessly in real time
(`--realtime`, so holds, recognition, auto math and the solver run as they do for
a person, and its own animations keep pace with the frames), at twice the
window's size. The hand writes with handwriting laid out from the curated glyph
samples in `crates/app/src/auto_math/glyphs.rs`; every stroke and every pointer
move eases in and out. A virtual camera then follows each scene's cues with
gentle eased zooms and pans on a spring, and the frames are encoded as looping animated WebP
into `crates/ui/assets/tour/{clip}-{light,dark}.webp`, which the tour embeds.

    cargo build --profile perf -p folio --features folio/tooling
    python3 scripts/record-tour.py [--reframe] [CLIP ...]

`--reframe` keeps the frames already captured and only reruns the camera and
encoding.

Needs `ffmpeg` with libwebp and Pillow; the solve clip needs the development math
pack (`scripts/setup-math-solver.py`). Work files go to `artifacts/tour/`.
"""
import json
import math
import os
import re
import shutil
import sqlite3
import subprocess
import sys
from pathlib import Path

from PIL import Image

ROOT = Path(__file__).resolve().parents[1]
BIN = ROOT / "target/perf/folio"
WORK = ROOT / "artifacts/tour"
OUT = ROOT / "crates/ui/assets/tour"
SIZE = (1100, 700)
FPS = 30
# Frames are rendered at this many pixels per point, so the camera can zoom in
# without blurring.
SCALE = 2
# The clips' pixels: the tour's 600 x 375 pt stage at 1.6x.
OUT_PX = (960, 600)
# How briskly the camera's spring follows its cues (rad/s): it settles in
# about 0.5 s, smoothing each cue's start without dragging behind it.
CAMERA_SPRING = 9.0
# Seconds the clip takes to rise out of the paper and to sink back into it.
FADE_IN, FADE_OUT = 0.3, 0.4
# The page's left edge and its rulings at 100% in the base document.
PAGE_X = 152
RULE = lambda n: 124 + 32 * n

# ------------------------------------------------------------ handwriting


def load_glyphs():
    """The glyph archetypes of glyphs.rs: char -> [(advance, [stroke points])]."""
    src = (ROOT / "crates/app/src/auto_math/glyphs.rs").read_text()
    strokes = {
        name: [tuple(map(float, p)) for p in re.findall(r"\[([-\d.]+), ([-\d.]+)\]", body)]
        for name, body in re.findall(r"const (S_\w+): &\[\[f32; 2\]\] = &\[(.*?)\];", src)
    }
    variants = {
        name: (float(adv), [strokes[s] for s in re.findall(r"S_\w+", body)])
        for name, adv, body in re.findall(
            r"const (V_\w+): GlyphVariant = GlyphVariant \{ advance: ([\d.]+), strokes: &\[(.*?)\] \};", src
        )
    }
    glyphs = {
        name: [variants[v] for v in re.findall(r"V_\w+", body)]
        for name, body in re.findall(r"const (GLYPH_\w+): &\[GlyphVariant\] = &\[(.*?)\];", src)
    }
    table = {}
    for chars, name in re.findall(r"((?:'.' \| )*'.') => Some\((GLYPH_\w+)\)", src):
        for c in re.findall(r"'(.)'", chars):
            table[c] = glyphs[name]
    return table


GLYPHS = load_glyphs()


def unit(seed, salt):
    x = (seed * 0x9E3779B97F4A7C15 + salt * 0xBF58476D1CE4E5B9) & 0xFFFFFFFFFFFFFFFF
    x ^= x >> 31
    x = (x * 0x94D049BB133111EB) & 0xFFFFFFFFFFFFFFFF
    x ^= x >> 29
    return (x & 0xFFFFFF) / float(0x1000000)


def catmull(points, subdivisions=4):
    if len(points) < 3:
        return list(points)
    out = []
    at = lambda i: points[max(0, min(len(points) - 1, i))]
    for i in range(len(points) - 1):
        p0, p1, p2, p3 = at(i - 1), at(i), at(i + 1), at(i + 2)
        for s in range(subdivisions):
            t = s / subdivisions
            t2, t3 = t * t, t * t * t
            f = lambda a, b, c, d: 0.5 * (2 * b + (c - a) * t + (2 * a - 5 * b + 4 * c - d) * t2 + (3 * b - a - 3 * c + d) * t3)
            out.append((f(p0[0], p1[0], p2[0], p3[0]), f(p0[1], p1[1], p2[1], p3[1])))
    out.append(points[-1])
    return out


PREFER = {"n": 0, "t": 0, "a": 0}


def handwriting(text, x, baseline, h, seed=7, slant=0.12):
    """Strokes (lists of window points) writing `text`, as glyphs.rs lays an answer out:
    a varied archetype per character, size and slant jitter, a slow tremor, and
    spacing. `=` is two dashes."""
    strokes = []
    top = baseline - h
    cursor = x
    for idx, ch in enumerate(text):
        if ch == " ":
            cursor += h * 0.45
            continue
        if ch == "=":
            dash = GLYPHS["-"][(seed + idx) % len(GLYPHS["-"])]
            gh = h * 0.68
            for k, dy in enumerate((0.42, 0.66)):
                pts = [(cursor + px * gh, top + (dy + (py - 0.5) * 0.25) * h + k * 1.5) for px, py in dash[1][0]]
                strokes.append(catmull(pts))
            cursor += dash[0] * gh + h * 0.17
            continue
        variants = GLYPHS.get(ch)
        if not variants:
            cursor += h * 0.4
            continue
        # Some archetypes read as another letter at a glance (an "n" like an
        # "r", an "a" like a "q"); those letters keep their clearest one.
        pick = PREFER.get(ch, (seed + idx * 17) % len(variants))
        adv, shape = variants[pick]
        h_mul, y_off = {"e": (0.68, 0.30), "x": (0.68, 0.30), "z": (0.68, 0.30), "a": (0.68, 0.30),
                         "c": (0.68, 0.30), "n": (0.68, 0.30), "y": (0.86, 0.30), "b": (0.92, 0.06),
                         "t": (0.92, 0.06), "k": (0.92, 0.06), "+": (0.68, 0.16)}.get(ch, (1.0, 0.0))
        salt = (idx + 1) * 31
        gh = h * h_mul * (1 + (unit(seed, salt) - 0.5) * 0.06)
        gy = top + y_off * h + (unit(seed, salt + 1) - 0.5) * 0.045 * h
        sl = slant + (unit(seed, salt + 2) - 0.5) * 0.05
        phase = unit(seed, salt + 3) * math.tau
        for si, raw in enumerate(shape):
            dense = catmull(raw, 4)
            pts = []
            for pi, (nx, ny) in enumerate(dense):
                frac = pi / max(1, len(dense) - 1)
                wx = math.sin(frac * math.tau + phase + si) * 0.008 * gh
                wy = math.cos(frac * math.pi * 1.5 + phase) * 0.006 * gh
                pts.append((cursor + (nx + (0.5 - ny) * sl) * gh + wx, gy + ny * gh + wy))
            strokes.append(pts)
        cursor += adv * gh + h * (0.17 + (unit(seed, salt + 4) - 0.5) * 0.03)
    return strokes


# ------------------------------------------------------------ the hand


def smooth(k):
    """Ease in and out: slow away, quick through the middle, slow to rest."""
    return k * k * k * (k * (6 * k - 15) + 10)


def ease_out_expo(k):
    """The camera's curve: it sets off at once and settles long and soft."""
    return 1.0 if k >= 1 else 1 - 2 ** (-10 * k)


class Script:
    """Timed pointer and key events, moved the way a hand moves: every stroke
    and every hover eases in and out, hovers bend a little, and the camera's
    cues ride along."""

    STEP = 1 / 120
    # Pen samples while drawing: about 2 points apart at writing speed, so
    # curves stay round rather than turning into straight segments.
    INK_STEP = 1 / 480

    def __init__(self):
        self.events = []
        self.cams = []
        self.t = 0.0
        self.at = (SIZE[0] / 2, SIZE[1] * 0.8)

    def ev(self, kind, **kw):
        self.events.append({"t": round(self.t, 4), "type": kind, **kw})

    def wait(self, s):
        self.t += s

    def mark(self, name):
        self.events.append({"t": round(self.t, 4), "type": "mark", "name": name})

    def camera(self, x, y, zoom, dur=2.0, at=None):
        """The camera eases to look at (`x`, `y`) at `zoom` over `dur` seconds,
        from now (or from `at`)."""
        start = self.t if at is None else at
        self.cams.append((start, start + dur, (x, y, zoom)))

    def path(self, pts, dur, step=None):
        """Moves through `pts` in `dur` seconds, eased along their length."""
        lengths = [0.0]
        for a, b in zip(pts, pts[1:]):
            lengths.append(lengths[-1] + math.hypot(b[0] - a[0], b[1] - a[1]))
        total = max(lengths[-1], 0.5)
        n = max(2, int(dur / (step or self.STEP)))
        j = 0
        for i in range(1, n + 1):
            d = smooth(i / n) * total
            while j < len(lengths) - 2 and lengths[j + 1] < d:
                j += 1
            seg = max(lengths[j + 1] - lengths[j], 1e-6)
            u = min(1.0, max(0.0, (d - lengths[j]) / seg))
            a, b = pts[j], pts[j + 1]
            self.t += dur / n
            self.ev("move", x=round(a[0] + (b[0] - a[0]) * u, 2), y=round(a[1] + (b[1] - a[1]) * u, 2))
        self.at = pts[-1]

    def glide(self, to, speed=1500.0, lift=0.12, least=0.07):
        """Hovers from where the hand is to `to` along a gentle arc."""
        (x0, y0), (x1, y1) = self.at, to
        d = math.hypot(x1 - x0, y1 - y0)
        if d < 0.5:
            self.at = to
            return
        dur = max(least, min(0.6, 0.06 + d / speed))
        nx, ny = (-(y1 - y0) / d, (x1 - x0) / d)
        bend = d * lift
        arc = [(x0 + (x1 - x0) * k + nx * math.sin(math.pi * k) * bend,
                y0 + (y1 - y0) * k + ny * math.sin(math.pi * k) * bend) for k in (i / 24 for i in range(25))]
        self.path(arc, dur)

    def stroke(self, pts, speed=900.0, hold=0.0, least=0.08):
        """Draws `pts` with the button down, easing in and out."""
        self.glide(pts[0])
        self.wait(0.02)
        self.ev("down", x=round(pts[0][0], 2), y=round(pts[0][1], 2))
        length = sum(math.hypot(b[0] - a[0], b[1] - a[1]) for a, b in zip(pts, pts[1:]))
        self.path(pts, max(least, length / speed), self.INK_STEP)
        if hold:
            self.t += hold
        self.ev("up", x=round(self.at[0], 2), y=round(self.at[1], 2))
        self.t += 0.02

    def write(self, strokes, speed=900.0, pause=0.015):
        for s in strokes:
            self.stroke(s, speed)
            self.wait(pause)

    def click(self, xy, before=0.1):
        self.glide(xy)
        self.wait(before)
        self.ev("down", x=round(xy[0], 2), y=round(xy[1], 2))
        self.wait(0.09)
        self.ev("up", x=round(xy[0], 2), y=round(xy[1], 2))
        self.wait(0.05)

    def drag(self, to, dur=0.7):
        """Presses where the hand is and drags to `to`, easing."""
        x0, y0 = self.at
        self.ev("down", x=round(x0, 2), y=round(y0, 2))
        self.wait(0.05)
        self.path([(x0, y0), to], dur, self.INK_STEP)
        self.ev("up", x=round(to[0], 2), y=round(to[1], 2))
        self.wait(0.03)

    def key(self, name, **mods):
        self.ev("key", name=name, **mods)
        self.wait(0.05)

    def type(self, text, every=0.12):
        self.events.append({"t": round(self.t, 4), "type": "type", "text": text, "every": every})
        self.t += every * len(text) + 0.05

    def view_at(self, t, first):
        """Where the camera's cues aim at time `t`: (x, y, zoom). Each cue sets
        off from wherever the one before it had got to when it began, so a cue
        that cuts the last one short carries on from there rather than
        jumping."""
        view = first
        cams = sorted(self.cams)
        for i, (start, end, target) in enumerate(cams):
            if t <= start:
                break
            stop = min(t, cams[i + 1][0]) if i + 1 < len(cams) else t
            k = ease_out_expo(min(1.0, (stop - start) / (end - start)))
            view = tuple(a + (b - a) * k for a, b in zip(view, target))
        return view


def loop_around(cx, cy, rx, ry, start=-math.pi / 2, turns=1.08, wobble=0.05, seed=3):
    """A hand-drawn closed loop, a little past its start."""
    n = 64
    pts = []
    for i in range(n + 1):
        k = i / n
        a = start - k * math.tau * turns
        w = 1 + wobble * math.sin(k * 11 + seed) + 0.02 * math.sin(k * 23)
        pts.append((cx + rx * w * math.cos(a), cy + ry * w * math.sin(a) + (k - 0.5) * 4))
    return pts


def scribble(x0, x1, y, height, seed=5):
    """A real scratch-out: quick looping passes back and forth across a word,
    their height breathing, the turns rounded, the hand drifting down a little."""
    pts = []
    passes = 7
    n = 26 * passes
    for i in range(n + 1):
        k = i / n
        sweep = 0.5 - 0.5 * math.cos(k * passes * math.pi)
        x = x0 + (x1 - x0) * sweep + math.sin(k * 37 + seed) * 3
        amp = height * (0.55 + 0.25 * math.sin(k * 9 + seed) + 0.2 * unit(seed, i // 26))
        y_ = y + math.sin(k * passes * math.pi * 3.1) * amp * 0.5 + (k - 0.5) * height * 0.3
        pts.append((x, y_))
    return pts


def rough_circle(cx, cy, r, seed=2):
    pts = []
    n = 72
    for i in range(n + 1):
        k = i / n
        a = -math.pi * 0.62 + k * math.tau * 1.03
        rr = r * (1 + 0.07 * math.sin(k * 7 + seed) + 0.04 * math.sin(k * 13))
        pts.append((cx + rr * 1.15 * math.cos(a), cy + rr * 0.9 * math.sin(a)))
    return pts


# ------------------------------------------------------------ the scenes
#
# Each scene returns its script and the camera's first view (x, y, zoom), in
# window points; zoom 1 shows 720 x 450 points. The camera only complements
# the action: a cue or two per scene, never past 1.2x, on an ease-out-expo
# curve over two seconds.

VIEW = (720.0, 450.0)


def locate(script, base, at, keys, realtime=True):
    """Where the elements `keys` are `at` seconds into `script`, as the app lays
    them out: their (x, y, w, h) in window points."""
    probe = WORK / "probe"
    shutil.rmtree(probe, ignore_errors=True)
    shutil.copytree(base, probe)
    (WORK / "probe.json").write_text(json.dumps(script.events))
    args = [str(BIN), "--data-dir", str(probe), "--script", str(WORK / "probe.json"), "--tree", str(WORK / "probe-tree.json"),
            "--size", f"{SIZE[0]}x{SIZE[1]}", "--scale", "1", "--at", str(at)]
    if realtime:
        args.append("--realtime")
    subprocess.run(args, check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    tree = json.loads((WORK / "probe-tree.json").read_text())
    found = {}

    def walk(node):
        if isinstance(node, dict):
            if node.get("key") in keys and "rect" in node:
                found.setdefault(node["key"], node["rect"])
            for v in node.values():
                walk(v)
        elif isinstance(node, list):
            for v in node:
                walk(v)

    walk(tree.get("elements"))
    missing = [k for k in keys if k not in found]
    if missing:
        raise SystemExit(f"not on screen at {at}: {missing}")
    return found


def centre(rect, dx=0.0):
    x, y, w, h = rect
    return (x + w * (0.3 + dx), y + h * 0.5)


def scene_write(base):
    s = Script()
    s.at = (330, 470)
    s.wait(0.3)
    s.mark("rec")
    s.wait(0.35)
    s.camera(480, 330, 1.12)
    s.write(handwriting("neat", 380, RULE(7) - 4, 58, seed=11))
    s.wait(0.25)
    s.camera(490, 350, 1.15)
    s.write(handwriting("12+30", 380, RULE(10) - 4, 50, seed=4))
    s.glide((640, 430))
    s.wait(1.8)
    s.mark("end")
    return s, (520, 350, 1.0)


def scene_encircle(base):
    s = Script()
    s.at = (500, 560)
    s.write(handwriting("text", 420, RULE(10) - 4, 60, seed=9), speed=2400, pause=0.005)
    s.glide((560, 330))
    s.wait(0.4)
    s.mark("rec")
    s.wait(0.3)
    s.camera(500, RULE(10) - 60, 1.15)
    s.stroke(loop_around(500, RULE(10) - 31, 120, 64), speed=760, hold=0.9)
    s.wait(1.2)
    # Ready to move: drag it a little.
    grab = (500, RULE(10) - 30)
    s.glide(grab)
    s.wait(0.12)
    s.camera(530, RULE(10) - 55, 1.15)
    s.drag((580, RULE(10) - 6), dur=0.8)
    s.glide((760, 560))
    s.wait(1.6)
    s.mark("end")
    return s, (510, RULE(10) - 70, 1.0)


def scene_scratch(base):
    s = Script()
    s.at = (520, 400)
    s.write(handwriting("next", 400, RULE(8) - 4, 58, seed=13), speed=2400, pause=0.005)
    s.write(handwriting("text", 640, RULE(8) - 4, 58, seed=21), speed=2400, pause=0.005)
    s.glide((520, 420))
    s.mark("rec")
    s.wait(0.4)
    s.camera(510, RULE(8) - 26, 1.18)
    s.wait(0.4)
    s.stroke(scribble(400, 560, RULE(8) - 26, 40), speed=1700)
    s.glide((600, 440))
    s.camera(550, RULE(8) - 26, 1.08, at=s.t + 0.4)
    s.wait(2.0)
    s.mark("end")
    return s, (550, RULE(8) - 26, 1.05)


def scene_snap(base):
    s = Script()
    s.at = (420, 450)
    # The page paints before the clip starts.
    s.wait(1.2)
    s.mark("rec")
    s.wait(0.3)
    s.camera(505, 335, 1.15)
    s.wait(0.2)
    s.stroke(rough_circle(470, 330, 62), speed=620, hold=0.9)
    s.camera(560, 340, 1.12)
    s.wait(0.3)
    s.stroke([(620 + i * 1.6 + math.sin(i / 5) * 3, 390 - i * 1.0 + math.sin(i / 3) * 2.5) for i in range(90)], speed=560, hold=0.9)
    s.glide((720, 470))
    s.wait(1.6)
    s.mark("end")
    return s, (540, 335, 1.0)


def scene_math(base):
    s = Script()
    s.at = (360, 430)
    # The page paints before the clip starts.
    s.wait(1.2)
    s.mark("rec")
    s.wait(0.3)
    s.camera(500, RULE(8) - 30, 1.18)
    s.write(handwriting("12+30=", 360, RULE(8) - 4, 56, seed=4))
    s.glide((620, 470))
    # The answer writes itself after the equals sign.
    s.camera(530, RULE(8) - 30, 1.2, at=s.t + 0.6)
    s.wait(3.6)
    s.mark("end")
    return s, (510, RULE(8) - 30, 1.05)


def scene_convert(base):
    s = Script()
    s.at = (520, 520)
    s.write(handwriting("text", 420, RULE(7) - 4, 62, seed=9), speed=2400, pause=0.005)
    s.key("l")
    s.glide((520, 230))
    s.mark("rec")
    s.wait(0.3)
    s.camera(500, RULE(7) + 10, 1.12)
    s.stroke(loop_around(502, RULE(7) - 34, 122, 64, seed=7), speed=820)
    s.wait(0.5)
    button = centre(locate(s, base, s.t, ["recognize-text"], realtime=False)["recognize-text"])
    s.click(button)
    s.glide((button[0] + 60, button[1] + 90))
    s.wait(3.0)
    replace = centre(locate(s, base, s.t, ["submit-modal"])["submit-modal"])
    s.camera(500, RULE(7) + 60, 1.1, at=s.t - 1.0)
    s.wait(0.5)
    s.click(replace)
    s.camera(500, RULE(7) + 10, 1.15)
    s.glide((replace[0] + 120, replace[1] + 120))
    s.wait(1.8)
    s.mark("end")
    return s, (500, RULE(7) + 30, 1.0)


def scene_solve(base):
    s = Script()
    s.at = (420, 520)
    s.write(handwriting("2x+3=7", 250, RULE(8) - 4, 56, seed=4), speed=2400, pause=0.005)
    s.key("l")
    s.glide((420, 240))
    s.mark("rec")
    s.wait(0.3)
    s.camera(400, RULE(8) - 40, 1.12)
    s.stroke(loop_around(388, RULE(8) - 30, 168, 60, seed=4), speed=900)
    s.wait(0.5)
    button = centre(locate(s, base, s.t, ["solve-selection"], realtime=False)["solve-selection"])
    s.click(button)
    s.glide((button[0] + 220, button[1] + 160))
    # The solver opens on the right: the camera widens to show the problem
    # beside its answer, then leans towards the steps.
    s.camera(560, 345, 0.74, at=s.t + 0.2)
    s.wait(2.6)
    s.camera(700, 345, 0.85)
    s.wait(3.0)
    s.mark("end")
    return s, (430, 340, 1.0)


def scene_search(base):
    s = Script()
    s.at = (520, 560)
    # "next" alone is made searchable, so only it lights up.
    s.write(handwriting("next", 478, RULE(9) - 4, 58, seed=13), speed=2400, pause=0.005)
    s.ev("click", key="more")
    s.wait(0.5)
    s.ev("click", key="more-section-math")
    s.wait(0.5)
    s.ev("click", key="index-page-handwriting")
    # Indexing opens a review of what it read; confirm it.
    s.wait(9.0)
    s.ev("click", key="submit-modal")
    s.wait(1.5)
    s.ev("key", name="Escape")
    s.wait(0.5)
    s.write(handwriting("text", 300, RULE(6) - 4, 54, seed=9), speed=2400, pause=0.005)
    s.write(handwriting("neat", 640, RULE(12) - 4, 54, seed=11), speed=2400, pause=0.005)
    s.glide((760, 300))
    s.mark("rec")
    s.wait(0.3)
    search = centre(locate(s, base, s.t, ["search"])["search"], 0.2)
    s.camera(640, 250, 1.08)
    s.click(search)
    s.wait(0.3)
    s.glide((search[0] - 140, search[1] + 150))
    s.type("next")
    # The handwritten match lights up on the page.
    s.camera(580, 300, 1.12, at=s.t + 0.2)
    s.wait(3.2)
    s.mark("end")
    return s, (580, 270, 1.0)


SCENES = {
    "write": scene_write,
    "encircle": scene_encircle,
    "scratch": scene_scratch,
    "snap": scene_snap,
    "math": scene_math,
    "convert": scene_convert,
    "solve": scene_solve,
    "search": scene_search,
}

# ------------------------------------------------------------ recording


def base_library(theme):
    """A library with one blank ruled document open and the tour seen, in
    `theme` (light or dark)."""
    base = WORK / f"base-{theme}"
    if (base / "notes.sqlite3").exists():
        return base
    base.mkdir(parents=True, exist_ok=True)
    script = WORK / "base.json"
    script.write_text(json.dumps([
        {"t": 0.5, "type": "click", "key": "tour-skip"},
        {"t": 1.0, "type": "click", "key": "library-new"},
        {"t": 1.6, "type": "click", "key": "submit-modal"},
        {"t": 2.5, "type": "mark", "name": "end"},
    ]))
    subprocess.run([str(BIN), "--data-dir", str(base), "--script", str(script), "--snap", str(WORK / f"base-{theme}.png"),
                    "--size", f"{SIZE[0]}x{SIZE[1]}", "--scale", "1", "--at", "3"], check=True,
                   stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    (base / "session.lock").unlink(missing_ok=True)
    db = sqlite3.connect(base / "notes.sqlite3")
    prefs = json.loads(db.execute("select data from settings where key='preferences'").fetchone()[0])
    prefs.update(dark=theme == "dark", follow_system_theme=False, tour_version=1)
    db.execute("update settings set data=? where key='preferences'", (json.dumps(prefs),))
    db.commit()
    return base


def inside(view):
    """`view` moved to stay inside the window."""
    x, y, zoom = view
    w, h = VIEW[0] / zoom, VIEW[1] / zoom
    return (min(max(x, w / 2), SIZE[0] - w / 2), min(max(y, h / 2), SIZE[1] - h / 2), zoom)


def camera_path(script, first, start, count):
    """The camera for each frame. It chases where the cues aim on a critically
    damped spring, so it eases off from rest however sharply a cue sets off and
    keeps its speed smooth when one cue takes over from another; the cues'
    ease-out-expo still sets the long, soft settle."""
    omega, dt = CAMERA_SPRING, 1 / FPS
    view = inside(script.view_at(start, first))
    speed = (0.0, 0.0, 0.0)
    path = []
    for i in range(count):
        aim = inside(script.view_at(start + i / FPS, first))
        # The exact step of a critically damped spring toward `aim`.
        decay = math.exp(-omega * dt)
        moved, sped = [], []
        for p, v, a in zip(view, speed, aim):
            off = p - a
            c = v + omega * off
            moved.append(a + (off + c * dt) * decay)
            sped.append((v - omega * c * dt) * decay)
        view, speed = inside(tuple(moved)), tuple(sped)
        path.append(view)
    return path


def fade(image, i, count, paper):
    """The clip rises out of the stage's paper and sinks back into it at the
    end, so its loop never cuts."""
    k = min(1.0, i / (FADE_IN * FPS), (count - 1 - i) / (FADE_OUT * FPS))
    return image if k >= 1 else Image.blend(Image.new("RGB", image.size, paper), image, smooth(max(k, 0.0)))


def record(name, theme, reframe=False):
    base = base_library(theme)
    script, first = SCENES[name](base)
    work = WORK / f"{name}-{theme}"
    frames = work / "frames"
    if not (reframe and frames.is_dir()):
        shutil.rmtree(work, ignore_errors=True)
        work.mkdir(parents=True)
        data = work / "data"
        shutil.copytree(base, data)
        (work / "script.json").write_text(json.dumps(script.events))
        subprocess.run([str(BIN), "--data-dir", str(data), "--script", str(work / "script.json"), "--realtime",
                        "--size", f"{SIZE[0]}x{SIZE[1]}", "--scale", str(SCALE), "--fps", str(FPS),
                        "--start", "mark:rec", "--duration", "auto", "--out", str(frames)], check=True,
                       stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    start = next(e["t"] for e in script.events if e.get("name") == "rec")
    done = work / "done"
    shutil.rmtree(done, ignore_errors=True)
    done.mkdir()
    shots = sorted(frames.glob("frame_*.png"))
    paper = None
    for i, (shot, (x, y, zoom)) in enumerate(zip(shots, camera_path(script, first, start, len(shots)))):
        w, h = VIEW[0] / zoom, VIEW[1] / zoom
        box = tuple(v * SCALE for v in (x - w / 2, y - h / 2, x + w / 2, y + h / 2))
        image = Image.open(shot).convert("RGB").resize(OUT_PX, Image.LANCZOS, box=box)
        # The paper's colour: the commonest in the first frame.
        paper = paper or max(image.resize((96, 60)).getcolors(96 * 60))[1]
        fade(image, i, len(shots), paper).save(done / f"f{i:04d}.png")
    OUT.mkdir(parents=True, exist_ok=True)
    out = OUT / f"{name}-{theme}.webp"
    subprocess.run(["ffmpeg", "-y", "-loglevel", "error", "-framerate", str(FPS), "-i", str(done / "f%04d.png"),
                    "-c:v", "libwebp_anim", "-lossless", "0", "-quality", "88", "-compression_level", "6",
                    "-preset", "drawing", "-loop", "0", str(out)], check=True)
    print(name, theme, len(shots), "frames", out.stat().st_size // 1024, "KiB")


def main():
    WORK.mkdir(parents=True, exist_ok=True)
    args = sys.argv[1:]
    reframe = "--reframe" in args
    names = [a for a in args if a != "--reframe"] or list(SCENES)
    for theme in ("dark", "light"):
        for name in names:
            record(name, theme, reframe)


if __name__ == "__main__":
    main()
