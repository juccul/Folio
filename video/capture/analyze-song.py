#!/usr/bin/env python3
"""Lock the trailer to a real song: find its tempo, bar grid and drops, and write
src/config/song.json so the edit's drop lands on the song's drop.

    python3 capture/analyze-song.py public/audio/song.mp3 [--drop SECONDS] [--video-drop-bar 8]

The song file must be one you are licensed to use in the video."""
import argparse
import json
from pathlib import Path

import librosa
import numpy as np

HERE = Path(__file__).resolve().parents[1]


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("audio", type=Path)
    ap.add_argument("--drop", type=float, help="force the drop time in the song (seconds)")
    edit = json.loads((HERE / "src/config/song-edit.json").read_text())
    ap.add_argument("--video-drop-bar", type=int, default=edit["dropBar"], help="bar of the edit where the drop must land")
    ap.add_argument("--write", action="store_true", help="write src/config/song.json")
    ap.add_argument("--bpm-hint", type=float, default=165.0)
    ap.add_argument("--bpm", type=float, help="force the tempo (skip beat tracking)")
    args = ap.parse_args()
    y, sr = librosa.load(str(args.audio), sr=22050, mono=True)
    tempo, beats = librosa.beat.beat_track(y=y, sr=sr, start_bpm=args.bpm_hint, units="time", tightness=400)
    tempo = float(np.atleast_1d(tempo)[0])
    # Half/double-time ambiguity: drum & bass is often detected at ~82 BPM.
    while tempo < args.bpm_hint * 0.75:
        tempo *= 2
    while tempo > args.bpm_hint * 1.5:
        tempo /= 2
    if args.bpm:
        tempo = args.bpm
    elif abs(tempo - args.bpm_hint) / args.bpm_hint < 0.03:
        tempo = args.bpm_hint  # trust the published tempo; detection drifts by a percent or two
    beat = 60 / tempo
    bar = beat * 4
    # Low-end energy per bar finds where the bass and kick arrive (the drops).
    S = np.abs(librosa.stft(y, n_fft=2048, hop_length=512))
    freqs = librosa.fft_frequencies(sr=sr, n_fft=2048)
    low = S[freqs < 150].sum(axis=0)
    full = S.sum(axis=0)
    times = librosa.frames_to_time(np.arange(S.shape[1]), sr=sr, hop_length=512)
    # Phase of the bar grid: kicks land on downbeats, so try every 16th-note offset and keep
    # the one whose bar starts carry the most low-end onset energy.
    lowy = librosa.effects.preemphasis(y, coef=-0.97)  # tilt toward the low end
    onset = librosa.onset.onset_strength(y=lowy, sr=sr, hop_length=512, fmax=200, n_mels=32)
    best, phase = -1.0, 0.0
    for k in range(64):
        start = k * bar / 64
        idx = librosa.time_to_frames(np.arange(start, times[-1], bar), sr=sr, hop_length=512)
        idx = idx[idx < len(onset)]
        score = float(onset[idx].mean()) if len(idx) else 0.0
        if score > best:
            best, phase = score, start
    nbars = int((times[-1] - phase) / bar)
    bar_low = []
    for b in range(nbars):
        a, z = phase + b * bar, phase + (b + 1) * bar
        m = (times >= a) & (times < z)
        bar_low.append((low[m].mean() if m.any() else 0, full[m].mean() if m.any() else 0))
    lows = np.array([x[0] for x in bar_low])
    norm = lows / (lows.max() or 1)
    # A drop is the bar where the next 8 bars are much heavier than the 8 before, usually
    # right after a dip (the gap or breakdown before it).
    def score(b):
        before = norm[max(0, b - 8):b].mean()
        after = norm[b:b + 8].mean()
        dip = norm[max(0, b - 1)]
        return after - before + 0.5 * (after - dip)

    scored = sorted(((b, score(b)) for b in range(8, nbars - 4)), key=lambda x: -x[1])
    drop_times = [phase + b * bar for b, _ in scored[:4]]
    drop = args.drop if args.drop is not None else drop_times[0]
    start_at = max(0.0, drop - args.video_drop_bar * bar)
    out = {
        "src": f"audio/{args.audio.name}",
        "bpm": round(tempo, 3),
        "barPhase": round(phase, 3),
        "drop": round(drop, 3),
        "dropCandidates": [round(t, 2) for t in drop_times],
        "startAt": round(start_at, 3),
        "videoDropBar": args.video_drop_bar,
    }
    if args.write:
        (HERE / "src/config/song.json").write_text(json.dumps(out, indent=2) + "\n")
    print(json.dumps(out, indent=2))
    print("bar energy (low end), 16 bars per row, with the song time of each row:")
    for r in range(0, nbars, 16):
        row = "".join(" .:-=+*#%@"[min(9, int(v * 9.99))] for v in norm[r:r + 16])
        print(f"  bar {r:3d}  {phase + r * bar:7.2f}s  |{row}|")
    print("bar energy (low end), one char per bar:")
    print("".join(" .:-=+*#%@"[min(9, int(v * 9.99))] for v in norm))


if __name__ == "__main__":
    main()
