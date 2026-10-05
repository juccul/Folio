"""Local GLM-OCR projection optimization; preserves weights and image geometry."""
from types import MethodType


def project_full_patch(convolution, pixels):
    """A full-kernel convolution has exactly one dot product per output channel."""
    import torch.nn.functional as functional
    projected = functional.linear(pixels.flatten(1), convolution.weight.flatten(1), convolution.bias)
    return projected.reshape(pixels.shape[0], convolution.out_channels, 1, 1, 1)


def optimize_glm_encoder(model):
    """Use GEMM for the pinned model's CUDA BF16 full-patch Conv3d only.

    This instance-local adapter changes neither parameters nor serialization.
    CPU, other dtypes and unexpected input shapes keep the original operator.
    """
    import torch
    visual = getattr(getattr(model, 'model', None), 'visual', None)
    patch_embed = getattr(visual, 'patch_embed', None)
    convolution = getattr(patch_embed, 'proj', None)
    if not isinstance(convolution, torch.nn.Conv3d):
        return False
    if getattr(convolution, '_folio_full_patch_projection', False):
        return True
    if (convolution.groups != 1 or convolution.in_channels != 3
            or convolution.kernel_size != (2, 14, 14)
            or convolution.stride != convolution.kernel_size
            or convolution.padding != (0, 0, 0)
            or convolution.dilation != (1, 1, 1)
            or convolution.padding_mode != 'zeros'):
        return False
    original = convolution.forward

    def forward(self, pixels):
        if (pixels.device.type == 'cuda' and self.weight.device == pixels.device
                and pixels.dtype == self.weight.dtype == torch.bfloat16
                and pixels.ndim == 5 and pixels.shape[1] == self.in_channels
                and tuple(pixels.shape[2:]) == self.kernel_size):
            return project_full_patch(self, pixels)
        return original(pixels)

    convolution.forward = MethodType(forward, convolution)
    convolution._folio_full_patch_projection = True
    return True
