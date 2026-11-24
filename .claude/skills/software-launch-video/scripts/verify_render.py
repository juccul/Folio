#!/usr/bin/env python3
"""Verify rendered software launch video files against technical, loudness, and safe-zone gates.

Requires: ffprobe and ffmpeg in PATH. Optional: Pillow (PIL) for drawing 9:16 safe-zone overlays.

Usage:
    python3 scripts/verify_render.py output/launch-16x9.mp4 --target youtube --extract-frames qc/16x9
    python3 scripts/verify_render.py output/launch-9x16.mp4 --target vertical --extract-frames qc/9x16
    python3 scripts/verify_render.py output/hero-loop.mp4 --target hero
"""

import argparse
import json
import re
import shutil
import subprocess
import sys
from pathlib import Path

TARGETS = {
    "youtube": {
        "aspect": (16, 9),
        "lufs": -14.0,
        "lufs_tol": 1.5,
        "true_peak_max": -0.8,
        "max_mb": None,
        "require_silent": False,
    },
    "x": {
        "aspect": None,
        "lufs": -14.0,
        "lufs_tol": 1.5,
        "true_peak_max": -0.8,
        "max_mb": 512.0,
        "require_silent": False,
    },
    "linkedin": {
        "aspect": None,
        "lufs": -16.0,
        "lufs_tol": 1.5,
        "true_peak_max": -1.3,
        "max_mb": 200.0,
        "require_silent": False,
    },
    "vertical": {
        "aspect": (9, 16),
        "lufs": -14.0,
        "lufs_tol": 1.5,
        "true_peak_max": -0.8,
        "max_mb": 200.0,
        "require_silent": False,
    },
    "square": {
        "aspect": (1, 1),
        "lufs": -14.0,
        "lufs_tol": 1.5,
        "true_peak_max": -0.8,
        "max_mb": 200.0,
        "require_silent": False,
    },
    "hero": {
        "aspect": (16, 9),
        "lufs": None,
        "lufs_tol": None,
        "true_peak_max": None,
        "max_mb": 2.0,
        "require_silent": True,
    },
}


def run_cmd(cmd: list[str]) -> subprocess.CompletedProcess:
    return subprocess.run(cmd, capture_output=True, text=True)


def probe_video(path: Path) -> dict:
    res = run_cmd([
        "ffprobe", "-v", "error",
        "-show_format", "-show_streams",
        "-print_format", "json", str(path),
    ])
    if res.returncode != 0:
        raise RuntimeError(f"ffprobe failed: {res.stderr.strip()}")
    return json.loads(res.stdout)


def measure_loudness(path: Path, target_lufs: float, target_tp: float) -> dict | None:
    res = run_cmd([
        "ffmpeg", "-hide_banner", "-nostats", "-i", str(path),
        "-af", f"loudnorm=I={target_lufs}:TP={target_tp}:LRA=11:print_format=json",
        "-f", "null", "-",
    ])
    match = re.search(r'\{[\s\S]*?"input_i"[\s\S]*?\}', res.stderr)
    if not match:
        return None
    data = json.loads(match.group(0))
    return {
        "input_i": float(data["input_i"]),
        "input_tp": float(data["input_tp"]),
        "input_lra": float(data["input_lra"]),
    }


def draw_safe_zone_overlay(image_path: Path, out_path: Path) -> bool:
    try:
        from PIL import Image, ImageDraw
    except ImportError:
        return False

    img = Image.open(image_path).convert("RGBA")
    w, h = img.size
    # Scale universal 1080x1920 cross-app safe-zone margins proportionally
    top = int(round(h * (250 / 1920)))
    bottom = int(round(h * (560 / 1920)))
    left = int(round(w * (60 / 1080)))
    right = int(round(w * (180 / 1080)))

    overlay = Image.new("RGBA", (w, h), (0, 0, 0, 0))
    draw = ImageDraw.Draw(overlay)
    mask_color = (220, 38, 38, 95)

    # Top, bottom, left, right platform UI obstruction zones
    draw.rectangle([0, 0, w, top], fill=mask_color)
    draw.rectangle([0, h - bottom, w, h], fill=mask_color)
    draw.rectangle([0, top, left, h - bottom], fill=mask_color)
    draw.rectangle([w - right, top, w, h - bottom], fill=mask_color)
    # Green border around safe core
    draw.rectangle([left, top, w - right, h - bottom], outline=(34, 197, 94, 230), width=4)

    combined = Image.alpha_composite(img, overlay).convert("RGB")
    combined.save(out_path)
    return True


def extract_qc_frames(path: Path, duration: float, out_dir: Path, is_vertical: bool) -> list[str]:
    out_dir.mkdir(parents=True, exist_ok=True)
    timestamps = [
        ("01_hook_0p5s", min(0.5, max(0.0, duration - 0.1))),
        ("02_hook_1p5s", min(1.5, max(0.0, duration - 0.1))),
        ("03_lock_3p0s", min(3.0, max(0.0, duration - 0.1))),
        ("04_body_25pct", duration * 0.25),
        ("05_body_50pct", duration * 0.50),
        ("06_body_75pct", duration * 0.75),
        ("07_tail_end", max(0.0, duration - 1.0)),
    ]
    extracted = []
    for label, ts in timestamps:
        frame_path = out_dir / f"{label}.png"
        res = run_cmd([
            "ffmpeg", "-y", "-hide_banner", "-loglevel", "error",
            "-ss", f"{ts:.3f}", "-i", str(path), "-vframes", "1", str(frame_path),
        ])
        if res.returncode == 0 and frame_path.exists():
            extracted.append(str(frame_path))
            if is_vertical:
                overlay_path = out_dir / f"{label}_safezone.png"
                if draw_safe_zone_overlay(frame_path, overlay_path):
                    extracted.append(str(overlay_path))
    return extracted


def parse_fps(rate_str: str) -> float:
    if "/" in rate_str:
        num, den = rate_str.split("/")
        return round(float(num) / float(den), 2) if float(den) else 0.0
    return round(float(rate_str), 2)


def main() -> int:
    parser = argparse.ArgumentParser(description="Verify a rendered software launch video against quality gates.")
    parser.add_argument("video", help="Path to rendered video file.")
    parser.add_argument("--target", choices=TARGETS.keys(), default="youtube", help="Distribution target preset.")
    parser.add_argument("--extract-frames", help="Optional directory to extract QC inspection frames.")
    parser.add_argument("--json", action="store_true", help="Output machine-readable JSON.")
    args = parser.parse_args()

    if not shutil.which("ffprobe") or not shutil.which("ffmpeg"):
        print("Error: ffmpeg and ffprobe must be installed and in PATH.", file=sys.stderr)
        return 2

    video_path = Path(args.video)
    if not video_path.exists():
        print(f"Error: File not found: {video_path}", file=sys.stderr)
        return 2

    cfg = TARGETS[args.target]
    probe = probe_video(video_path)
    streams = probe.get("streams", [])
    v_stream = next((s for s in streams if s.get("codec_type") == "video"), None)
    a_stream = next((s for s in streams if s.get("codec_type") == "audio"), None)

    if not v_stream:
        print("Error: No video stream found.", file=sys.stderr)
        return 2

    width = int(v_stream["width"])
    height = int(v_stream["height"])
    codec = v_stream.get("codec_name", "")
    pix_fmt = v_stream.get("pix_fmt", "")
    fps = parse_fps(v_stream.get("r_frame_rate", "0/1"))
    duration = float(probe.get("format", {}).get("duration", 0.0))
    size_mb = round(video_path.stat().st_size / (1024 * 1024), 2)

    errors = []
    warnings = []

    if codec != "h264":
        warnings.append(f"Video codec is '{codec}'; prefer 'h264' (yuv420p) for universal social and browser compatibility.")
    if pix_fmt and pix_fmt != "yuv420p":
        warnings.append(f"Pixel format is '{pix_fmt}'; prefer 'yuv420p' so Safari and mobile players render cleanly.")

    if cfg["aspect"]:
        exp_w, exp_h = cfg["aspect"]
        actual_ratio = width / height
        expected_ratio = exp_w / exp_h
        if abs(actual_ratio - expected_ratio) > 0.03:
            errors.append(f"Aspect ratio mismatch for {args.target}: got {width}x{height}, expected {exp_w}:{exp_h}.")

    if cfg["max_mb"] is not None and size_mb > cfg["max_mb"]:
        errors.append(f"File size {size_mb} MB exceeds {args.target} limit of {cfg['max_mb']} MB.")

    if args.target == "hero" and duration > 35.0:
        warnings.append(f"Hero loop is {duration:.1f}s; keep silent homepage hero loops to 15-30s.")

    loudness = None
    if cfg["require_silent"]:
        if a_stream is not None:
            errors.append("Homepage hero loop contains an audio stream. Strip audio (-an) so playback is strictly silent.")
    else:
        if a_stream is None:
            warnings.append("No audio stream detected. Confirm this cut is intentionally silent.")
        else:
            sample_rate = int(a_stream.get("sample_rate", 0))
            if sample_rate != 48000:
                warnings.append(f"Audio sample rate is {sample_rate} Hz; prefer 48000 Hz AAC.")
            loudness = measure_loudness(video_path, cfg["lufs"], -1.0 if cfg["lufs"] == -14.0 else -1.5)
            if loudness:
                if abs(loudness["input_i"] - cfg["lufs"]) > cfg["lufs_tol"]:
                    errors.append(
                        f"Integrated loudness {loudness['input_i']:.1f} LUFS is outside "
                        f"{cfg['lufs']:.1f} +/- {cfg['lufs_tol']:.1f} LUFS for {args.target}."
                    )
                if loudness["input_tp"] > cfg["true_peak_max"]:
                    errors.append(
                        f"True peak {loudness['input_tp']:.1f} dBTP exceeds ceiling {cfg['true_peak_max']:.1f} dBTP."
                    )

    extracted_frames = []
    if args.extract_frames:
        is_vertical = height > width
        extracted_frames = extract_qc_frames(video_path, duration, Path(args.extract_frames), is_vertical)

    report = {
        "status": "PASS" if not errors else "FAIL",
        "file": str(video_path),
        "target": args.target,
        "width": width,
        "height": height,
        "codec": codec,
        "pix_fmt": pix_fmt,
        "fps": fps,
        "duration_sec": round(duration, 2),
        "size_mb": size_mb,
        "has_audio": a_stream is not None,
        "loudness": loudness,
        "extracted_frames": extracted_frames,
        "errors": errors,
        "warnings": warnings,
    }

    if args.json:
        print(json.dumps(report, indent=2))
    else:
        print(f"=== Render Verification ({args.target}): {report['status']} ===")
        print(
            f"Video: {width}x{height} @ {fps} fps | {codec}/{pix_fmt} | "
            f"{duration:.2f}s | {size_mb} MB | Audio: {'yes' if a_stream else 'no'}"
        )
        if loudness:
            print(
                f"Loudness: {loudness['input_i']:.1f} LUFS | "
                f"True Peak: {loudness['input_tp']:.1f} dBTP | LRA: {loudness['input_lra']:.1f} LU"
            )
        if extracted_frames:
            print(f"Extracted {len(extracted_frames)} QC frame(s) to {args.extract_frames}")
        for err in errors:
            print(f"[FAIL] {err}")
        for warn in warnings:
            print(f"[WARN] {warn}")
        if not errors and not warnings:
            print("All technical and loudness gates passed.")

    return 0 if report["status"] == "PASS" else 1


if __name__ == "__main__":
    sys.exit(main())
