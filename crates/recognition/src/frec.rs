//! Memory-mapped loader for FREC0001 quantized stroke-recognition weight bundles.
use memmap2::Mmap;
use serde::Deserialize;
use std::{collections::HashMap, fs::File, path::Path};

const MAGIC: &[u8; 8] = b"FREC0001";

#[derive(Debug, Deserialize)]
pub struct ModelConfig {
    pub vocab_size: usize,
    pub in_features: usize,
    pub d_model: usize,
    pub nhead: usize,
    #[serde(default)]
    pub num_layers: usize,
    #[serde(default)]
    pub num_encoder_layers: usize,
    #[serde(default)]
    pub num_decoder_layers: usize,
    pub dim_ff: usize,
    #[serde(default = "default_conv_kernel")]
    pub conv_kernel: usize,
    #[serde(default)]
    pub pad_id: usize,
}

fn default_conv_kernel() -> usize {
    15
}

#[derive(Debug, Deserialize)]
pub struct TensorDesc {
    pub dtype: String,
    pub shape: Vec<usize>,
    pub offset: usize,
    pub len: usize,
    pub scale_offset: Option<usize>,
    pub scale_len: Option<usize>,
}

#[derive(Debug, Deserialize)]
pub struct Header {
    pub kind: String,
    pub config: ModelConfig,
    pub vocab: Vec<String>,
    pub tensors: HashMap<String, TensorDesc>,
}

pub struct FrecFile {
    mmap: Mmap,
    pub header: Header,
}

pub enum WeightMatrix<'a> {
    F32 {
        data: &'a [f32],
        out_dim: usize,
        in_dim: usize,
    },
    I8Row {
        data: &'a [i8],
        scales: &'a [f32],
        out_dim: usize,
        in_dim: usize,
    },
}

impl FrecFile {
    pub fn open(path: &Path) -> Result<Self, String> {
        let file = File::open(path)
            .map_err(|e| format!("Cannot open recognition model {}: {e}", path.display()))?;
        // SAFETY: model bundles are immutable bundled/verified files.
        let mmap = unsafe { Mmap::map(&file) }
            .map_err(|e| format!("Cannot map recognition model {}: {e}", path.display()))?;
        if mmap.len() < 12 || &mmap[0..8] != MAGIC {
            return Err(format!("Invalid recognition model header: {}", path.display()));
        }
        let hdr_len = u32::from_le_bytes(mmap[8..12].try_into().unwrap()) as usize;
        if 12 + hdr_len > mmap.len() {
            return Err(format!("Truncated recognition model header: {}", path.display()));
        }
        let header: Header = serde_json::from_slice(&mmap[12..12 + hdr_len])
            .map_err(|e| format!("Invalid recognition model metadata: {e}"))?;
        // Validate every tensor slice fits inside mmap and has valid alignment.
        for (name, t) in &header.tensors {
            let end = t
                .offset
                .checked_add(t.len)
                .ok_or_else(|| format!("Overflow in tensor {name}"))?;
            if end > mmap.len() {
                return Err(format!("Tensor {name} exceeds file bounds"));
            }
            if t.dtype == "f32" && (t.len % 4 != 0 || t.offset % 4 != 0) {
                return Err(format!("Unaligned f32 tensor {name}"));
            }
            if let (Some(so), Some(sl)) = (t.scale_offset, t.scale_len) {
                let se = so
                    .checked_add(sl)
                    .ok_or_else(|| format!("Overflow in scales for {name}"))?;
                if se > mmap.len() || sl % 4 != 0 || so % 4 != 0 {
                    return Err(format!("Invalid scale slice for {name}"));
                }
            }
        }
        Ok(Self { mmap, header })
    }

    pub fn f32_slice(&self, name: &str) -> Result<(&[f32], &[usize]), String> {
        let desc = self
            .header
            .tensors
            .get(name)
            .ok_or_else(|| format!("Missing tensor {name}"))?;
        if desc.dtype != "f32" {
            return Err(format!("Expected f32 tensor for {name}, got {}", desc.dtype));
        }
        let bytes = &self.mmap[desc.offset..desc.offset + desc.len];
        let slice: &[f32] = bytemuck::cast_slice(bytes);
        Ok((slice, &desc.shape))
    }

    pub fn opt_f32(&self, name: &str) -> Option<&[f32]> {
        self.f32_slice(name).ok().map(|(s, _)| s)
    }

    pub fn linear(&self, name: &str) -> Result<WeightMatrix<'_>, String> {
        let desc = self
            .header
            .tensors
            .get(name)
            .ok_or_else(|| format!("Missing linear tensor {name}"))?;
        if desc.shape.len() != 2 {
            return Err(format!("Expected 2D weight for {name}"));
        }
        let (out_dim, in_dim) = (desc.shape[0], desc.shape[1]);
        match desc.dtype.as_str() {
            "f32" => {
                let data: &[f32] = bytemuck::cast_slice(&self.mmap[desc.offset..desc.offset + desc.len]);
                Ok(WeightMatrix::F32 { data, out_dim, in_dim })
            }
            "i8_row" => {
                let data: &[i8] = bytemuck::cast_slice(&self.mmap[desc.offset..desc.offset + desc.len]);
                let so = desc.scale_offset.ok_or_else(|| format!("Missing scale_offset for {name}"))?;
                let sl = desc.scale_len.ok_or_else(|| format!("Missing scale_len for {name}"))?;
                let scales: &[f32] = bytemuck::cast_slice(&self.mmap[so..so + sl]);
                if scales.len() != out_dim {
                    return Err(format!("Scale length mismatch for {name}"));
                }
                Ok(WeightMatrix::I8Row {
                    data,
                    scales,
                    out_dim,
                    in_dim,
                })
            }
            other => Err(format!("Unsupported dtype {other} for {name}")),
        }
    }
}
