#!/usr/bin/env python3
"""Validate spoken launch video scripts against runtime word budgets and pacing anti-patterns.

Usage:
    python3 scripts/script_budget.py --duration 60 --pace launch script.txt
    cat script.txt | python3 scripts/script_budget.py --duration 45 --pace enterprise --json
"""

import argparse
import json
import re
import sys
from pathlib import Path

PACE_WPM = {
    "enterprise": 135,
    "launch": 150,
    "social": 165,
}

VISUAL_RESERVE_RATIO = 0.17

BANNED_FILLER = [
    "seamless",
    "seamlessly",
    "empowering",
    "empower",
    "unlock",
    "revolutionize",
    "revolutionary",
    "game-changer",
    "game changer",
    "cutting-edge",
    "next-generation",
    "synergy",
    "supercharge",
    "effortless",
    "effortlessly",
    "reimagine",
]

TAIL_CLICHES = [
    "in summary",
    "to summarize",
    "to wrap up",
    "to wrap things up",
    "that's about it",
    "thats about it",
    "in conclusion",
    "thanks for watching",
    "hi everyone",
    "hello everyone",
    "welcome back",
]


def strip_stage_directions(text: str) -> str:
    # Remove bracketed stage directions like [0:00 - Hook] or (cut to UI)
    cleaned = re.sub(r"\[[^\]]*\]", " ", text)
    cleaned = re.sub(r"\([^)]*\)", " ", cleaned)
    # Remove markdown headings and timestamp prefixes like 0:05 -
    lines = []
    for line in cleaned.splitlines():
        stripped = line.strip()
        if not stripped or stripped.startswith("#"):
            continue
        stripped = re.sub(r"^\d{1,2}:\d{2}(?:\.\d+)?\s*[-:]?\s*", "", stripped)
        lines.append(stripped)
    return " ".join(lines)


def analyze_script(raw_text: str, duration_sec: float, wpm: int) -> dict:
    spoken = strip_stage_directions(raw_text)
    words = re.findall(r"[A-Za-z0-9']+", spoken)
    word_count = len(words)

    speech_window_sec = duration_sec * (1.0 - VISUAL_RESERVE_RATIO)
    max_words = int(round((speech_window_sec * wpm) / 60.0))
    est_speech_sec = (word_count / wpm) * 60.0 if wpm > 0 else 0.0
    visual_hold_left_sec = duration_sec - est_speech_sec

    sentences = [s.strip() for s in re.split(r"[.!?]+", spoken) if s.strip()]
    sentence_lengths = [len(re.findall(r"[A-Za-z0-9']+", s)) for s in sentences]
    avg_sentence_len = round(sum(sentence_lengths) / len(sentence_lengths), 1) if sentence_lengths else 0.0
    long_sentences = [
        {"sentence": s, "words": n}
        for s, n in zip(sentences, sentence_lengths)
        if n > 24
    ]

    lower_spoken = spoken.lower()
    filler_hits = sorted({term for term in BANNED_FILLER if re.search(r"\b" + re.escape(term) + r"\b", lower_spoken)})
    cliche_hits = sorted({term for term in TAIL_CLICHES if re.search(r"\b" + re.escape(term) + r"\b", lower_spoken)})

    errors = []
    warnings = []

    if word_count == 0:
        errors.append("No spoken words detected after stripping stage directions.")
    if word_count > max_words:
        errors.append(
            f"Over word budget: {word_count} words exceeds max {max_words} words for {duration_sec:.0f}s at {wpm} WPM "
            f"(with 17% visual-hold reserve). Trim at least {word_count - max_words} words."
        )
    if avg_sentence_len > 22.0:
        warnings.append(f"Average sentence length is {avg_sentence_len} words (target 15-20 for spoken cadence).")
    if long_sentences:
        warnings.append(f"{len(long_sentences)} sentence(s) exceed 24 words; split for cleaner vocal rhythm.")
    if filler_hits:
        warnings.append(f"Corporate filler detected: {', '.join(filler_hits)}. Replace with concrete product mechanics.")
    if cliche_hits:
        errors.append(f"Intro/tail retention-killer cliche detected: {', '.join(cliche_hits)}. Remove immediately.")

    return {
        "status": "PASS" if not errors else "FAIL",
        "duration_sec": duration_sec,
        "wpm": wpm,
        "speech_window_sec": round(speech_window_sec, 1),
        "word_count": word_count,
        "max_words": max_words,
        "headroom_words": max_words - word_count,
        "estimated_speech_sec": round(est_speech_sec, 1),
        "remaining_visual_hold_sec": round(visual_hold_left_sec, 1),
        "sentence_count": len(sentences),
        "avg_sentence_words": avg_sentence_len,
        "long_sentences": long_sentences,
        "filler_hits": filler_hits,
        "cliche_hits": cliche_hits,
        "errors": errors,
        "warnings": warnings,
    }


def main() -> int:
    parser = argparse.ArgumentParser(description="Validate a software launch video script budget and cadence.")
    parser.add_argument("script_file", nargs="?", help="Path to script text/markdown file (or read from stdin).")
    parser.add_argument("--duration", type=float, required=True, help="Target total runtime in seconds (e.g., 60).")
    parser.add_argument("--pace", choices=PACE_WPM.keys(), default="launch", help="Speaking pace preset.")
    parser.add_argument("--wpm", type=int, help="Optional custom WPM override.")
    parser.add_argument("--json", action="store_true", help="Output machine-readable JSON.")
    args = parser.parse_args()

    if args.script_file:
        text = Path(args.script_file).read_text(encoding="utf-8")
    else:
        text = sys.stdin.read()

    wpm = args.wpm if args.wpm else PACE_WPM[args.pace]
    report = analyze_script(text, args.duration, wpm)

    if args.json:
        print(json.dumps(report, indent=2))
    else:
        print(f"=== Script Budget Audit: {report['status']} ===")
        print(
            f"Words: {report['word_count']} / {report['max_words']} max "
            f"({report['duration_sec']:.0f}s @ {report['wpm']} WPM, 17% visual hold)"
        )
        print(
            f"Est. Speech: {report['estimated_speech_sec']}s | "
            f"Visual Hold Left: {report['remaining_visual_hold_sec']}s | "
            f"Avg Sentence: {report['avg_sentence_words']} words"
        )
        for err in report["errors"]:
            print(f"[FAIL] {err}")
        for warn in report["warnings"]:
            print(f"[WARN] {warn}")
        if not report["errors"] and not report["warnings"]:
            print("All pacing, word-budget, and copy checks passed cleanly.")

    return 0 if report["status"] == "PASS" else 1


if __name__ == "__main__":
    sys.exit(main())
