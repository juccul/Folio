#!/usr/bin/env python3
"""Score for the product-trailer cut, arranged on src/config/trailer.json and synthesised
from scratch (no third-party licence to clear)."""
import importlib.util
import json
from pathlib import Path

import numpy as np

HERE = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location("au", HERE / "make-audio.py")
au = importlib.util.module_from_spec(spec)
spec.loader.exec_module(au)
SR, hz, filt, place = au.SR, au.hz, au.filt, au.place
TR = json.loads((HERE.parent / "src/config/trailer.json").read_text())
rng = np.random.default_rng(5)

LEVEL = {"macro": "intro", "line": "intro", "title": "hit", "hero": "light", "stuck": "break",
         "solve": "full", "stats": "half", "end": "end"}


def drone(freqs, dur):
    n = int(dur * SR)
    t = np.arange(n) / SR
    x = sum(np.sin(2 * np.pi * f * t + rng.uniform(0, 6)) + 0.35 * np.sin(2 * np.pi * f * 2.003 * t) for f in freqs)
    lfo = 0.75 + 0.25 * np.sin(2 * np.pi * 0.25 * t)
    return x / len(freqs) * lfo * np.minimum(1, t / 0.6) * np.minimum(1, (dur - t) / 0.4) * 0.5


def stab(chord, dur=2.5):
    n = int(dur * SR)
    t = np.arange(n) / SR
    x = np.zeros(n)
    for m in chord:
        for det in (-0.08, 0, 0.09):
            f = hz(m) * 2 ** (det / 12)
            x += 2 * ((f * t + rng.uniform()) % 1) - 1
    x = filt(x, "lowpass", 3000) * np.exp(-t * 1.6)
    return x / (len(chord) * 3) * 1.4


def bell(m, dur=1.6):
    n = int(dur * SR)
    t = np.arange(n) / SR
    f = hz(m)
    return (np.sin(2 * np.pi * f * t) + 0.3 * np.sin(2 * np.pi * f * 2.76 * t) * np.exp(-t * 6)) * np.exp(-t * 2.5) * 0.5


def score(cut):
    fps, bpm = TR["fps"], TR["bpm"]
    beat = 60 / bpm
    scenes = TR["cuts"][cut]
    starts, t = [], 0.0
    for s in scenes:
        starts.append(t)
        t += s["len"] / fps
    total = t
    n = int((total + 3) * SR)
    drums = np.zeros((n, 2))
    music = np.zeros((n, 2))
    duck = np.ones(n)
    K, C, H, HO = au.kick(), au.clap(), au.hat(), au.hat(True)
    prog = [([62, 66, 69, 73], 50), ([59, 62, 66, 69], 47), ([55, 59, 62, 66], 43), ([57, 61, 64, 69], 45)]
    level_at = []
    for s, st in zip(scenes, starts):
        lv = LEVEL.get(s["id"], "full")
        level_at.append((st, st + s["len"] / fps, lv, s))
    end_start = next(st for st, _, lv, _ in level_at if lv == "end")
    # Drone under the opening, swelling into the title.
    title_start = next((st for st, _, lv, _ in level_at if lv == "hit"), None)
    intro_end = title_start if title_start is not None else starts[1]
    place(music, drone([hz(38), hz(45), hz(50)], intro_end + 0.3), 0, 0.9)
    for i in range(int(intro_end / (beat * 2))):
        place(music, bell([74, 81, 78, 76][i % 4]), i * beat * 2, 0.22, 0.35 if i % 2 else 0.65)
        place(drums, K * 0.45, i * beat * 2, 0.5)
    place(drums, au.riser(min(2.0, intro_end)), intro_end - min(2.0, intro_end), 0.7)
    beat_i = 0
    tb = 0.0
    while tb < total:
        lv = next((l for a, b, l, _ in level_at if a <= tb < b), "end")
        bar_pos = beat_i % 4
        chord, root = prog[(beat_i // 4) % 4]
        if lv in ("light", "full", "half"):
            if lv != "half" or bar_pos == 0:
                place(drums, K, tb, 0.95)
                s0 = int(tb * SR)
                e0 = min(n, s0 + int(0.22 * SR))
                duck[s0:e0] = np.minimum(duck[s0:e0], 0.3 + 0.7 * np.arange(e0 - s0) / (0.22 * SR))
            if lv == "full" and bar_pos in (1, 3):
                place(drums, C, tb, 0.55, 0.55)
            if lv == "half" and bar_pos == 2:
                place(drums, C, tb, 0.7, 0.5)
            for e8 in range(2):
                place(drums, HO if e8 else H, tb + e8 * beat / 2, (0.45 if e8 else 0.3) * (0.7 if lv == "light" else 1), 0.62)
                if lv == "full":
                    place(drums, H, tb + e8 * beat / 2 + beat / 4, 0.2, 0.4)
            place(music, au.bass(hz(root - 12), beat / 2 * 0.9), tb + beat / 2, 0.75)
            if lv != "half":
                for j in range(2):
                    note = chord[(beat_i * 2 + j) % 4] + (12 if (beat_i + j) % 3 == 0 else 0)
                    place(music, au.pluck(hz(note)), tb + j * beat / 2, 0.28 if lv == "full" else 0.2, 0.3 if j else 0.7)
            if bar_pos == 0:
                place(music, au.pad([hz(m - 12) for m in chord[:3]], beat * 4), tb, 0.2)
        elif lv in ("break", "hit") and bar_pos == 0:
            place(music, drone([hz(root - 12), hz(root - 5)], beat * 4), tb, 0.7)
        beat_i += 1
        tb += beat
    # Accents.
    for st, en, lv, s in level_at:
        if lv in ("hit", "end") or s["id"] == "solve":
            place(drums, au.impact(), st, 0.9)
            place(drums, au.crash(), st, 0.8, 0.5)
            place(music, stab([50, 57, 62, 66, 69] if lv != "end" else [50, 57, 62, 64, 69]), st, 0.7)
        if lv == "break":
            place(drums, au.riser(en - st), st, 0.8)
        if s["type"] in ("split", "device") and s["id"] != "solve":
            place(drums, au.whoosh(0.3), max(0, st - 0.3), 0.35, 0.5)
        if lv == "half":
            for i in range(3):
                place(drums, au.impact() * 0.7, st + (4 + i * 30) / fps, 0.6)
    place(music, au.chime(), end_start + 0.4, 0.5)
    place(music, drone([hz(50), hz(57), hz(62)], total - end_start + 1.0), end_start, 0.6)
    music[:, 0] *= duck
    music[:, 1] *= duck
    mix = drums + music
    for d, g in ((0.149, 0.2), (0.263, 0.14), (0.41, 0.08)):
        o = int(d * SR)
        mix[o:, 0] += mix[:-o, 1] * g
        mix[o:, 1] += mix[:-o, 0] * g
    mix = mix[: int((total) * SR)]
    fade = np.minimum(1, (len(mix) - np.arange(len(mix))) / (1.2 * SR))[:, None]
    mix *= fade
    return np.tanh(mix / np.max(np.abs(mix)) * 1.25) * 0.6


if __name__ == "__main__":
    for cut in ("flagship", "teaser"):
        au.save(f"trailer-{cut}.wav", score(cut))
    print("trailer audio written")
