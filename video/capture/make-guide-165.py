#!/usr/bin/env python3
"""Guide track for the song edit: a drum & bass sketch at the song's tempo and key
(165 BPM, A minor), arranged on src/config/song-edit.json (intro, build, one-beat gap,
drop, half-time, end). It stands in until the licensed song is dropped in, and is
generated from scratch, so there is nothing to clear. Also writes the edit's sparse foley."""
import importlib.util
import json
from pathlib import Path

import numpy as np

HERE = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location("au", HERE / "make-audio.py")
au = importlib.util.module_from_spec(spec)
spec.loader.exec_module(au)
SR, hz, filt, place, save = au.SR, au.hz, au.filt, au.place, au.save
E = json.loads((HERE.parent / "src/config/song-edit.json").read_text())
rng = np.random.default_rng(165)
BPM = E["bpm"]
BEAT = 60 / BPM
BAR = BEAT * 4
S16 = BEAT / 4


def rhodes(m, dur, vel=1.0):
    n = int(dur * SR)
    t = np.arange(n) / SR
    f = hz(m)
    mod = np.sin(2 * np.pi * f * t) * 1.6 * np.exp(-t * 3.5)
    x = np.sin(2 * np.pi * f * t + mod) * np.exp(-t * 1.4)
    x += 0.25 * np.sin(2 * np.pi * f * 4.0 * t) * np.exp(-t * 9)  # tine
    trem = 1 + 0.12 * np.sin(2 * np.pi * 4.5 * t)
    return x * trem * np.minimum(1, t / 0.003) * vel


def chord(notes, dur, vel=1.0):
    return sum(rhodes(m, dur, vel) for m in notes) / len(notes)


def snare():
    n = int(0.25 * SR)
    t = np.arange(n) / SR
    body = np.sin(2 * np.pi * 190 * t) * np.exp(-t * 30)
    noise = filt(rng.uniform(-1, 1, n), "bandpass", [1500, 9000]) * np.exp(-t * 22)
    return np.tanh((body * 0.8 + noise) * 1.8) * 0.8


def reese(m, dur):
    n = int(dur * SR)
    t = np.arange(n) / SR
    f = hz(m)
    x = sum(2 * ((f * d * t + rng.uniform()) % 1) - 1 for d in (0.993, 1.0, 1.007))
    lfo = 450 + 250 * np.sin(2 * np.pi * 0.7 * t)
    x = filt(x, "lowpass", 650) * (0.8 + 0.2 * np.sin(2 * np.pi * 0.35 * t))
    sub = np.sin(2 * np.pi * f / 2 * t)
    env = np.minimum(1, t / 0.01) * np.minimum(1, (dur - t) / 0.03)
    return (np.tanh(x * 1.2) * 0.45 + sub * 0.7) * env


def pen():
    """A nib drawn across glass: soft, papery, moving."""
    n = int(1.3 * SR)
    t = np.arange(n) / SR
    noise = rng.uniform(-1, 1, n)
    x = filt(noise, "bandpass", [2500, 7000]) * (0.4 + 0.6 * np.abs(np.sin(2 * np.pi * 3.1 * t)))
    return x * np.minimum(1, t / 0.05) * np.minimum(1, (1.3 - t) / 0.3) * 0.35


PROG = [([57, 60, 64, 67, 71], 45), ([53, 57, 60, 64, 67], 41), ([50, 53, 57, 60, 64], 38), ([52, 55, 59, 62, 64], 40)]  # Am9 Fmaj9 Dm9 Em7


def section(bar):
    for s in E["sections"]:
        if s["from"] <= bar < s["to"]:
            return s["name"]
    return "end"


def guide():
    nbars = max(s["to"] for s in E["sections"])
    total = nbars * BAR
    n = int((total + 3) * SR)
    drums = np.zeros((n, 2))
    music = np.zeros((n, 2))
    duck = np.ones(n)
    K, SN, H, HO = au.kick(), snare(), au.hat(), au.hat(True)
    drop = E["dropBar"]
    for b in range(nbars):
        t0 = b * BAR
        sec = section(b)
        notes, root = PROG[b % 4]
        gap = b == drop - 1  # the last beat before the drop is silent
        cut = t0 + 3 * BEAT if gap else t0 + BAR
        # Chords: every bar, voiced softer in the drop.
        c = chord(notes, BAR * 1.1, 0.9 if sec in ("intro", "half", "end") else 0.6)
        if gap:
            c = c[: int((cut - t0) * SR)]
        place(music, c, t0, 0.55, 0.45)
        if sec == "intro":
            for i in range(16):
                place(drums, H * 0.5, t0 + i * S16, 0.12 + 0.08 * (i % 2 == 0), 0.6)
        if sec in ("build", "drop"):
            bd = np.zeros((int(BAR * SR) + 4000, 2))
            for i in range(16):
                at = i * S16
                if gap and at >= 3 * BEAT:
                    continue
                if i in (0, 10):
                    place(bd, K, at, 1.0)
                if i in (4, 12):
                    place(bd, SN, at, 0.9, 0.5)
                if i in (7, 15) and sec == "drop":
                    place(bd, SN, at, 0.22, 0.55)
                place(bd, HO if i % 4 == 2 else H, at, 0.35 if i % 2 == 0 else 0.18, 0.62)
            if sec == "build":
                # Filter opens across the build.
                k = (b - 8) / 8
                cutoff = 500 + 7000 * k ** 2
                bd[:, 0] = filt(bd[:, 0], "lowpass", cutoff)
                bd[:, 1] = filt(bd[:, 1], "lowpass", cutoff)
                bd *= 0.6 + 0.4 * k
            if sec == "drop" and b == drop + 15:
                # Fill under the four-word burst.
                for i in range(8, 16):
                    place(bd, SN, i * S16, 0.35 + 0.05 * i, 0.5)
            s0 = int(t0 * SR)
            e0 = min(n, s0 + len(bd))
            drums[s0:e0] += bd[: e0 - s0]
            if sec == "drop":
                for i in (0, 10):
                    s1 = int((t0 + i * S16) * SR)
                    e1 = min(n, s1 + int(0.18 * SR))
                    duck[s1:e1] = np.minimum(duck[s1:e1], 0.35 + 0.65 * np.arange(e1 - s1) / (0.18 * SR))
                # Reese bass: root on 1, a push on the "and" of 2, octave on 4.
                for at, m, d in ((0, root, 1.5), (6, root, 0.5), (8, root + 3, 1.0), (12, root + 7, 1.0)):
                    place(music, reese(m, d * BEAT), t0 + at * S16, 0.75)
        if sec == "half":
            place(drums, K, t0, 1.0)
            place(drums, SN, t0 + 2 * BEAT, 0.95, 0.5)
            for i in range(8):
                place(drums, H, t0 + i * BEAT / 2, 0.25, 0.6)
            place(music, reese(root, BAR * 0.95), t0, 0.6)
        if b == drop - 2:
            place(drums, au.riser(2 * BAR - BEAT), t0, 0.7)
    # Accents and foley (kept sparse).
    place(drums, au.crash(), drop * BAR, 1.0, 0.5)
    place(drums, au.impact(), drop * BAR, 0.8)
    half = next(s["from"] for s in E["sections"] if s["name"] == "half")
    end = next(s["from"] for s in E["sections"] if s["name"] == "end")
    place(drums, au.impact(), half * BAR, 0.7)
    place(drums, au.crash(), end * BAR, 0.9, 0.5)
    place(music, chord(PROG[0][0], 4.5, 1.0), end * BAR, 0.8)
    place(music, reese(PROG[0][1], 2.5), end * BAR, 0.5)
    music[:, 0] *= duck
    music[:, 1] *= duck
    mix = drums + music
    for d, g in ((0.109, 0.18), (0.2182, 0.12), (0.436, 0.07)):
        o = int(d * SR)
        mix[o:, 0] += mix[:-o, 1] * g
        mix[o:, 1] += mix[:-o, 0] * g
    mix = mix[: int((total + 0.3) * SR)]
    fade = np.minimum(1, (len(mix) - np.arange(len(mix))) / (1.5 * SR))[:, None]
    mix *= fade
    return np.tanh(mix / np.max(np.abs(mix)) * 1.3) * 0.6


if __name__ == "__main__":
    save("guide-165.wav", guide())
    save("pen.wav", pen())
    print("guide track and foley written")
