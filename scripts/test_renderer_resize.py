#!/usr/bin/env python3
"""Test the production GPUI resize policy without GPUI's upstream dev dependencies."""
import json
import os
from pathlib import Path
import subprocess
import tempfile


def main():
    root = Path(__file__).resolve().parents[1]
    policy = root / 'vendor/gpui/src/platform/blade/resize.rs'
    acknowledgement = root / 'vendor/gpui/src/platform/linux/x11/resize_ack.rs'
    with tempfile.TemporaryDirectory(prefix='folio-renderer-resize-') as temporary:
        directory = Path(temporary)
        wrapper = directory / 'resize_tests.rs'
        wrapper.write_text(
            '#[path = ' + json.dumps(str(policy), ensure_ascii=False) + ']\nmod resize;\n'
            '#[path = ' + json.dumps(str(acknowledgement), ensure_ascii=False)
            + ']\nmod resize_ack;\n',
            encoding='utf-8',
        )
        executable = directory / ('resize_tests.exe' if os.name == 'nt' else 'resize_tests')
        subprocess.run([
            'rustc', '--edition=2024', '--test', '--crate-name', 'folio_renderer_resize',
            str(wrapper), '-o', str(executable),
        ], check=True)
        subprocess.run([str(executable)], check=True)


if __name__ == '__main__':
    main()
