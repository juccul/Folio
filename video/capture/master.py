#!/usr/bin/env python3
"""Two-pass EBU R128 mastering for the rendered cuts, plus the silent hero loop and poster."""
import json
import re
import subprocess
import sys
from pathlib import Path

OUT = Path(__file__).resolve().parents[2] / "artifacts/launch-out"  # git-ignored


def measure(src, i, tp):
    r = subprocess.run(["ffmpeg", "-hide_banner", "-nostats", "-i", str(src), "-af",
                        f"loudnorm=I={i}:TP={tp}:LRA=11:print_format=json", "-f", "null", "-"],
                       capture_output=True, text=True)
    return json.loads(re.search(r"\{[\s\S]*?\"input_i\"[\s\S]*?\}", r.stderr).group(0))


def master(src, dst, i=-14, tp=-1.0):
    m = measure(src, i, tp)
    af = (f"loudnorm=I={i}:TP={tp}:LRA=11:measured_I={m['input_i']}:measured_TP={m['input_tp']}:"
          f"measured_LRA={m['input_lra']}:measured_thresh={m['input_thresh']}:offset={m['target_offset']}:linear=true,"
          f"alimiter=limit={10 ** ((tp - 0.6) / 20):.4f}:attack=1:release=60:level=false,aresample=192000,aresample=48000")
    subprocess.run(["ffmpeg", "-y", "-loglevel", "error", "-i", str(src), "-c:v", "libx264", "-crf", "17", "-preset", "slow", "-pix_fmt", "yuv420p", "-color_range", "tv", "-af", af,
                    "-ar", "48000", "-c:a", "aac", "-b:a", "192k", "-movflags", "+faststart", str(dst)], check=True)
    print("mastered", dst.name, "from", m["input_i"], "LUFS")


def hero(src):
    dst = OUT / "folio-hero-loop.mp4"
    for crf in (26, 28, 30, 32):
        subprocess.run(["ffmpeg", "-y", "-loglevel", "error", "-i", str(src), "-an", "-vf", "scale=1280:-2:flags=lanczos",
                        "-c:v", "libx264", "-profile:v", "high", "-level", "4.1", "-pix_fmt", "yuv420p", "-crf", str(crf),
                        "-preset", "slow", "-movflags", "+faststart", str(dst)], check=True)
        if dst.stat().st_size < 1.9 * 1024 * 1024:
            break
    print("hero", dst.stat().st_size // 1024, "KiB at crf", crf)
    poster = OUT / "folio-hero-poster.jpg"
    for q in (5, 8, 12, 16):
        subprocess.run(["ffmpeg", "-y", "-loglevel", "error", "-ss", "3.2", "-i", str(src), "-vframes", "1",
                        "-vf", "scale=1280:-2", "-q:v", str(q), str(poster)], check=True)
        if poster.stat().st_size < 48 * 1024:
            break
    print("poster", poster.stat().st_size // 1024, "KiB")


if __name__ == "__main__":
    for name in sys.argv[1:]:
        if name == "hero":
            hero(OUT / "raw-hero.mp4")
            continue
        stem = ("folio-" + name) if name.startswith(("trailer-", "song-")) else f"folio-launch-{name}"
        master(OUT / f"raw-{name}.mp4", OUT / f"{stem}.mp4")
        if name.endswith("16x9") and "teaser" not in name:
            master(OUT / f"raw-{name}.mp4", OUT / f"{stem}-linkedin.mp4", -16, -1.5)
