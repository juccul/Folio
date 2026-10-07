#!/usr/bin/env python3
"""Score the shared OCR cohorts and retain paired output differences."""
import csv
import hashlib
import json
import re
from pathlib import Path
import sys

APP = Path(__file__).resolve().parents[1]
OLD = APP / 'artifacts/benchmarks/recognition-2026-10-04'
ROOT = APP / 'artifacts/benchmarks/vulkan-2026-10-07'
sys.path.insert(0, str(OLD))
import analyze
import editdistance
import numpy as np
from jinja2 import Environment


def vision_intervals(log):
    """Bracket synchronous image encoding using the server's trace timestamps."""
    intervals = []
    pending = None
    for line in log.splitlines():
        match = re.match(r'(\d+)\.(\d+)\.(\d+)\.(\d+) ', line)
        if not match:
            continue
        minutes, seconds, milliseconds, microseconds = map(int, match.groups())
        timestamp = minutes * 60 + seconds + milliseconds / 1000 + microseconds / 1e6
        if 'encoding mtmd batch from idx' in line:
            pending = timestamp
        elif 'decoding image batch 1/' in line and pending is not None:
            intervals.append((timestamp - pending) * 1000)
            pending = None
    return intervals


def main():
    summaries = []
    datasets = {}
    template = Environment(trim_blocks=True, lstrip_blocks=True).from_string(
        (OLD / 'models/glm-ocr/chat_template.jinja').read_text())
    verified_prompts = []
    for path in sorted((ROOT / 'results').glob('*.json')):
        data = json.loads(path.read_text())
        rows = data['samples']
        if len(rows) != 100:
            print(f'Incomplete: {path.name} ({len(rows)})', file=sys.stderr)
            continue
        kind = data['metadata']['kind']
        if 'rendered_prompt_probe' in data['metadata']:
            prompt = data['metadata']['rendered_prompt_probe']['prompt']
            normalized_prompt = re.sub(r'<__media_[^>]+__>', '<|begin_of_image|><|image|><|end_of_image|>', prompt)
            expected_prompt = template.render(messages=[{'role': 'user', 'content': [
                {'type': 'image'}, {'type': 'text', 'text': 'Text Recognition:' if kind == 'text' else 'Formula Recognition:'}]}],
                add_generation_prompt=True)
            assert normalized_prompt == expected_prompt, f'Prompt differs from baseline: {path.name}'
            verified_prompts.append({'run': path.stem, 'equivalent_after_media_marker_expansion': True,
                                     'checkpoint_template': expected_prompt})
        cohort = json.loads((OLD / f'{kind}-samples.json').read_text())
        assert [(r['id'], r['expected']) for r in rows] == [(r['id'], r['label']) for r in cohort]
        for row, source in zip(rows, cohort):
            if 'image_sha256' in row:
                assert row['image_sha256'] == hashlib.sha256(Path(source['image']).read_bytes()).hexdigest()
        summary = analyze.summarize(path)
        vision_ms = [sum(r['vision_encoding_ms']) for r in rows if r.get('vision_encoding_ms')]
        log_path = ROOT / 'logs' / (path.stem + '.log')
        if not vision_ms and path.stem.startswith('llamacpp-') and log_path.exists():
            intervals = vision_intervals(log_path.read_text())
            # One cold request, two warmups, then one fresh encoding per scored request.
            if len(intervals) == len(rows) + 3:
                vision_ms = intervals[3:]
                summary['vision_encoding_timing_method'] = 'Server trace interval from encoding start to image decoding start; includes encoder setup/transfers'
        if vision_ms:
            summary['vision_encoding_median_ms'] = float(np.median(vision_ms))
            summary['vision_encoding_p95_ms'] = float(np.percentile(vision_ms, 95))
        server_timings = [r['response'].get('timings', {}) for r in rows if 'response' in r]
        for key in ('prompt_ms', 'predicted_ms', 'cache_n', 'prompt_n', 'predicted_n'):
            values = [t[key] for t in server_timings if key in t]
            if values:
                summary[f'server_{key}_median'] = float(np.median(values))
                summary[f'server_{key}_max'] = max(values)
        summary['max_output_tokens'] = max(r.get('tokens', r.get('response', {}).get('usage', {}).get('completion_tokens', 0)) for r in rows)
        summaries.append(summary)
        datasets[path.stem] = data
    pairs = []
    random = np.random.default_rng(0)
    for name, data in datasets.items():
        kind = data['metadata']['kind']
        baseline_name = f'transformers-cuda-bf16-{kind}'
        if name == baseline_name or baseline_name not in datasets:
            continue
        base = datasets[baseline_name]['samples']
        rows = data['samples']
        token_differences = [r['response']['usage']['prompt_tokens'] - b['input_tokens']
                             for r, b in zip(rows, base) if 'response' in r]
        if token_differences:
            extra_inputs = {'different_input_token_counts': sum(d != 0 for d in token_differences),
                            'maximum_absolute_input_token_difference': max(abs(d) for d in token_differences)}
        else:
            extra_inputs = {}
        changes = [{'id': r['id'], 'expected': r['expected'], 'baseline': b['prediction'],
                    'prediction': r['prediction']} for r, b in zip(rows, base) if r['prediction'] != b['prediction']]
        indices = random.integers(0, len(rows), size=(10000, len(rows)))
        if kind == 'text':
            norm = analyze.compact
            errors = np.array([editdistance.eval(norm(r['expected']), norm(r['prediction'])) for r in rows])
            base_errors = np.array([editdistance.eval(norm(r['expected']), norm(r['prediction'])) for r in base])
            lengths = np.array([len(norm(r['expected'])) for r in rows])
            boot = (errors-base_errors)[indices].sum(axis=1)/lengths[indices].sum(axis=1)*100
            delta = float((errors-base_errors).sum()/lengths.sum()*100)
            metric = 'nonspace_CER_percentage_points'
        else:
            norm = analyze.formula_format
            errors = np.array([norm(r['expected']) == norm(r['prediction']) for r in rows], dtype=int)
            base_errors = np.array([norm(r['expected']) == norm(r['prediction']) for r in base], dtype=int)
            boot = (errors-base_errors)[indices].mean(axis=1)*100
            delta = float((errors-base_errors).mean()*100)
            metric = 'formula_match_percentage_points'
        extra = {} if kind == 'text' else {'lost_formula_matches': int(((base_errors == 1) & (errors == 0)).sum()),
                                          'gained_formula_matches': int(((base_errors == 0) & (errors == 1)).sum())}
        pairs.append({'run': name, 'baseline': baseline_name, 'raw_changed_outputs': len(changes), **extra, **extra_inputs,
                      'metric': metric, 'difference': delta,
                      'bootstrap_95_interval': np.percentile(boot, [2.5, 97.5]).tolist(),
                      'samples': changes})
    (ROOT / 'summary.json').write_text(json.dumps({'runs': summaries, 'paired': pairs}, indent=2) + '\n')
    (ROOT / 'prompt-verification.json').write_text(json.dumps(verified_prompts, indent=2) + '\n')
    fields = ['run', 'kind', 'n', 'median_ms', 'p95_ms', 'peak_rss_mib', 'current_rss_mib',
              'sampled_nvidia_process_peak_mib', 'peak_cuda_allocated_mib', 'peak_cuda_reserved_mib',
              'load_seconds', 'cold_first_inference_seconds', 'vision_encoding_median_ms',
              'server_prompt_ms_median', 'server_predicted_ms_median', 'max_output_tokens',
              'truncated', 'cer_raw_rate', 'cer_nonspace_rate', 'exact_format']
    with (ROOT / 'summary.csv').open('w') as out:
        writer = csv.DictWriter(out, fieldnames=fields, extrasaction='ignore')
        writer.writeheader()
        for summary in summaries:
            row = dict(summary)
            for name in ('cer_raw', 'cer_nonspace'):
                if name in row:
                    row[name + '_rate'] = row[name]['rate']
            writer.writerow(row)
    for row in summaries:
        quality = f"CER {row['cer_nonspace']['rate']:.2%}" if row['kind'] == 'text' else f"math {row['exact_format']}/100"
        print(f"{row['run']:<36} {quality:>13} median {row['median_ms']:.1f}ms P95 {row['p95_ms']:.1f}ms RSS {row['peak_rss_mib']:.0f}MiB")


if __name__ == '__main__':
    main()
