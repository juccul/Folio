"""Projection equivalence, geometry fallbacks, and unchanged model parameters."""
from types import SimpleNamespace
import unittest

import torch
from recognition_encoder import optimize_glm_encoder, project_full_patch


class EncoderTests(unittest.TestCase):
    def setUp(self):
        torch.manual_seed(0)

    def model(self, convolution):
        return SimpleNamespace(model=SimpleNamespace(visual=SimpleNamespace(
            patch_embed=SimpleNamespace(proj=convolution))))

    def test_linear_projection_matches_full_patch_with_and_without_bias(self):
        for bias in (True, False):
            convolution = torch.nn.Conv3d(3, 7, (2, 14, 14), stride=(2, 14, 14), bias=bias).double()
            pixels = torch.randn(3, 3, 2, 14, 14, dtype=torch.float64).transpose(-1, -2)
            self.assertFalse(pixels.is_contiguous())
            torch.testing.assert_close(project_full_patch(convolution, pixels), convolution(pixels),
                                       rtol=1e-12, atol=1e-12)

    def test_adapter_preserves_parameters_and_cpu_forward(self):
        convolution = torch.nn.Conv3d(3, 7, (2, 14, 14), stride=(2, 14, 14)).double()
        pixels = torch.randn(2, 3, 2, 14, 14, dtype=torch.float64)
        expected = convolution(pixels)
        parameters = {name: id(value) for name, value in convolution.named_parameters()}
        state = {name: value.clone() for name, value in convolution.state_dict().items()}
        self.assertTrue(optimize_glm_encoder(self.model(convolution)))
        self.assertTrue(optimize_glm_encoder(self.model(convolution)))
        torch.testing.assert_close(convolution(pixels), expected, rtol=0, atol=0)
        self.assertEqual(parameters, {name: id(value) for name, value in convolution.named_parameters()})
        for name, value in convolution.state_dict().items():
            torch.testing.assert_close(value, state[name], rtol=0, atol=0)

    def test_unknown_or_overlapping_geometry_is_rejected(self):
        self.assertFalse(optimize_glm_encoder(SimpleNamespace()))
        for convolution in (
                torch.nn.Conv3d(3, 7, (2, 14, 14), stride=1),
                torch.nn.Conv3d(3, 7, (2, 14, 14), stride=(2, 14, 14), padding=1),
                torch.nn.Conv3d(3, 7, (1, 14, 14), stride=(1, 14, 14)),
                torch.nn.Conv3d(6, 8, (2, 14, 14), stride=(2, 14, 14), groups=2)):
            self.assertFalse(optimize_glm_encoder(self.model(convolution)))

    @unittest.skipUnless(torch.cuda.is_available(), 'CUDA-only optimized path')
    def test_cuda_bf16_path_and_larger_input_fallback(self):
        convolution = torch.nn.Conv3d(3, 7, (2, 14, 14), stride=(2, 14, 14)).cuda().bfloat16()
        patch = torch.randn(2, 3, 2, 14, 14, device='cuda', dtype=torch.bfloat16)
        larger = torch.randn(2, 3, 2, 14, 28, device='cuda', dtype=torch.bfloat16)
        original, fallback = convolution(patch), convolution(larger)
        self.assertTrue(optimize_glm_encoder(self.model(convolution)))
        torch.testing.assert_close(convolution(patch), original, rtol=0.02, atol=0.01)
        torch.testing.assert_close(convolution(larger), fallback, rtol=0, atol=0)


if __name__ == '__main__':
    unittest.main()
