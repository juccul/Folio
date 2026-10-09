#!/usr/bin/env python3
"""Verify packaged offline tools without a system Python or Poppler installation."""
import argparse
import json
from pathlib import Path
import struct
import sqlite3
import subprocess
import tempfile


def make_pdf(path):
    content = b'0 0 1 rg 20 20 80 80 re f\n'
    objects = [b'<< /Type /Catalog /Pages 2 0 R >>',
               b'<< /Type /Pages /Kids [3 0 R] /Count 1 >>',
               b'<< /Type /Page /Parent 2 0 R /MediaBox [0 0 300 400] /Contents 4 0 R >>',
               f'<< /Length {len(content)} >>\nstream\n'.encode() + content + b'endstream']
    data = bytearray(b'%PDF-1.4\n')
    offsets = [0]
    for index, body in enumerate(objects, 1):
        offsets.append(len(data))
        data.extend(f'{index} 0 obj\n'.encode() + body + b'\nendobj\n')
    start = len(data)
    data.extend(f'xref\n0 {len(offsets)}\n0000000000 65535 f \n'.encode())
    for offset in offsets[1:]:
        data.extend(f'{offset:010} 00000 n \n'.encode())
    data.extend(f'trailer\n<< /Size {len(offsets)} /Root 1 0 R >>\nstartxref\n{start}\n%%EOF\n'.encode())
    path.write_bytes(data)



def verify_pen(database, output):
    with sqlite3.connect(database) as connection:
        assert connection.execute('SELECT count(*) FROM notes').fetchone()[0] == 1, 'Cancelled page setup created an extra document'
        objects = [json.loads(row[0]) for row in connection.execute('SELECT data FROM objects')]
    strokes = [obj['Stroke'] for obj in objects if 'Stroke' in obj]
    assert len(strokes) == 1, f'Expected one native pen stroke, found {len(strokes)}'
    points = strokes[0]['raw']
    assert len(points) >= 10, len(points)
    pressure = [point['pressure'] for point in points]
    assert max(pressure) - min(pressure) > .3, pressure
    assert any(point['tilt_x'] == 20 and point['tilt_y'] == -10 for point in points), points
    evidence = {'native_windows_ink': True, 'pressure_and_tilt': True,
                'no_duplicate_stroke': True, 'samples': len(points)}
    output.write_text(json.dumps(evidence, indent=2) + '\n', encoding='utf-8')
    print(json.dumps(evidence))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--package', type=Path)
    parser.add_argument('--pen-database', type=Path)
    parser.add_argument('--output', type=Path)
    args = parser.parse_args()
    if args.pen_database:
        verify_pen(args.pen_database, args.output)
        return
    if not args.package:
        parser.error('--package or --pen-database is required')
    package = args.package.resolve()
    pack_path = package / 'math-solver/pack.json'
    pack = json.loads(pack_path.read_text(encoding='utf-8'))
    python = (pack_path.parent / pack['python']).resolve()
    worker = (pack_path.parent / pack['worker']).resolve()
    with tempfile.TemporaryDirectory(prefix='folio-windows-tools-') as temporary:
        root = Path(temporary)
        # Invoke from an unrelated directory to detect hidden working-directory dependencies.
        requests = [{'expression': '2*x+3=11', 'operation': 'solve'}, [],
                    {'expression': '6*7', 'operation': 'evaluate'},
                    {'expression': '2×3', 'operation': 'evaluate'}]
        result = subprocess.run([str(python), '-u', str(worker), '--config', str(pack_path)],
                                input=''.join(json.dumps(r) + '\n' for r in requests),
                                capture_output=True, text=True, encoding='utf-8', cwd=root, timeout=30)
        if result.returncode:
            raise RuntimeError(result.stderr)
        responses = [json.loads(line) for line in result.stdout.splitlines()]
        assert len(responses) == 4, responses
        assert responses[0].get('answer') == '{4}', responses[0]
        assert responses[1].get('error'), responses[1]
        assert responses[2].get('answer') == '42', responses[2]
        assert responses[3].get('answer') == '6' and responses[3].get('input') == '2×3', responses[3]
        source = root / 'PDF with spaces.pdf'
        make_pdf(source)
        preview = root / 'preview'
        tool = package / 'pdf/bin/pdftoppm.exe'
        result = subprocess.run([str(tool), '-f', '1', '-l', '1', '-singlefile', '-scale-to', '800',
                                 '-png', str(source), str(preview)], cwd=root, capture_output=True, timeout=30)
        if result.returncode:
            raise RuntimeError(result.stderr.decode(errors='replace'))
        data = preview.with_suffix('.png').read_bytes()
        assert data[:8] == b'\x89PNG\r\n\x1a\n'
        width, height = struct.unpack('>II', data[16:24])
        assert width == 600 and height == 800, (width, height)
    evidence = {'offline_math': True, 'malformed_request_recovery': True,
                'portable_pdf_preview': True, 'unrelated_working_directory': True, 'unicode_protocol': True}
    if args.output:
        args.output.write_text(json.dumps(evidence, indent=2) + '\n', encoding='utf-8')
    print(json.dumps(evidence))


if __name__ == '__main__':
    main()
