#!/usr/bin/env python3
"""Synthesise the launch video's original audio from oscillators and noise (no third-party
licence to clear): an upbeat 120 BPM track arranged on src/config/timing.json, plus foley."""
import json
import wave
from pathlib import Path

import numpy as np
from scipy.signal import butter, lfilter, sosfilt

SR = 48000
HERE = Path(__file__).resolve().parents[1]
OUT = HERE / "public/audio"
T = json.loads((HERE / "src/config/timing.json").read_text())
rng = np.random.default_rng(11)


def save(name, x):
    x = np.asarray(x, float)
    st = np.stack([x, x], 1) if x.ndim == 1 else x
    st = np.clip(st, -1, 1)
    with wave.open(str(OUT / name), "wb") as w:
        w.setnchannels(2)
        w.setsampwidth(2)
        w.setframerate(SR)
        w.writeframes((st * 32767).astype("<i2").tobytes())


def filt(x, kind, f, order=2):
    sos = butter(order, f, btype=kind, fs=SR, output="sos")
    return sosfilt(sos, x)


def hz(n):
    return 440.0 * 2 ** ((n - 69) / 12)


def env(n, a, d):
    t = np.arange(n) / SR
    return np.minimum(1, t / max(a, 1e-4)) * np.exp(-t / d)


def kick():
    n = int(0.45 * SR)
    t = np.arange(n) / SR
    f = 48 + 110 * np.exp(-t * 30)
    ph = 2 * np.pi * np.cumsum(f) / SR
    body = np.sin(ph) * np.exp(-t * 7)
    click = filt(rng.uniform(-1, 1, n), "highpass", 2500) * np.exp(-t * 300) * 0.4
    return np.tanh((body + click) * 1.6) * 0.9


def clap():
    n = int(0.35 * SR)
    t = np.arange(n) / SR
    noise = filt(rng.uniform(-1, 1, n), "bandpass", [900, 3500])
    e = np.zeros(n)
    for off in (0, 0.011, 0.022):
        s = int(off * SR)
        e[s:] += np.exp(-(t[: n - s]) * (60 if off < 0.02 else 14))
    return noise * e * 0.7


def hat(open_=False):
    n = int((0.22 if open_ else 0.05) * SR)
    t = np.arange(n) / SR
    noise = filt(rng.uniform(-1, 1, n), "highpass", 7000)
    return noise * np.exp(-t * (14 if open_ else 90)) * 0.35


def pluck(freq, dur=0.32):
    n = int(dur * SR)
    t = np.arange(n) / SR
    mod = np.sin(2 * np.pi * freq * 2 * t) * 1.2 * np.exp(-t * 18)
    x = np.sin(2 * np.pi * freq * t + mod) * np.exp(-t * 9)
    x += 0.3 * np.sin(2 * np.pi * freq * 2 * t) * np.exp(-t * 16)
    return x * np.minimum(1, t / 0.002)


def bass(freq, dur):
    n = int(dur * SR)
    t = np.arange(n) / SR
    x = sum(np.sin(2 * np.pi * freq * k * t) / k for k in range(1, 7))
    x = filt(x, "lowpass", 900)
    return np.tanh(x * 1.5) * env(n, 0.004, dur * 0.9) * 0.55


def pad(freqs, dur):
    n = int(dur * SR)
    t = np.arange(n) / SR
    x = np.zeros(n)
    for f in freqs:
        for det in (-0.1, 0.0, 0.11):
            ff = f * 2 ** (det / 12)
            x += 2 * ((ff * t + rng.uniform()) % 1) - 1  # saw
    x = filt(x, "lowpass", 2200)
    return x / (len(freqs) * 3) * np.minimum(1, t / 0.05) * np.minimum(1, (dur - t) / 0.1)


def crash():
    n = int(2.2 * SR)
    t = np.arange(n) / SR
    return filt(rng.uniform(-1, 1, n), "highpass", 4000) * np.exp(-t * 2.2) * 0.4


def riser(dur):
    n = int(dur * SR)
    t = np.arange(n) / SR
    k = t / dur
    noise = rng.uniform(-1, 1, n)
    out = np.zeros(n)
    # Sweep a band upward in blocks.
    blk = 2048
    for s in range(0, n, blk):
        f = 400 + 9000 * (s / n) ** 2
        seg = noise[max(0, s - 512): s + blk]
        y = filt(seg, "bandpass", [f, min(f * 1.8, 20000)])
        out[s: s + blk] = y[-len(out[s: s + blk]):]
    return out * k ** 2 * 0.5


def place(buf, x, at, gain=1.0, pan=0.5):
    s = int(at * SR)
    if s >= len(buf):
        return
    e = min(len(buf), s + len(x))
    buf[s:e, 0] += x[: e - s] * gain * (1 - pan) * 2 * 0.71
    buf[s:e, 1] += x[: e - s] * gain * pan * 2 * 0.71


def track(cut):
    t = T[cut]
    fps, bpm = T["fps"], T["bpm"]
    beat = 60 / bpm
    bar = beat * 4
    total = (sum(t["slots"].values()) + t["bento"] + t["end"]) / fps
    bento = sum(t["slots"].values()) / fps if t["bento"] else None
    end = (sum(t["slots"].values()) + t["bento"]) / fps
    n = int((total + 2.5) * SR)
    drums = np.zeros((n, 2))
    music = np.zeros((n, 2))
    duck = np.ones(n)
    prog = [([62, 66, 69], 50), ([59, 62, 66], 47), ([55, 59, 62], 43), ([57, 61, 64], 45)]  # D Bm G A
    arp_shape = [0, 1, 2, 1, 2, 0, 2, 1]
    K, C, H, HO = kick(), clap(), hat(), hat(True)
    nbars = int(np.ceil(total / bar)) + 1
    for b in range(nbars):
        t0 = b * bar
        if t0 >= end + bar * 1.5:
            break
        chord, root = prog[b % 4]
        pre = bento is not None and bento - bar <= t0 < bento  # the bar before the bento: build
        outro = t0 >= end
        for q in range(4):
            tb = t0 + q * beat
            if tb >= total + 0.5:
                continue
            if not (pre and q >= 2) and not (outro and t0 >= end + bar):
                place(drums, K, tb, 0.95)
                s = int(tb * SR)
                e = min(n, s + int(0.22 * SR))
                duck[s:e] = np.minimum(duck[s:e], 0.35 + 0.65 * (np.arange(e - s) / (0.22 * SR)))
            if q in (1, 3) and (b > 0 or cut == "teaser") and not outro:
                place(drums, C, tb, 0.6, 0.55)
            for e8 in range(2):
                th = tb + e8 * beat / 2
                if not outro:
                    place(drums, HO if e8 == 1 else H, th, 0.5 if e8 else 0.35, 0.62)
                    place(drums, H, th + beat / 4, 0.22, 0.62)
            if not outro or t0 < end + bar:
                place(music, bass(hz(root - 12), beat / 2 * 0.9), tb + beat / 2, 0.8)
        if pre:
            for i in range(8):  # snare roll into the bento
                place(drums, C, t0 + bar / 2 + i * beat / 4, 0.25 + 0.06 * i, 0.5)
        # Plucked arpeggio in 8ths, panned left and right.
        for i in range(8):
            ta = t0 + i * beat / 2
            if ta >= total:
                continue
            note = chord[arp_shape[i]] + (12 if i in (3, 7) else 0)
            place(music, pluck(hz(note)), ta, 0.32, 0.3 if i % 2 else 0.7)
        place(music, pad([hz(m - 12) for m in chord], bar), t0, 0.22)
    if bento is not None:
        place(drums, riser(bar), bento - bar, 0.6)
        place(drums, crash(), bento, 1.0, 0.45)
    place(drums, crash(), end, 1.0, 0.55)
    music[:, 0] *= duck
    music[:, 1] *= duck
    mix = drums + music
    # Short stereo room.
    for d, g in ((0.137, 0.18), (0.229, 0.12)):
        o = int(d * SR)
        mix[o:, 0] += mix[:-o, 1] * g
        mix[o:, 1] += mix[:-o, 0] * g
    mix = mix[: int((total + 0.2) * SR)]
    fade = np.minimum(1, (len(mix) - np.arange(len(mix))) / (0.6 * SR))[:, None]
    mix *= fade
    return np.tanh(mix / np.max(np.abs(mix)) * 1.3) * 0.6


def whoosh(dur=0.3, big=False):
    n = int((dur + 0.25) * SR)
    t = np.arange(n) / SR
    noise = rng.uniform(-1, 1, n)
    k = np.clip(t / dur, 0, 1)
    e = np.where(t < dur, k ** 2.5, np.exp(-(t - dur) * 16))
    lo = filt(noise, "bandpass", [500, 3000])
    hi = filt(noise, "bandpass", [2500, 9000])
    x = (lo * (1 - k) + hi * k) * e
    if big:
        tt = np.maximum(0, t - dur)
        x += np.sin(2 * np.pi * 70 * tt) * np.exp(-tt * 12) * (t >= dur) * 0.8
    return x / np.max(np.abs(x)) * 0.6


def impact():
    n = int(1.4 * SR)
    t = np.arange(n) / SR
    f = 40 + 60 * np.exp(-t * 18)
    boom = np.sin(2 * np.pi * np.cumsum(f) / SR) * np.exp(-t * 3.5)
    x = np.tanh(boom * 2) + filt(rng.uniform(-1, 1, n), "lowpass", 1500) * np.exp(-t * 9) * 0.3
    return x / np.max(np.abs(x)) * 0.7


def chime():
    n = int(2.0 * SR)
    t = np.arange(n) / SR
    x = np.zeros(n)
    for f, d, a in ((hz(81), 0.0, 1.0), (hz(86), 0.09, 0.8), (hz(90), 0.18, 0.7)):
        s = int(d * SR)
        tt = t[: n - s]
        x[s:] += sum(np.sin(2 * np.pi * f * h * tt) * w for h, w in ((1, 1), (2.01, 0.2))) * np.exp(-tt * 3) * a
    return x * np.minimum(1, t / 0.003) / 2.5


if __name__ == "__main__":
    OUT.mkdir(parents=True, exist_ok=True)
    save("whoosh.wav", whoosh())
    save("swoosh-big.wav", whoosh(0.3, True))
    save("impact.wav", impact())
    save("chime.wav", chime())
    save("bed-flagship.wav", track("flagship"))
    save("bed-teaser.wav", track("teaser"))
    print("audio written to", OUT)
