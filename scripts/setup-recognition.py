#!/usr/bin/env python3
"""Assemble a shared GLM-OCR pack from already downloaded, pinned local files.

Run with the inference runtime's Python. No network or model download.
"""
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import sys

ROOT = Path(__file__).resolve().parents[1]
MODEL_REVISION = '2e85a62840ccac27daa451df36c736c4636b8628'
MODEL_HASHES = {
    'README.md': '172334449c3df98650bc9a42781c254c64c9d466b440e2ca8ce31f99829212b2',
    'chat_template.jinja': '062e7ee4cc8defa88a5938b5d456dc60366ffd80647f918946ee747bf09ddc7c',
    'config.json': 'ad537817b2becde827cbb23569a1dd83a352ee64cc34ce751a4ef000ecce4b5f',
    'generation_config.json': '4e2a3412f65b2a21d315d928986c55723d3e60dfb92c3982a5b9fd56835b0aa5',
    'model.safetensors': 'a16eb0de98d199293371c560f95f83130d2a2c9612449df16839f08ff9498815',
    'preprocessor_config.json': '02cc50c36240882ae35e8cd4077a25a379664108185d728d261cb785aefeccff',
    'processor_config.json': 'a56fbd559186fefd6c22a3acc6a393a18a9fe52c264692a122a143d24042112b',
    'tokenizer.json': 'aa0fd058c73a5718bb191f6672dc16d122ee0147b20c123d1726514298f9968a',
    'tokenizer_config.json': '95abd49942be9cdc6e2653528aed7b4c1cb9e180828421d3488ab38da3057016',
}


def digest(path):
    with path.open('rb') as source:
        return hashlib.file_digest(source, 'sha256').hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path, default=ROOT / 'artifacts/recognition-v2')
    parser.add_argument('--ocr-model', type=Path,
                        default=ROOT / 'artifacts/benchmarks/recognition-2026-10-04/models/glm-ocr')
    parser.add_argument('--copy-model', action='store_true', help='Copy verified GLM files into the pack')
    parser.add_argument('--device', choices=['cpu', 'cuda'], default='cpu')
    parser.add_argument('--threads', type=int, default=4)
    parser.add_argument('--encoder-projection', choices=['linear_cuda', 'native'], default='linear_cuda')
    parser.add_argument('--quantization', choices=['bf16', 'int8'], default='bf16')
    args = parser.parse_args()
    import torch
    import transformers
    from transformers import GlmOcrForConditionalGeneration, AutoProcessor
    if transformers.__version__ != '5.18.0':
        parser.error('Install the pinned inference requirements; Transformers 5.18.0 is required')
    quantization_runtime = {}
    if args.quantization == 'int8':
        import bitsandbytes
        import accelerate
        if bitsandbytes.__version__ != '0.50.2' or accelerate.__version__ != '1.15.0':
            parser.error('INT8 requires the pinned scripts/recognition-int8-requirements.txt')
        quantization_runtime = {'bitsandbytes': bitsandbytes.__version__,
                                'accelerate': accelerate.__version__, 'llm_int8_threshold': 6.0}
    model = args.ocr_model.resolve()
    for filename, expected in MODEL_HASHES.items():
        path = model / filename
        if not path.is_file() or digest(path) != expected:
            parser.error(f'GLM-OCR does not match the pinned, validated checkpoint: {filename}')
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    if args.copy_model:
        destination = output / 'glm-ocr'
        destination.mkdir(exist_ok=True)
        for filename in MODEL_HASHES:
            if model / filename != destination / filename:
                shutil.copy2(model / filename, destination / filename)
        model_reference = 'glm-ocr'
    else:
        model_reference = str(model)
    shutil.copy2(ROOT / 'scripts/recognition-worker.py', output / 'recognition-worker.py')
    shutil.copy2(ROOT / 'scripts/recognition_encoder.py', output / 'recognition_encoder.py')
    # Retain the upstream declaration and attribution for referenced models too.
    shutil.copy2(model / 'README.md', output / 'GLM-OCR-MODEL-CARD.md')
    (output / 'MODEL-NOTICE.txt').write_text(
        'GLM-OCR by Z.ai / zai-org. Model weights declared MIT in the retained model card.\n'
        f'Repository: https://huggingface.co/zai-org/GLM-OCR/tree/{MODEL_REVISION}\n'
        'The Folio adapter uses native Transformers classes. The SDK and PP-DocLayout-V3 are not included.\n')
    config = {'python': sys.executable, 'worker': 'recognition-worker.py',
              'ocr_model': model_reference, 'device': args.device,
              'threads': max(1, min(8, args.threads)), 'encoder_projection': args.encoder_projection,
              'quantization': args.quantization}
    manifest = {'ocr': {'repository': 'zai-org/GLM-OCR', 'revision': MODEL_REVISION,
                        'license': 'MIT', 'tasks': ['text', 'math'], 'file_sha256': MODEL_HASHES},
                'runtime': {'torch': torch.__version__, 'transformers': transformers.__version__,
                            'encoder_projection': args.encoder_projection,
                            'quantization': args.quantization, **quantization_runtime}}
    (output / 'manifest.json').write_text(json.dumps(manifest, indent=2) + '\n')
    # Publish the configuration only after verification/copying succeeds.
    temporary = output / 'pack.json.tmp'
    temporary.write_text(json.dumps(config, indent=2) + '\n')
    temporary.replace(output / 'pack.json')
    if output == ROOT / 'artifacts/recognition-v2':
        link = ROOT / 'target/recognition'
        link.parent.mkdir(parents=True, exist_ok=True)
        if not link.exists() and not link.is_symlink():
            link.symlink_to(output, target_is_directory=True)
    print(f'Offline GLM-OCR ready for text and math: {output / "pack.json"}')
    print('Set FOLIO_RECOGNITION_CONFIG to that path, or install as recognition/pack.json in the Folio data directory.')


if __name__ == '__main__':
    main()
