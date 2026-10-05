"""Geometry/offline regressions; no models or dataset downloads needed."""
import importlib.util
from pathlib import Path
import socket
import sys
from types import SimpleNamespace
import unittest
from unittest.mock import Mock, patch

spec = importlib.util.spec_from_file_location('recognition_worker', Path(__file__).with_name('recognition-worker.py'))
worker = importlib.util.module_from_spec(spec)
spec.loader.exec_module(worker)

class PreprocessingTests(unittest.TestCase):
    def test_paragraph_preserves_two_lines_and_late_dot_in_one_image(self):
        strokes = [[(0, 10), (100, 10)], [(0, 70), (100, 70)], [(25, 0)]]
        image = worker.render_ink(strokes, 'text')
        self.assertEqual(image.size, (232, 172))
        self.assertLess(image.getpixel((116, 36))[0], 100)
        self.assertLess(image.getpixel((116, 156))[0], 100)
        self.assertLess(image.getpixel((66, 16))[0], 100)
        self.assertEqual(image.getpixel((116, 96)), (255, 255, 255))

    def test_translation_does_not_change_ocr_pixels(self):
        strokes = [[(-10, 2), (20, 30)], [(5, 40)]]
        shifted = [[(x + 200, y - 100) for x, y in s] for s in strokes]
        for kind in ('text', 'math'):
            self.assertEqual(worker.render_ink(strokes, kind).tobytes(),
                             worker.render_ink(shifted, kind).tobytes())

    def test_math_preserves_fraction_layout_as_one_image(self):
        strokes = [[(0, 0), (10, 15)], [(0, 35), (10, 50)], [(-10, 25), (20, 25)]]
        image = worker.render_ink(strokes, 'math')
        self.assertEqual(image.mode, 'RGB')
        self.assertLessEqual(image.height, 512)
        self.assertLessEqual(image.width, 640)
        self.assertEqual(image.getpixel((0, 0)), (255, 255, 255))

    def test_math_wrappers_removed_without_changing_formula(self):
        expression = r'\frac{x-y}{\sqrt{2}} = a_{1}'
        for left, right in [('$$', '$$'), ('\\[', '\\]'), ('\\(', '\\)'), ('```latex\n', '\n```')]:
            self.assertEqual(worker.strip_math_wrappers(left + expression + right), expression)
        self.assertEqual(worker.strip_math_wrappers('$x$ + $y$'), '$x$ + $y$')
        self.assertEqual(worker.strip_math_wrappers(expression), expression)

    def test_text_and_math_reuse_one_model(self):
        recognizer = object.__new__(worker.Recognizer)
        recognizer.model = recognizer.processor = None
        recognizer.torch = SimpleNamespace(bfloat16='bf16')
        recognizer.device = 'cpu'
        recognizer.config = {}
        recognizer.path = Mock(return_value=Path('/local/glm'))
        model_loader, processor_loader = Mock(), Mock()
        module = SimpleNamespace(GlmOcrForConditionalGeneration=model_loader, AutoProcessor=processor_loader)
        with patch.dict(sys.modules, {'transformers': module}):
            recognizer.load('text')
            model = recognizer.model
            recognizer.load('math')
            self.assertIs(recognizer.model, model)
            model_loader.from_pretrained.assert_called_once()
            processor_loader.from_pretrained.assert_called_once()
            with self.assertRaises(ValueError):
                recognizer.load('unsupported')

    def test_truncated_generation_never_returns_partial_result(self):
        import torch
        from PIL import Image
        class Inputs(dict):
            def to(self, *args, **kwargs):
                return self
        recognizer = object.__new__(worker.Recognizer)
        recognizer.torch, recognizer.device = torch, 'cpu'
        recognizer.processor = Mock()
        recognizer.processor.apply_chat_template.return_value = Inputs(input_ids=torch.ones((1, 3), dtype=torch.long))
        recognizer.model = Mock()
        for kind, limit in worker.TOKEN_LIMITS.items():
            recognizer.model.generate.return_value = torch.ones((1, 3 + limit), dtype=torch.long)
            with self.assertRaisesRegex(ValueError, 'truncated'):
                recognizer.image(Image.new('RGB', (16, 16)), kind)
        recognizer.processor.batch_decode.assert_not_called()

    def test_int8_loading_maps_to_device_and_reuses_quantized_model(self):
        recognizer = object.__new__(worker.Recognizer)
        recognizer.model = recognizer.processor = None
        recognizer.torch = SimpleNamespace(bfloat16='bf16')
        recognizer.device = 'cpu'
        recognizer.config = {'quantization': 'int8'}
        recognizer.path = Mock(return_value=Path('/local/glm'))
        class Int8Layer:
            pass
        model = Mock(is_loaded_in_8bit=True)
        model.modules.return_value = [Int8Layer()]
        model.parameters.return_value = [SimpleNamespace(device=SimpleNamespace(type='cpu'))]
        loader = Mock()
        loader.from_pretrained.return_value = model
        quantization = Mock()
        module = SimpleNamespace(GlmOcrForConditionalGeneration=loader, AutoProcessor=Mock(),
                                 BitsAndBytesConfig=quantization)
        bnb = SimpleNamespace(nn=SimpleNamespace(Linear8bitLt=Int8Layer))
        with patch.dict(sys.modules, {'transformers': module, 'bitsandbytes': bnb}):
            recognizer.load('text')
            recognizer.load('math')
        quantization.assert_called_once_with(load_in_8bit=True, llm_int8_threshold=6.0)
        loader.from_pretrained.assert_called_once()
        self.assertEqual(loader.from_pretrained.call_args.kwargs['device_map'], {'': 'cpu'})
        self.assertIs(recognizer.model, model)
        model.to.assert_not_called()
        model.eval.assert_called_once()

    def test_int8_rejects_unquantized_or_offloaded_model(self):
        class Int8Layer:
            pass
        for layers, device, error in [([], 'cpu', 'INT8 weights'),
                                      ([Int8Layer()], 'cuda', 'offloaded')]:
            recognizer = object.__new__(worker.Recognizer)
            recognizer.model = recognizer.processor = None
            recognizer.torch = SimpleNamespace(bfloat16='bf16')
            recognizer.device = 'cpu'
            recognizer.config = {'quantization': 'int8'}
            recognizer.path = Mock(return_value=Path('/local/glm'))
            model = Mock(is_loaded_in_8bit=True)
            model.modules.return_value = layers
            model.parameters.return_value = [SimpleNamespace(device=SimpleNamespace(type=device))]
            loader = Mock()
            loader.from_pretrained.return_value = model
            module = SimpleNamespace(GlmOcrForConditionalGeneration=loader, AutoProcessor=Mock(),
                                     BitsAndBytesConfig=Mock())
            bnb = SimpleNamespace(nn=SimpleNamespace(Linear8bitLt=Int8Layer))
            with patch.dict(sys.modules, {'transformers': module, 'bitsandbytes': bnb}):
                with self.assertRaisesRegex(ValueError, error):
                    recognizer.load('text')
            self.assertIsNone(recognizer.model)

    def test_unknown_quantization_is_rejected(self):
        recognizer = object.__new__(worker.Recognizer)
        recognizer.model = None
        recognizer.config = {'quantization': 'typo'}
        recognizer.path = Mock(return_value=Path('/local/glm'))
        module = SimpleNamespace(GlmOcrForConditionalGeneration=Mock(), AutoProcessor=Mock())
        with patch.dict(sys.modules, {'transformers': module}):
            with self.assertRaisesRegex(ValueError, 'bf16 or int8'):
                recognizer.load('text')
        module.GlmOcrForConditionalGeneration.from_pretrained.assert_not_called()

    def test_nonfinite_and_empty_input_rejected(self):
        for value in [[], [[]], [[(1, float('nan'))]], [[(1, 2, 3)]]]:
            with self.assertRaises(ValueError):
                worker.validate_strokes(value)

    def test_image_input_is_bounded_and_preserves_crop_pixels(self):
        import tempfile
        import contextlib
        from PIL import Image
        recognizer=object.__new__(worker.Recognizer)
        recognizer.load=Mock()
        recognizer.image=Mock(return_value='x+1=3')
        recognizer.torch=SimpleNamespace(inference_mode=contextlib.nullcontext)
        with tempfile.TemporaryDirectory() as root:
            path=Path(root)/'crop.png'
            Image.new('RGB',(64,32),(7,20,30)).save(path)
            self.assertEqual(recognizer.recognize({'kind':'math','image_path':str(path)}),'x+1=3')
            self.assertEqual(recognizer.image.call_args.args[0].getpixel((0,0)),(7,20,30))
            Image.new('RGBA',(64,32),(0,0,0,0)).save(path)
            recognizer.recognize({'kind':'math','image_path':str(path)})
            self.assertEqual(recognizer.image.call_args.args[0].getpixel((0,0)),(255,255,255))
            Image.new('RGB',(2001,2000)).save(path)
            with self.assertRaisesRegex(ValueError,'smaller image'):
                recognizer.recognize({'kind':'math','image_path':str(path)})

    def test_internet_sockets_are_blocked(self):
        for family in [socket.AF_INET, socket.AF_INET6]:
            with self.assertRaisesRegex(OSError, 'disabled'):
                socket.socket(family)

if __name__ == '__main__':
    unittest.main()
