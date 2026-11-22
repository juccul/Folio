//! Pure-Rust SIMD-friendly tensor kernels shared by Math v2 and Handwriting v2 Conformer models.
use crate::frec::{FrecFile, WeightMatrix};
use rayon::prelude::*;

#[inline(always)]
pub fn gelu(x: f32) -> f32 {
    // Exact PyTorch GELU (erf approximation matches nn.GELU to <1e-6)
    0.5 * x * (1.0 + erff_approx(x * std::f32::consts::FRAC_1_SQRT_2))
}

#[inline(always)]
fn erff_approx(x: f32) -> f32 {
    // Abramowitz & Stegun 7.1.26 (max error 1.5e-7)
    let sign = x.signum();
    let a = x.abs();
    let t = 1.0 / (1.0 + 0.3275911 * a);
    let y = 1.0
        - (((((1.061405429 * t - 1.453152027) * t) + 1.421413741) * t - 0.284496736) * t
            + 0.254829592)
            * t
            * (-a * a).exp();
    sign * y
}

#[inline(always)]
pub fn silu(x: f32) -> f32 {
    x / (1.0 + (-x).exp())
}

#[inline(always)]
pub fn sigmoid(x: f32) -> f32 {
    1.0 / (1.0 + (-x).exp())
}

/// Apply LayerNorm over the last dimension `d` of `(t, d)` tensor.
pub fn layer_norm(x: &[f32], t: usize, d: usize, weight: &[f32], bias: &[f32], out: &mut [f32]) {
    let eps = 1e-5_f32;
    let inv_d = 1.0 / d as f32;
    for row in 0..t {
        let xr = &x[row * d..(row + 1) * d];
        let or = &mut out[row * d..(row + 1) * d];
        let mut sum = 0.0_f32;
        for &v in xr {
            sum += v;
        }
        let mean = sum * inv_d;
        let mut var = 0.0_f32;
        for &v in xr {
            let diff = v - mean;
            var += diff * diff;
        }
        let inv_std = 1.0 / (var * inv_d + eps).sqrt();
        for i in 0..d {
            or[i] = (xr[i] - mean) * inv_std * weight[i] + bias[i];
        }
    }
}

/// Inner dot product between f32 activation vector and i8 quantized weight row.
/// Uses runtime-dispatched AVX2+FMA on x86_64 with a portable 8-wide fallback.
#[inline(always)]
fn dot_f32_i8_scalar(x: &[f32], w: &[i8]) -> f32 {
    let n = x.len();
    let chunks = n / 8;
    let mut a0 = 0.0_f32;
    let mut a1 = 0.0_f32;
    let mut a2 = 0.0_f32;
    let mut a3 = 0.0_f32;
    let mut a4 = 0.0_f32;
    let mut a5 = 0.0_f32;
    let mut a6 = 0.0_f32;
    let mut a7 = 0.0_f32;
    for c in 0..chunks {
        let i = c * 8;
        a0 += x[i] * (w[i] as f32);
        a1 += x[i + 1] * (w[i + 1] as f32);
        a2 += x[i + 2] * (w[i + 2] as f32);
        a3 += x[i + 3] * (w[i + 3] as f32);
        a4 += x[i + 4] * (w[i + 4] as f32);
        a5 += x[i + 5] * (w[i + 5] as f32);
        a6 += x[i + 6] * (w[i + 6] as f32);
        a7 += x[i + 7] * (w[i + 7] as f32);
    }
    let mut sum = (a0 + a1) + (a2 + a3) + (a4 + a5) + (a6 + a7);
    for i in (chunks * 8)..n {
        sum += x[i] * (w[i] as f32);
    }
    sum
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2,fma")]
unsafe fn hsum256_ps(v: std::arch::x86_64::__m256) -> f32 {
    use std::arch::x86_64::*;
    let hi = _mm256_extractf128_ps(v, 1);
    let lo = _mm256_castps256_ps128(v);
    let sum128 = _mm_add_ps(lo, hi);
    let shuf = _mm_movehdup_ps(sum128);
    let sums = _mm_add_ps(sum128, shuf);
    let shuf2 = _mm_movehl_ps(shuf, sums);
    _mm_cvtss_f32(_mm_add_ss(sums, shuf2))
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2,fma")]
unsafe fn dot_f32_i8_avx2(x: &[f32], w: &[i8]) -> f32 {
    use std::arch::x86_64::*;
    let n = x.len();
    let xp = x.as_ptr();
    let wp = w.as_ptr();
    let mut acc0 = _mm256_setzero_ps();
    let mut acc1 = _mm256_setzero_ps();
    let mut i = 0usize;
    while i + 16 <= n {
        unsafe {
            let w0_i8 = _mm_loadl_epi64(wp.add(i) as *const __m128i);
            let w0_f = _mm256_cvtepi32_ps(_mm256_cvtepi8_epi32(w0_i8));
            let x0 = _mm256_loadu_ps(xp.add(i));
            acc0 = _mm256_fmadd_ps(x0, w0_f, acc0);

            let w1_i8 = _mm_loadl_epi64(wp.add(i + 8) as *const __m128i);
            let w1_f = _mm256_cvtepi32_ps(_mm256_cvtepi8_epi32(w1_i8));
            let x1 = _mm256_loadu_ps(xp.add(i + 8));
            acc1 = _mm256_fmadd_ps(x1, w1_f, acc1);
        }
        i += 16;
    }
    let mut sum = unsafe { hsum256_ps(_mm256_add_ps(acc0, acc1)) };
    while i < n {
        sum += x[i] * (w[i] as f32);
        i += 1;
    }
    sum
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2,fma")]
unsafe fn linear_i8_block8_avx2(
    x: &[f32],
    wdata: &[i8],
    scales: &[f32],
    bias: Option<&[f32]>,
    in_dim: usize,
    out_dim: usize,
    out_block: &mut [f32],
) {
    use std::arch::x86_64::*;
    let x0 = x.as_ptr();
    let x1 = unsafe { x0.add(in_dim) };
    let x2 = unsafe { x0.add(2 * in_dim) };
    let x3 = unsafe { x0.add(3 * in_dim) };
    let x4 = unsafe { x0.add(4 * in_dim) };
    let x5 = unsafe { x0.add(5 * in_dim) };
    let x6 = unsafe { x0.add(6 * in_dim) };
    let x7 = unsafe { x0.add(7 * in_dim) };
    let wp_base = wdata.as_ptr();

    for o in 0..out_dim {
        let wp = unsafe { wp_base.add(o * in_dim) };
        let mut s0 = _mm256_setzero_ps();
        let mut s1 = _mm256_setzero_ps();
        let mut s2 = _mm256_setzero_ps();
        let mut s3 = _mm256_setzero_ps();
        let mut s4 = _mm256_setzero_ps();
        let mut s5 = _mm256_setzero_ps();
        let mut s6 = _mm256_setzero_ps();
        let mut s7 = _mm256_setzero_ps();
        let mut k = 0usize;
        while k + 8 <= in_dim {
            unsafe {
                let wi8 = _mm_loadl_epi64(wp.add(k) as *const __m128i);
                let wf = _mm256_cvtepi32_ps(_mm256_cvtepi8_epi32(wi8));
                s0 = _mm256_fmadd_ps(_mm256_loadu_ps(x0.add(k)), wf, s0);
                s1 = _mm256_fmadd_ps(_mm256_loadu_ps(x1.add(k)), wf, s1);
                s2 = _mm256_fmadd_ps(_mm256_loadu_ps(x2.add(k)), wf, s2);
                s3 = _mm256_fmadd_ps(_mm256_loadu_ps(x3.add(k)), wf, s3);
                s4 = _mm256_fmadd_ps(_mm256_loadu_ps(x4.add(k)), wf, s4);
                s5 = _mm256_fmadd_ps(_mm256_loadu_ps(x5.add(k)), wf, s5);
                s6 = _mm256_fmadd_ps(_mm256_loadu_ps(x6.add(k)), wf, s6);
                s7 = _mm256_fmadd_ps(_mm256_loadu_ps(x7.add(k)), wf, s7);
            }
            k += 8;
        }
        let mut r0 = unsafe { hsum256_ps(s0) };
        let mut r1 = unsafe { hsum256_ps(s1) };
        let mut r2 = unsafe { hsum256_ps(s2) };
        let mut r3 = unsafe { hsum256_ps(s3) };
        let mut r4 = unsafe { hsum256_ps(s4) };
        let mut r5 = unsafe { hsum256_ps(s5) };
        let mut r6 = unsafe { hsum256_ps(s6) };
        let mut r7 = unsafe { hsum256_ps(s7) };
        while k < in_dim {
            let wf = wdata[o * in_dim + k] as f32;
            r0 += x[k] * wf;
            r1 += x[in_dim + k] * wf;
            r2 += x[2 * in_dim + k] * wf;
            r3 += x[3 * in_dim + k] * wf;
            r4 += x[4 * in_dim + k] * wf;
            r5 += x[5 * in_dim + k] * wf;
            r6 += x[6 * in_dim + k] * wf;
            r7 += x[7 * in_dim + k] * wf;
            k += 1;
        }
        let sc = scales[o];
        let b = bias.map_or(0.0, |bs| bs[o]);
        out_block[o] = r0 * sc + b;
        out_block[out_dim + o] = r1 * sc + b;
        out_block[2 * out_dim + o] = r2 * sc + b;
        out_block[3 * out_dim + o] = r3 * sc + b;
        out_block[4 * out_dim + o] = r4 * sc + b;
        out_block[5 * out_dim + o] = r5 * sc + b;
        out_block[6 * out_dim + o] = r6 * sc + b;
        out_block[7 * out_dim + o] = r7 * sc + b;
    }
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2,fma")]
unsafe fn dot_f32_f32_avx2(x: &[f32], w: &[f32]) -> f32 {
    use std::arch::x86_64::*;
    let n = x.len();
    let xp = x.as_ptr();
    let wp = w.as_ptr();
    let mut acc0 = _mm256_setzero_ps();
    let mut acc1 = _mm256_setzero_ps();
    let mut i = 0usize;
    while i + 16 <= n {
        unsafe {
            acc0 = _mm256_fmadd_ps(_mm256_loadu_ps(xp.add(i)), _mm256_loadu_ps(wp.add(i)), acc0);
            acc1 = _mm256_fmadd_ps(
                _mm256_loadu_ps(xp.add(i + 8)),
                _mm256_loadu_ps(wp.add(i + 8)),
                acc1,
            );
        }
        i += 16;
    }
    let mut sum = unsafe { hsum256_ps(_mm256_add_ps(acc0, acc1)) };
    while i < n {
        sum += x[i] * w[i];
        i += 1;
    }
    sum
}

#[inline(always)]
fn dot_f32_i8(x: &[f32], w: &[i8], use_avx2: bool) -> f32 {
    #[cfg(target_arch = "x86_64")]
    if use_avx2 {
        return unsafe { dot_f32_i8_avx2(x, w) };
    }
    let _ = use_avx2;
    dot_f32_i8_scalar(x, w)
}

#[inline(always)]
fn dot_f32_f32_scalar(x: &[f32], w: &[f32]) -> f32 {
    let n = x.len();
    let chunks = n / 8;
    let mut a0 = 0.0_f32;
    let mut a1 = 0.0_f32;
    let mut a2 = 0.0_f32;
    let mut a3 = 0.0_f32;
    let mut a4 = 0.0_f32;
    let mut a5 = 0.0_f32;
    let mut a6 = 0.0_f32;
    let mut a7 = 0.0_f32;
    for c in 0..chunks {
        let i = c * 8;
        a0 += x[i] * w[i];
        a1 += x[i + 1] * w[i + 1];
        a2 += x[i + 2] * w[i + 2];
        a3 += x[i + 3] * w[i + 3];
        a4 += x[i + 4] * w[i + 4];
        a5 += x[i + 5] * w[i + 5];
        a6 += x[i + 6] * w[i + 6];
        a7 += x[i + 7] * w[i + 7];
    }
    let mut sum = (a0 + a1) + (a2 + a3) + (a4 + a5) + (a6 + a7);
    for i in (chunks * 8)..n {
        sum += x[i] * w[i];
    }
    sum
}

#[inline(always)]
fn dot_f32_f32(x: &[f32], w: &[f32], use_avx2: bool) -> f32 {
    #[cfg(target_arch = "x86_64")]
    if use_avx2 {
        return unsafe { dot_f32_f32_avx2(x, w) };
    }
    let _ = use_avx2;
    dot_f32_f32_scalar(x, w)
}

#[inline(always)]
fn has_avx2_fma() -> bool {
    #[cfg(target_arch = "x86_64")]
    {
        std::is_x86_feature_detected!("avx2") && std::is_x86_feature_detected!("fma")
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        false
    }
}

/// Linear projection `out[t, out_dim] = x[t, in_dim] @ W^T + bias`.
/// Parallelized with Rayon using 8-token weight reuse and runtime AVX2+FMA kernels.
pub fn linear(
    x: &[f32],
    t: usize,
    w: &WeightMatrix<'_>,
    bias: Option<&[f32]>,
    out: &mut [f32],
) {
    let avx2 = has_avx2_fma();
    match *w {
        WeightMatrix::I8Row {
            data,
            scales,
            out_dim,
            in_dim,
        } => {
            if t == 1 {
                let x0 = &x[..in_dim];
                if out_dim >= 256 {
                    out[..out_dim].par_chunks_mut(64).enumerate().for_each(|(ci, chunk)| {
                        let base = ci * 64;
                        for (j, slot) in chunk.iter_mut().enumerate() {
                            let o = base + j;
                            let row = &data[o * in_dim..(o + 1) * in_dim];
                            let b = bias.map_or(0.0, |bs| bs[o]);
                            *slot = dot_f32_i8(x0, row, avx2) * scales[o] + b;
                        }
                    });
                } else {
                    for o in 0..out_dim {
                        let row = &data[o * in_dim..(o + 1) * in_dim];
                        let b = bias.map_or(0.0, |bs| bs[o]);
                        out[o] = dot_f32_i8(x0, row, avx2) * scales[o] + b;
                    }
                }
            } else {
                // Parallelize over sequence rows in chunks of 8 (8x weight reuse per load!)
                out[..t * out_dim]
                    .par_chunks_mut(out_dim * 8)
                    .enumerate()
                    .for_each(|(bi, out_block)| {
                        let t0 = bi * 8;
                        let rows = out_block.len() / out_dim;
                        #[cfg(target_arch = "x86_64")]
                        if avx2 && rows == 8 {
                            let x_slice = &x[t0 * in_dim..(t0 + 8) * in_dim];
                            unsafe {
                                linear_i8_block8_avx2(
                                    x_slice, data, scales, bias, in_dim, out_dim, out_block,
                                );
                            }
                            return;
                        }
                        if rows == 4 || rows == 8 {
                            for sub in 0..(rows / 4) {
                                let st = t0 + sub * 4;
                                let ob = &mut out_block[sub * 4 * out_dim..(sub + 1) * 4 * out_dim];
                                let x0 = &x[st * in_dim..(st + 1) * in_dim];
                                let x1 = &x[(st + 1) * in_dim..(st + 2) * in_dim];
                                let x2 = &x[(st + 2) * in_dim..(st + 3) * in_dim];
                                let x3 = &x[(st + 3) * in_dim..(st + 4) * in_dim];
                                for o in 0..out_dim {
                                    let wrow = &data[o * in_dim..(o + 1) * in_dim];
                                    let sc = scales[o];
                                    let b = bias.map_or(0.0, |bs| bs[o]);
                                    let mut s0 = 0.0_f32;
                                    let mut s1 = 0.0_f32;
                                    let mut s2 = 0.0_f32;
                                    let mut s3 = 0.0_f32;
                                    for k in 0..in_dim {
                                        let wf = wrow[k] as f32;
                                        s0 += x0[k] * wf;
                                        s1 += x1[k] * wf;
                                        s2 += x2[k] * wf;
                                        s3 += x3[k] * wf;
                                    }
                                    ob[o] = s0 * sc + b;
                                    ob[out_dim + o] = s1 * sc + b;
                                    ob[2 * out_dim + o] = s2 * sc + b;
                                    ob[3 * out_dim + o] = s3 * sc + b;
                                }
                            }
                        } else {
                            for r in 0..rows {
                                let xr = &x[(t0 + r) * in_dim..(t0 + r + 1) * in_dim];
                                let or = &mut out_block[r * out_dim..(r + 1) * out_dim];
                                for o in 0..out_dim {
                                    let wrow = &data[o * in_dim..(o + 1) * in_dim];
                                    let b = bias.map_or(0.0, |bs| bs[o]);
                                    or[o] = dot_f32_i8(xr, wrow, avx2) * scales[o] + b;
                                }
                            }
                        }
                    });
            }
        }
        WeightMatrix::F32 {
            data,
            out_dim,
            in_dim,
        } => {
            out[..t * out_dim]
                .par_chunks_mut(out_dim)
                .enumerate()
                .for_each(|(r, or)| {
                    let xr = &x[r * in_dim..(r + 1) * in_dim];
                    for o in 0..out_dim {
                        let wrow = &data[o * in_dim..(o + 1) * in_dim];
                        let b = bias.map_or(0.0, |bs| bs[o]);
                        or[o] = dot_f32_f32(xr, wrow, avx2) + b;
                    }
                });
        }
    }
}

/// 1D Convolution over `(t_in, c_in)` -> `(t_out, c_out)` with stride `stride` and zero padding `pad`.
/// `weight` is PyTorch layout `[c_out, c_in, k]`.
pub fn conv1d(
    x: &[f32],
    t_in: usize,
    c_in: usize,
    weight: &[f32],
    bias: &[f32],
    c_out: usize,
    k: usize,
    stride: usize,
    pad: usize,
    apply_gelu: bool,
) -> (Vec<f32>, usize) {
    let t_out = (t_in + 2 * pad - k) / stride + 1;
    let mut out = vec![0.0_f32; t_out * c_out];
    for to in 0..t_out {
        let base_in = (to * stride) as isize - pad as isize;
        let or = &mut out[to * c_out..(to + 1) * c_out];
        for co in 0..c_out {
            let mut sum = bias[co];
            let w_co = &weight[co * c_in * k..(co + 1) * c_in * k];
            for ki in 0..k {
                let ti = base_in + ki as isize;
                if ti >= 0 && (ti as usize) < t_in {
                    let xr = &x[ti as usize * c_in..(ti as usize + 1) * c_in];
                    for ci in 0..c_in {
                        sum += xr[ci] * w_co[ci * k + ki];
                    }
                }
            }
            or[co] = if apply_gelu { gelu(sum) } else { sum };
        }
    }
    (out, t_out)
}

/// Depthwise 1D Convolution over `(t, d)` with `groups = d`, `stride = 1`, `pad = k / 2`.
/// `weight` is PyTorch layout `[d, 1, k]`.
pub fn depthwise_conv1d(x: &[f32], t: usize, d: usize, weight: &[f32], bias: &[f32], k: usize, out: &mut [f32]) {
    let pad = (k / 2) as isize;
    for ti in 0..t {
        let or = &mut out[ti * d..(ti + 1) * d];
        or.copy_from_slice(bias);
        for ki in 0..k {
            let src = ti as isize + ki as isize - pad;
            if src >= 0 && (src as usize) < t {
                let xr = &x[src as usize * d..(src as usize + 1) * d];
                for c in 0..d {
                    or[c] += xr[c] * weight[c * k + ki];
                }
            }
        }
    }
}

/// Precomputed Rotary Positional Embedding table `(max_len, head_dim / 2)`.
pub struct RopeTable {
    pub cos: Vec<f32>,
    pub sin: Vec<f32>,
    pub half_hd: usize,
}

impl RopeTable {
    pub fn new(head_dim: usize, max_len: usize) -> Self {
        let half_hd = head_dim / 2;
        let mut inv_freq = vec![0.0_f32; half_hd];
        for i in 0..half_hd {
            inv_freq[i] = 1.0 / 10000.0_f32.powf((2 * i) as f32 / head_dim as f32);
        }
        let mut cos = vec![0.0_f32; max_len * half_hd];
        let mut sin = vec![0.0_f32; max_len * half_hd];
        for pos in 0..max_len {
            let p = pos as f32;
            for i in 0..half_hd {
                let angle = p * inv_freq[i];
                cos[pos * half_hd + i] = angle.cos();
                sin[pos * half_hd + i] = angle.sin();
            }
        }
        Self { cos, sin, half_hd }
    }

    /// Apply RoPE in-place to a vector slice of length `head_dim` at position `pos`.
    #[inline(always)]
    pub fn apply_vec(&self, v: &mut [f32], pos: usize) {
        let off = pos * self.half_hd;
        for i in 0..self.half_hd {
            let c = self.cos[off + i];
            let s = self.sin[off + i];
            let x1 = v[2 * i];
            let x2 = v[2 * i + 1];
            v[2 * i] = x1 * c - x2 * s;
            v[2 * i + 1] = x1 * s + x2 * c;
        }
    }
}

pub fn softmax_inplace(v: &mut [f32]) {
    let mut max_v = f32::NEG_INFINITY;
    for &x in v.iter() {
        if x > max_v {
            max_v = x;
        }
    }
    let mut sum = 0.0_f32;
    for x in v.iter_mut() {
        *x = (*x - max_v).exp();
        sum += *x;
    }
    let inv = 1.0 / sum.max(1e-12);
    for x in v.iter_mut() {
        *x *= inv;
    }
}

/// Shared RoPE Conformer block forward pass for both `StrokeConformerCTC` (HTR v2)
/// and `StrokeMathConformerV2` (Math v2).
/// `conv_prefix`: `"conv"` for HTR (`self.conv.norm`...) or `""` for Math (`self.conv_norm`, `self.pw1`...).
pub fn conformer_block(
    frec: &FrecFile,
    prefix: &str,
    conv_nested: bool,
    x: &mut [f32],
    t: usize,
    d: usize,
    nhead: usize,
    dim_ff: usize,
    conv_kernel: usize,
    rope: &RopeTable,
) -> Result<(), String> {
    let mut norm_buf = vec![0.0_f32; t * d];
    let mut ff_mid = vec![0.0_f32; t * dim_ff];
    let mut tmp_d = vec![0.0_f32; t * d];

    // 1. Half-step FFN 1
    let (w, _) = frec.f32_slice(&format!("{prefix}.ff1_norm.weight"))?;
    let (b, _) = frec.f32_slice(&format!("{prefix}.ff1_norm.bias"))?;
    layer_norm(x, t, d, w, b, &mut norm_buf);
    let w1 = frec.linear(&format!("{prefix}.ff1.0.weight"))?;
    let b1 = frec.opt_f32(&format!("{prefix}.ff1.0.bias"));
    linear(&norm_buf, t, &w1, b1, &mut ff_mid);
    for v in &mut ff_mid {
        *v = silu(*v);
    }
    let w2 = frec.linear(&format!("{prefix}.ff1.3.weight"))?;
    let b2 = frec.opt_f32(&format!("{prefix}.ff1.3.bias"));
    linear(&ff_mid, t, &w2, b2, &mut tmp_d);
    for i in 0..t * d {
        x[i] += 0.5 * tmp_d[i];
    }

    // 2. RoPE Multi-Head Self-Attention
    let (w, _) = frec.f32_slice(&format!("{prefix}.attn_norm.weight"))?;
    let (b, _) = frec.f32_slice(&format!("{prefix}.attn_norm.bias"))?;
    layer_norm(x, t, d, w, b, &mut norm_buf);
    let wqkv = frec.linear(&format!("{prefix}.qkv.weight"))?;
    let bqkv = frec.opt_f32(&format!("{prefix}.qkv.bias"));
    let mut qkv = vec![0.0_f32; t * 3 * d];
    linear(&norm_buf, t, &wqkv, bqkv, &mut qkv);

    let hd = d / nhead;
    let scale = 1.0 / (hd as f32).sqrt();
    // Rearrange into Q, K, V per head: [nhead, t, hd] and apply RoPE to Q and K
    let mut qs = vec![0.0_f32; nhead * t * hd];
    let mut ks = vec![0.0_f32; nhead * t * hd];
    let mut vs = vec![0.0_f32; nhead * t * hd];
    for pos in 0..t {
        let base = pos * 3 * d;
        for h in 0..nhead {
            let dst = (h * t + pos) * hd;
            qs[dst..dst + hd].copy_from_slice(&qkv[base + h * hd..base + (h + 1) * hd]);
            ks[dst..dst + hd].copy_from_slice(&qkv[base + d + h * hd..base + d + (h + 1) * hd]);
            vs[dst..dst + hd].copy_from_slice(&qkv[base + 2 * d + h * hd..base + 2 * d + (h + 1) * hd]);
            rope.apply_vec(&mut qs[dst..dst + hd], pos);
            rope.apply_vec(&mut ks[dst..dst + hd], pos);
        }
    }

    let mut attn_out = vec![0.0_f32; t * d];
    let head_outs: Vec<Vec<f32>> = (0..nhead)
        .into_par_iter()
        .map(|h| {
            let qh = &qs[h * t * hd..(h + 1) * t * hd];
            let kh = &ks[h * t * hd..(h + 1) * t * hd];
            let vh = &vs[h * t * hd..(h + 1) * t * hd];
            let mut out_h = vec![0.0_f32; t * hd];
            let mut scores = vec![0.0_f32; t];
            for i in 0..t {
                let qi = &qh[i * hd..(i + 1) * hd];
                for j in 0..t {
                    let kj = &kh[j * hd..(j + 1) * hd];
                    let mut s = 0.0_f32;
                    for k in 0..hd {
                        s += qi[k] * kj[k];
                    }
                    scores[j] = s * scale;
                }
                softmax_inplace(&mut scores);
                let oi = &mut out_h[i * hd..(i + 1) * hd];
                for j in 0..t {
                    let wj = scores[j];
                    let vj = &vh[j * hd..(j + 1) * hd];
                    for k in 0..hd {
                        oi[k] += wj * vj[k];
                    }
                }
            }
            out_h
        })
        .collect();
    for h in 0..nhead {
        for pos in 0..t {
            attn_out[pos * d + h * hd..pos * d + (h + 1) * hd]
                .copy_from_slice(&head_outs[h][pos * hd..(pos + 1) * hd]);
        }
    }

    let wproj = frec.linear(&format!("{prefix}.attn_proj.weight"))?;
    let bproj = frec.opt_f32(&format!("{prefix}.attn_proj.bias"));
    linear(&attn_out, t, &wproj, bproj, &mut tmp_d);
    for i in 0..t * d {
        x[i] += tmp_d[i];
    }

    // 3. Conformer Depthwise Conv Module
    let (cn_name, pw1_name, dw_name, post_name, pw2_name) = if conv_nested {
        (
            format!("{prefix}.conv.norm"),
            format!("{prefix}.conv.pw1"),
            format!("{prefix}.conv.dw"),
            format!("{prefix}.conv.post_norm"),
            format!("{prefix}.conv.pw2"),
        )
    } else {
        (
            format!("{prefix}.conv_norm"),
            format!("{prefix}.pw1"),
            format!("{prefix}.dw"),
            format!("{prefix}.conv_post"),
            format!("{prefix}.pw2"),
        )
    };
    let (w, _) = frec.f32_slice(&format!("{cn_name}.weight"))?;
    let (b, _) = frec.f32_slice(&format!("{cn_name}.bias"))?;
    layer_norm(x, t, d, w, b, &mut norm_buf);
    let wpw1 = frec.linear(&format!("{pw1_name}.weight"))?;
    let bpw1 = frec.opt_f32(&format!("{pw1_name}.bias"));
    let mut glu_in = vec![0.0_f32; t * 2 * d];
    linear(&norm_buf, t, &wpw1, bpw1, &mut glu_in);
    let mut glu_out = vec![0.0_f32; t * d];
    for pos in 0..t {
        let a = &glu_in[pos * 2 * d..pos * 2 * d + d];
        let g = &glu_in[pos * 2 * d + d..(pos + 1) * 2 * d];
        let o = &mut glu_out[pos * d..(pos + 1) * d];
        for c in 0..d {
            o[c] = a[c] * sigmoid(g[c]);
        }
    }
    let (dw_w, _) = frec.f32_slice(&format!("{dw_name}.weight"))?;
    let (dw_b, _) = frec.f32_slice(&format!("{dw_name}.bias"))?;
    depthwise_conv1d(&glu_out, t, d, dw_w, dw_b, conv_kernel, &mut tmp_d);
    let (pn_w, _) = frec.f32_slice(&format!("{post_name}.weight"))?;
    let (pn_b, _) = frec.f32_slice(&format!("{post_name}.bias"))?;
    layer_norm(&tmp_d, t, d, pn_w, pn_b, &mut norm_buf);
    for v in &mut norm_buf {
        *v = silu(*v);
    }
    let wpw2 = frec.linear(&format!("{pw2_name}.weight"))?;
    let bpw2 = frec.opt_f32(&format!("{pw2_name}.bias"));
    linear(&norm_buf, t, &wpw2, bpw2, &mut tmp_d);
    for i in 0..t * d {
        x[i] += tmp_d[i];
    }

    // 4. Half-step FFN 2 + final LayerNorm
    let (w, _) = frec.f32_slice(&format!("{prefix}.ff2_norm.weight"))?;
    let (b, _) = frec.f32_slice(&format!("{prefix}.ff2_norm.bias"))?;
    layer_norm(x, t, d, w, b, &mut norm_buf);
    let w1 = frec.linear(&format!("{prefix}.ff2.0.weight"))?;
    let b1 = frec.opt_f32(&format!("{prefix}.ff2.0.bias"));
    linear(&norm_buf, t, &w1, b1, &mut ff_mid);
    for v in &mut ff_mid {
        *v = silu(*v);
    }
    let w2 = frec.linear(&format!("{prefix}.ff2.3.weight"))?;
    let b2 = frec.opt_f32(&format!("{prefix}.ff2.3.bias"));
    linear(&ff_mid, t, &w2, b2, &mut tmp_d);
    for i in 0..t * d {
        x[i] += 0.5 * tmp_d[i];
    }
    let (fw, _) = frec.f32_slice(&format!("{prefix}.final_norm.weight"))?;
    let (fb, _) = frec.f32_slice(&format!("{prefix}.final_norm.bias"))?;
    layer_norm(x, t, d, fw, fb, &mut norm_buf);
    x.copy_from_slice(&norm_buf);
    Ok(())
}
