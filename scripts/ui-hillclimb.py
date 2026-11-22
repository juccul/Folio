#!/usr/bin/env python3
"""UI hill-climb harness: capture, build judge panels and track council scores.

Everything is written under the ignored artifacts/ui-hillclimb/ directory.

  references              download reference-app screenshots from Flathub
  capture LABEL [--worktree DIR | --binary PATH]
                          build Folio from a checkout and capture the judged views
  panel ROUND --baseline LABEL --candidate LABEL
                          assemble the judge panel (side-by-side sheets + brief)
  record ROUND JUDGE FILE store one judge's JSON scores for a round
  table                   print the round-by-round score table (Markdown)
  check --threshold N --judges a,b
                          exit 0 only when every listed judge rates the candidate
                          >= N in every category of the latest round
"""
import argparse
import html
import json
import shutil
import subprocess
import sys
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
WORK = ROOT / 'artifacts/ui-hillclimb'
FIXTURE = ROOT / 'artifacts/validation/performance-pass/ui-fixture'

# The surfaces judged every round, in light and dark. Names come from
# scripts/capture-all-views.py.
VIEWS = [
    'library-grid', 'library-list', 'library-folder', 'search', 'document-menu',
    'editor', 'editor-pages', 'editor-selection', 'editor-actions-document',
    'pen-settings', 'settings-appearance', 'open-document',
]
MODES = ('light', 'dark')
CATEGORIES = [
    'hierarchy', 'typography', 'color_contrast', 'spacing_layout',
    'iconography', 'consistency', 'clarity', 'polish',
]
# Closest products in Folio's category; Flathub hosts curated screenshots.
REFERENCE_APPS = {
    'Rnote': 'com.github.flxzt.rnote',
    'Xournal++': 'com.github.xournalpp.xournalpp',
    'Saber': 'com.adilhanney.saber',
}

BRIEF = '''You are a senior product designer on a UI review panel for Folio, a
handwriting-first notes app (library of notebooks, infinite/paged ink canvas,
pen tools, settings).

Images in this folder:
- sheets/<mode>-<view>.png: the same Folio view, LEFT = "{baseline}", RIGHT =
  "{candidate}", side by side for overview. Screens are captured at 1440x1000
  from an isolated demo library.
- baseline/<mode>-<view>.png and candidate/<mode>-<view>.png: the same screens
  at full size. Judge text size, icon detail and fine contrast from these, as
  the side-by-side sheets are scaled down.
- references/*.png: screenshots of other apps in the same category ({refs}).
  Drop-in references may also appear there.

Open and look at every image. Judge what you see, not code.

Score each product from 1.0 to 10.0 (one decimal) in every category:
{categories}
10 means best-in-class shipped product quality; be strict and calibrated
against the references. Then list the most important concrete problems in
"{candidate}" (location, what is wrong, specific fix), most important first.
Do not edit any files.

Reply with ONLY this JSON (no prose around it):
{{
  "scores": {{
    "{baseline}":  {{"hierarchy": 0.0, ...every category...}},
    "{candidate}": {{...}},
    "<reference app name>": {{...}}
  }},
  "findings": [{{"view": "light-editor", "severity": "high|medium|low",
                 "problem": "...", "fix": "..."}}],
  "verdict": "one or two sentences"
}}
'''


def run(*command, **kwargs):
    print('+', ' '.join(map(str, command)), flush=True)
    return subprocess.run(command, check=True, **kwargs)


def references(args):
    out = WORK / 'references'
    out.mkdir(parents=True, exist_ok=True)
    for name, app in REFERENCE_APPS.items():
        with urllib.request.urlopen(f'https://flathub.org/api/v2/appstream/{app}', timeout=30) as response:
            data = json.load(response)
        for index, shot in enumerate(data.get('screenshots', [])[:args.per_app], 1):
            largest = max(shot['sizes'], key=lambda size: int(size['width']))
            target = out / f'{name.replace("+", "p")}-{index}.png'
            with urllib.request.urlopen(largest['src'], timeout=60) as response:
                target.write_bytes(response.read())
            print(target.relative_to(ROOT), shot.get('caption') or '')


def capture(args):
    if args.binary:
        binary = args.binary.resolve()
    else:
        checkout = (args.worktree or ROOT).resolve()
        run('cargo', 'build', '--locked', '--release', '-p', 'folio', cwd=checkout)
        binary = checkout / 'target/release/folio'
    out = WORK / 'captures' / args.label
    if out.exists():
        shutil.rmtree(out)
    run(sys.executable, ROOT / 'scripts/capture-all-views.py', '--binary', binary, '--fixture', FIXTURE,
        '--output', out, '--views', ','.join(args.views or VIEWS))
    missing = [f'{mode}-{view}' for view in (args.views or VIEWS) for mode in MODES if not (out / f'{mode}-{view}.png').is_file()]
    print('captured', out.relative_to(ROOT), 'missing:', missing or 'none')


def panel(args):
    from PIL import Image, ImageDraw, ImageFont
    base, cand = WORK / 'captures' / args.baseline, WORK / 'captures' / args.candidate
    out = WORK / 'rounds' / str(args.round)
    if out.exists():
        shutil.rmtree(out)
    (out / 'sheets').mkdir(parents=True)
    try:
        path = subprocess.check_output(['fc-match', '-f', '%{file}', 'sans:bold'], text=True)
        font = ImageFont.truetype(path, 26)
    except (OSError, subprocess.CalledProcessError):
        font = ImageFont.load_default(26)
    sheets = []
    for view in VIEWS:
        for mode in MODES:
            name = f'{mode}-{view}.png'
            if not ((base / name).is_file() and (cand / name).is_file()):
                continue
            left, right = Image.open(base / name).convert('RGB'), Image.open(cand / name).convert('RGB')
            bar, gap = 44, 24
            sheet = Image.new('RGB', (left.width + gap + right.width, bar + max(left.height, right.height)), '#808080')
            sheet.paste(left, (0, bar))
            sheet.paste(right, (left.width + gap, bar))
            draw = ImageDraw.Draw(sheet)
            draw.text((12, 8), f'{args.baseline}  ·  {mode} {view}', fill='white', font=font)
            draw.text((left.width + gap + 12, 8), args.candidate, fill='white', font=font)
            sheet.save(out / 'sheets' / name, optimize=True)
            for side, source in (('baseline', base), ('candidate', cand)):
                (out / side).mkdir(exist_ok=True)
                shutil.copy(source / name, out / side / name)
            sheets.append(name)
    refs = WORK / 'references'
    if refs.is_dir():
        shutil.copytree(refs, out / 'references')
    ref_names = sorted({p.stem.rsplit('-', 1)[0] for p in (out / 'references').glob('*.png')}) if refs.is_dir() else []
    (out / 'brief.txt').write_text(BRIEF.format(
        baseline=args.baseline, candidate=args.candidate, refs=', '.join(ref_names) or 'none',
        categories='\n'.join('- ' + c for c in CATEGORIES)))
    meta = {'baseline': args.baseline, 'candidate': args.candidate, 'sheets': sheets, 'references': ref_names}
    (out / 'panel.json').write_text(json.dumps(meta, indent=2) + '\n')
    gallery = ''.join(f'<h2>{html.escape(s)}</h2><a href="sheets/{s}"><img src="sheets/{s}"></a>' for s in sheets)
    (out / 'index.html').write_text('<!doctype html><meta charset="utf-8"><title>Round ' + str(args.round) +
        '</title><style>body{font:15px system-ui;background:#1b1b1b;color:#eee;margin:24px}img{width:100%}</style>' + gallery)
    print('panel', out, f'({len(sheets)} sheets, {len(ref_names)} reference apps)')


def parse_scores(text):
    start, end = text.find('{'), text.rfind('}')
    return json.loads(text[start:end + 1])


def record(args):
    out = WORK / 'rounds' / str(args.round)
    meta = json.loads((out / 'panel.json').read_text())
    data = parse_scores(Path(args.file).read_text())
    candidate = data['scores'].get(meta['candidate'])
    assert candidate, f'No scores for candidate {meta["candidate"]!r}'
    missing = [c for c in CATEGORIES if c not in candidate]
    assert not missing, f'Missing categories: {missing}'
    (out / 'scores').mkdir(exist_ok=True)
    (out / 'scores' / f'{args.judge}.json').write_text(json.dumps(data, indent=2) + '\n')
    print(args.judge, 'min', min(candidate[c] for c in CATEGORIES), 'mean', round(sum(candidate[c] for c in CATEGORIES) / len(CATEGORIES), 2))


def rounds():
    result = []
    for directory in sorted((WORK / 'rounds').glob('*'), key=lambda p: int(p.name) if p.name.isdigit() else 0):
        if (directory / 'panel.json').is_file():
            meta = json.loads((directory / 'panel.json').read_text())
            scores = {p.stem: json.loads(p.read_text())['scores'] for p in sorted((directory / 'scores').glob('*.json'))} if (directory / 'scores').is_dir() else {}
            result.append((directory.name, meta, scores))
    return result


def summary(values):
    numbers = [values[c] for c in CATEGORIES if c in values]
    return f'{sum(numbers) / len(numbers):.1f} (min {min(numbers):.1f})' if numbers else '—'


def table(args):
    history = rounds()
    judges = sorted({judge for _, _, scores in history for judge in scores})
    print('| Round | Product | ' + ' | '.join(judges) + ' |')
    print('|---|---|' + '---|' * len(judges))
    for number, meta, scores in history:
        products = list(dict.fromkeys([meta['candidate'], meta['baseline']])) + sorted({p for s in scores.values() for p in s} - {meta['candidate'], meta['baseline']})
        for product in products:
            label = f'**{product}**' if product == meta['candidate'] else product
            print(f'| {number} | {label} | ' + ' | '.join(summary(scores.get(j, {}).get(product, {})) for j in judges) + ' |')


def check(args):
    history = rounds()
    assert history, 'No rounds yet'
    number, meta, scores = history[-1]
    ok = True
    for judge in args.judges.split(','):
        values = scores.get(judge, {}).get(meta['candidate'])
        if not values:
            print(f'round {number}: {judge} has not scored'); ok = False; continue
        low = {c: values[c] for c in CATEGORIES if values[c] < args.threshold}
        print(f'round {number}: {judge}', 'PASS' if not low else f'below {args.threshold}: {low}')
        ok &= not low
    sys.exit(0 if ok else 1)


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = parser.add_subparsers(dest='command', required=True)
    p = sub.add_parser('references'); p.add_argument('--per-app', type=int, default=3); p.set_defaults(func=references)
    p = sub.add_parser('capture'); p.add_argument('label'); p.add_argument('--worktree', type=Path); p.add_argument('--binary', type=Path)
    p.add_argument('--views', type=lambda v: v.split(',')); p.set_defaults(func=capture)
    p = sub.add_parser('panel'); p.add_argument('round', type=int); p.add_argument('--baseline', required=True); p.add_argument('--candidate', required=True); p.set_defaults(func=panel)
    p = sub.add_parser('record'); p.add_argument('round', type=int); p.add_argument('judge'); p.add_argument('file'); p.set_defaults(func=record)
    p = sub.add_parser('table'); p.set_defaults(func=table)
    p = sub.add_parser('check'); p.add_argument('--threshold', type=float, required=True); p.add_argument('--judges', required=True); p.set_defaults(func=check)
    args = parser.parse_args()
    args.func(args)


if __name__ == '__main__':
    main()
