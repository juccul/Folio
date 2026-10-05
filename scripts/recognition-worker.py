#!/usr/bin/env python3
"""Resident local ink recognizer. JSON lines on stdin/stdout; diagnostics on stderr.

No Hub access, remote model code, language model cleanup, or document mutations.
The application owns cancellation, timeouts, review and undo.
"""
import argparse
import contextlib
import json
import math
import os
from pathlib import Path
import re
import socket
import sys

os.environ.update(HF_HUB_OFFLINE="1", TRANSFORMERS_OFFLINE="1",
                  HF_HUB_DISABLE_TELEMETRY="1", TOKENIZERS_PARALLELISM="false")
_socket = socket.socket

class OfflineSocket(_socket):
    def __init__(self, family=socket.AF_INET, *args, **kwargs):
        if family in (socket.AF_INET, socket.AF_INET6):
            raise OSError("Network access is disabled in Folio recognition")
        super().__init__(family, *args, **kwargs)

socket.socket = OfflineSocket

PROMPTS = {'text': 'Text Recognition:', 'math': 'Formula Recognition:'}
TOKEN_LIMITS = {'text': 1024, 'math': 512}

def validate_strokes(strokes):
    if not isinstance(strokes, list) or not 1 <= len(strokes) <= 4096:
        raise ValueError("Select 1–4096 ink strokes")
    count = 0
    clean = []
    for stroke in strokes:
        if not isinstance(stroke, list):
            raise ValueError("Invalid stroke")
        points = []
        for point in stroke:
            count += 1
            if count > 250_000 or len(point) != 2 or not all(
                    isinstance(v, (int, float)) and math.isfinite(v) for v in point):
                raise ValueError("Invalid or oversized stroke data")
            points.append(tuple(point))
        if points:
            clean.append(points)
    if not clean:
        raise ValueError("No visible ink in selection")
    return clean

def bounds(strokes):
    points = [p for s in strokes for p in s]
    return (min(p[0] for p in points), min(p[1] for p in points),
            max(p[0] for p in points), max(p[1] for p in points))

def render_ink(strokes, kind):
    from PIL import Image, ImageDraw
    x0, y0, x1, y1 = bounds(strokes)
    w, h = max(x1 - x0, 1), max(y1 - y0, 1)
    if kind == 'math':
        scale, margin, thickness = min(608/w, 480/h), 16, 1.75
    elif kind == 'text':
        # Preserve the entire selection's paragraph layout for image-based OCR.
        # Short words should not become huge vision inputs. The processor can
        # enlarge tiny crops itself; ordinary page strokes need at most 2x.
        scale, margin, thickness = min(1400/w, 1000/h, 2), 16, 1.75
    else:
        raise ValueError("Choose text or math recognition")
    size = (max(16, round(w * scale) + 2 * margin), max(16, round(h * scale) + 2 * margin))
    image = Image.new('RGB', (size[0] * 4, size[1] * 4), 'white')
    draw = ImageDraw.Draw(image)
    radius = thickness * 2
    for stroke in strokes:
        points = [((x-x0)*scale*4 + margin*4, (y-y0)*scale*4 + margin*4) for x, y in stroke]
        if len(points) > 1:
            draw.line(points, fill='black', width=round(thickness*4), joint='curve')
        for x, y in (points[:1] + points[-1:]):
            draw.ellipse((x-radius, y-radius, x+radius, y+radius), fill='black')
    return image.resize(size, Image.Resampling.LANCZOS)

def strip_math_wrappers(text):
    text = text.strip()
    # Only strip display wrappers, never rewrite mathematical meaning.
    for left, right in [('```latex', '```'), ('```', '```'), ('\\[', '\\]'),
                        ('\\(', '\\)'), ('$$', '$$'), ('$', '$')]:
        if text.startswith(left) and text.endswith(right) and len(text) > len(left) + len(right):
            inner = text[len(left):-len(right)].strip()
            if left in ('$', '$$') and re.search(r'(?<!\\)\$', inner):
                continue
            text = inner
    return text

class Recognizer:
    def __init__(self, config_path):
        self.root = config_path.parent
        self.config = json.loads(config_path.read_text())
        import torch
        self.torch = torch
        torch.set_num_threads(max(1, min(8, int(self.config.get('threads', 4)))))
        torch.set_num_interop_threads(1)
        # CPU is the fully open-source default. CUDA is an explicit local opt-in.
        self.device = self.config.get('device', 'cpu')
        if self.device not in ('cpu', 'cuda'):
            raise ValueError("Recognition device must be cpu or cuda")
        if self.device == 'cuda' and not torch.cuda.is_available():
            raise ValueError("Configured CUDA device is unavailable; choose cpu in pack.json")
        self.quantization = self.config.get('quantization', 'bf16')
        if self.quantization not in ('bf16', 'int8'):
            raise ValueError("Recognition quantization must be bf16 or int8")
        self.model, self.processor = None, None

    def path(self, key):
        value = self.config.get(key)
        if not value:
            raise ValueError(f"Offline {key} is missing from the recognition pack")
        path = Path(value)
        path = path if path.is_absolute() else self.root / path
        if not path.exists():
            raise ValueError(f"Offline model file is missing: {path}")
        return path

    def load(self, kind):
        if kind not in PROMPTS:
            raise ValueError("Choose text or math recognition")
        if self.model is not None:
            return
        from transformers import AutoProcessor, GlmOcrForConditionalGeneration
        path = self.path('ocr_model')
        processor = AutoProcessor.from_pretrained(path, local_files_only=True, trust_remote_code=False)
        quantization = self.config.get('quantization', 'bf16')
        if quantization not in ('bf16', 'int8'):
            raise ValueError("Recognition quantization must be bf16 or int8")
        options = {}
        if quantization == 'int8':
            from transformers import BitsAndBytesConfig
            options = {'quantization_config': BitsAndBytesConfig(
                load_in_8bit=True, llm_int8_threshold=6.0),
                'device_map': {'': self.device}}
        model = GlmOcrForConditionalGeneration.from_pretrained(
            path, local_files_only=True, trust_remote_code=False,
            dtype=self.torch.bfloat16, attn_implementation='sdpa', **options)
        quantized_layers = 0
        if quantization == 'int8':
            import bitsandbytes
            quantized_layers = sum(isinstance(module, bitsandbytes.nn.Linear8bitLt)
                                   for module in model.modules())
            if not quantized_layers or not getattr(model, 'is_loaded_in_8bit', False):
                raise ValueError("Requested INT8 recognition did not load INT8 weights")
            if any(parameter.device.type != self.device for parameter in model.parameters()):
                raise ValueError("INT8 recognition has unexpected offloaded parameters")
        else:
            model = model.to(self.device)
        model.eval()
        projection = self.config.get('encoder_projection', 'linear_cuda')
        if projection not in ('linear_cuda', 'native'):
            raise ValueError("Encoder projection must be linear_cuda or native")
        if self.device == 'cuda' and projection == 'linear_cuda':
            from recognition_encoder import optimize_glm_encoder
            if not optimize_glm_encoder(model):
                raise ValueError("GLM encoder geometry does not match the validated projection")
        self.processor, self.model = processor, model
        print(f"Folio OCR: GLM-OCR {quantization.upper()} on {self.device}; "
              f"{quantized_layers} INT8 linear layers; encoder={projection}", file=sys.stderr)

    def image(self, image, kind):
        messages = [{'role': 'user', 'content': [
                {'type': 'image', 'image': image.convert('RGB')},
                {'type': 'text', 'text': PROMPTS[kind]}]}]
        inputs = self.processor.apply_chat_template(
            messages, tokenize=True, add_generation_prompt=True, return_dict=True,
            return_tensors='pt').to(self.device, dtype=self.torch.bfloat16)
        limit = TOKEN_LIMITS[kind]
        output = self.model.generate(**inputs, max_new_tokens=limit, do_sample=False, num_beams=1, use_cache=True)
        if output.shape[-1] - inputs['input_ids'].shape[-1] >= limit:
            raise ValueError("Recognition output was truncated; select a smaller region")
        text = self.processor.batch_decode(output[:, inputs['input_ids'].shape[-1]:],
                skip_special_tokens=True, clean_up_tokenization_spaces=False)[0].strip()
        return strip_math_wrappers(text) if kind == 'math' else text

    def recognize(self, request):
        kind = request.get('kind')
        image_path = request.get('image_path')
        if image_path:
            from PIL import Image
            with Image.open(image_path) as source:
                if source.width*source.height > 4_000_000:
                    raise ValueError('Select a smaller image region for recognition')
                image = Image.new('RGB',source.size,'white')
                rgba=source.convert('RGBA');image.paste(rgba,mask=rgba.getchannel('A'))
        else:
            strokes = validate_strokes(request.get('strokes'))
            image = render_ink(strokes, kind)
        self.load(kind)
        with self.torch.inference_mode():
            return self.image(image, kind)

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--config', type=Path, required=True)
    args = parser.parse_args()
    # Bind lifetime to parent even after a crash. Normal cancellation uses kill/wait.
    if sys.platform == 'linux':
        import ctypes
        import signal
        parent = os.getppid()
        ctypes.CDLL(None).prctl(1, signal.SIGTERM, 0, 0, 0)
        if os.getppid() != parent:
            return
    recognizer = None
    for line in sys.stdin:
        try:
            if len(line) > 16_000_000:
                raise ValueError("Recognition request is too large")
            with contextlib.redirect_stdout(sys.stderr):
                if recognizer is None:
                    recognizer = Recognizer(args.config.resolve())
                text = recognizer.recognize(json.loads(line))
            response = {'text': text}
        except Exception as error:
            response = {'error': str(error)}
        print(json.dumps(response, ensure_ascii=False), flush=True)

if __name__ == '__main__':
    main()
