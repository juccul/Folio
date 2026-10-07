#!/usr/bin/env python3
"""Summarize Folio input reports/JSONL recordings and separately annotated camera CSV.
No pass/fail claim is inferred from delivered events or CPU paint timings.
"""
import argparse
import csv
import json
import math
from pathlib import Path

LIMIT = 100_000
MAX_BYTES = 64 * 1024 * 1024


def percentiles(values):
    if not values:
        return None
    values = sorted(values)
    def at(p):
        index = (len(values) - 1) * p
        low = math.floor(index)
        high = math.ceil(index)
        return values[low] + (values[high] - values[low]) * (index - low)
    return dict(samples=len(values), p50_ms=at(.5), p95_ms=at(.95),
                p99_ms=at(.99), max_ms=values[-1])


def analyze_recording(path):
    if path.stat().st_size > MAX_BYTES:
        raise ValueError('Recording exceeds 64 MiB; split the trial')
    text = path.read_text()
    try:
        report = json.loads(text)
    except json.JSONDecodeError:
        report = None
    if isinstance(report, dict) and report.get('type') == 'folio_input_check':
        if report.get('schema_version') != 1:
            raise ValueError('Unsupported input-report schema')
        return report
    counts, pressures, intervals, buttons, eraser = {}, [], [], 0, 0
    contact, last, summary, total = False, None, None, 0
    for line_number, line in enumerate(text.splitlines(), 1):
        if not line.strip():
            continue
        try:
            event = json.loads(line)
            if not isinstance(event, dict):
                raise ValueError('Expected an object')
            if event.get('type') == 'dispatch_to_cpu_paint':
                summary = event
                continue
            if event.get('type') != 'tablet':
                continue
            phase = event['phase']
            if phase not in {'Hover', 'Down', 'Move', 'Up', 'Leave', 'Cancel'}:
                raise ValueError('Unknown tablet phase')
            pressure = float(event['pressure'])
            timestamp = int(event['timestamp'])
            if not math.isfinite(pressure) or not 0 <= timestamp <= 0xFFFFFFFF:
                raise ValueError('Invalid pressure/timestamp')
            total += 1
            counts[phase] = counts.get(phase, 0) + 1
            buttons |= int(event.get('buttons', 0))
            eraser += bool(event.get('eraser'))
            if phase == 'Down':
                contact, last = True, None
            if contact and len(pressures) < LIMIT:
                pressures.append(pressure)
            if phase == 'Move' and contact:
                if last is not None:
                    gap = (timestamp - last) & 0xFFFFFFFF
                    if 0 < gap < 60_000 and len(intervals) < LIMIT:
                        intervals.append(gap)
                last = timestamp
            if phase in {'Up', 'Leave', 'Cancel'}:
                contact, last = False, None
        except (KeyError, TypeError, ValueError, json.JSONDecodeError) as error:
            raise ValueError(f'Invalid recording line {line_number}: {error}') from error
    if not total:
        raise ValueError('Recording has no tablet frames')
    return dict(type='tablet_recording_analysis', frames=total, phases=counts,
                contact_pressure_range=[min(pressures), max(pressures)] if pressures else None,
                observed_button_mask=buttons, eraser_frames=eraser,
                contact_move_intervals=percentiles(intervals), dispatch_to_cpu_paint=summary,
                sample_limit=LIMIT,
                timing_limits='Delivered frames and CPU paint only; excludes physical transport and display')


def analyze_camera(path, fps):
    if not math.isfinite(fps) or fps <= 0:
        raise ValueError('Camera FPS must be finite and positive')
    if path.stat().st_size > MAX_BYTES:
        raise ValueError('Camera CSV exceeds 64 MiB')
    samples = []
    with path.open(newline='') as source:
        rows = csv.DictReader(source)
        if not {'contact_frame', 'ink_frame'}.issubset(rows.fieldnames or []):
            raise ValueError('CSV requires contact_frame and ink_frame columns')
        for number, row in enumerate(rows, 2):
            try:
                contact, ink = int(row['contact_frame']), int(row['ink_frame'])
                if contact < 0 or ink < contact:
                    raise ValueError('Ink frame must follow non-negative contact frame')
                samples.append((ink - contact) * 1000 / fps)
                if len(samples) > LIMIT:
                    raise ValueError('Camera trial exceeds sample limit')
            except (TypeError, ValueError) as error:
                raise ValueError(f'Invalid camera row {number}: {error}') from error
    if not samples:
        raise ValueError('Camera CSV contains no annotations')
    return dict(type='camera_contact_to_visible_ink', camera_fps=fps,
                frame_resolution_ms=1000 / fps, latency=percentiles(samples),
                annotation_limits='Measured from supplied contact/visible-ink frames; annotation accuracy and provenance require human review')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--recording', type=Path)
    parser.add_argument('--camera-csv', type=Path)
    parser.add_argument('--camera-fps', type=float)
    args = parser.parse_args()
    if not (args.recording or args.camera_csv):
        parser.error('Provide --recording and/or --camera-csv')
    if bool(args.camera_csv) != (args.camera_fps is not None):
        parser.error('--camera-csv and --camera-fps must be supplied together')
    try:
        result = {'hardware_validation': 'No automatic certification; retain physical-trial evidence'}
        if args.recording:
            result['application'] = analyze_recording(args.recording)
        if args.camera_csv:
            result['physical_camera'] = analyze_camera(args.camera_csv, args.camera_fps)
        print(json.dumps(result, indent=2, allow_nan=False))
    except (OSError, ValueError) as error:
        parser.error(str(error))


if __name__ == '__main__':
    main()
