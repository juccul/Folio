#!/usr/bin/env python3
"""Export trained PyTorch Math v2 & HTR v2 checkpoints into compact pure-Rust .frec bundles.

Format FREC0001:
  [0..8]      b"FREC0001"
  [8..12]     u32 LE header JSON length
  [12..12+N]  UTF-8 JSON metadata (kind, config, vocab, tensor descriptors)
  [aligned64] Raw tensor payloads:
                - dtype="f32":    little-endian float32 bytes
                - dtype="i8_row": int8 matrix (out_dim * in_dim) + float32 per-row scales (out_dim)
"""
from __future__ import annotations

import argparse
import gzip
import hashlib
import json
import struct
from pathlib import Path

import numpy as np
import torch

ALIGN = 64


def align_up(n: int, a: int = ALIGN) -> int:
    return (n + a - 1) // a * a


def export_frec(ckpt_path: Path, vocab: list[str], kind: str, out_path: Path):
    ckpt = torch.load(ckpt_path, map_location="cpu", weights_only=False)
    state: dict[str, torch.Tensor] = ckpt["model_state"]
    cfg: dict = dict(ckpt["config"])

    blobs: list[bytes] = []
    descriptors: dict[str, dict] = {}
    cursor = 0

    def append_bytes(data: bytes) -> tuple[int, int]:
        nonlocal cursor
        pad = align_up(cursor) - cursor
        if pad:
            blobs.append(b"\x00" * pad)
            cursor += pad
        off = cursor
        blobs.append(data)
        cursor += len(data)
        return off, len(data)

    for name, tensor in state.items():
        # Skip non-persistent buffers if any slipped in (RoPE / sinusoidal PE are recomputed in Rust)
        if name.endswith(".pe") or name.endswith(".cos") or name.endswith(".sin"):
            continue
        arr = tensor.detach().cpu().float().numpy()
        # Quantize large 2D Linear weights (except embeddings) per output row into symmetric int8
        quantize_i8 = (
            arr.ndim == 2
            and name.endswith(".weight")
            and "emb" not in name
            and arr.shape[0] * arr.shape[1] >= 4096
        )
        if quantize_i8:
            max_abs = np.max(np.abs(arr), axis=1)
            scales = np.maximum(max_abs / 127.0, 1e-8).astype("<f4")
            q = np.clip(np.round(arr / scales[:, None]), -127, 127).astype(np.int8)
            q_off, q_len = append_bytes(q.tobytes(order="C"))
            s_off, s_len = append_bytes(scales.tobytes(order="C"))
            descriptors[name] = {
                "dtype": "i8_row",
                "shape": list(arr.shape),
                "offset": q_off,
                "len": q_len,
                "scale_offset": s_off,
                "scale_len": s_len,
            }
        else:
            raw = arr.astype("<f4").tobytes(order="C")
            off, length = append_bytes(raw)
            descriptors[name] = {
                "dtype": "f32",
                "shape": list(arr.shape),
                "offset": off,
                "len": length,
            }

    header = {
        "kind": kind,
        "config": cfg,
        "vocab": vocab,
        "tensors": descriptors,
    }
    hdr_bytes = json.dumps(header, separators=(",", ":")).encode("utf-8")
    data_start = align_up(12 + len(hdr_bytes))
    pad_hdr = data_start - (12 + len(hdr_bytes))

    # Rebase offsets relative to file start so Rust can slice directly from mmap!
    for desc in header["tensors"].values():
        desc["offset"] += data_start
        if "scale_offset" in desc:
            desc["scale_offset"] += data_start
    # Re-serialize with rebased offsets (ensure header size stays <= data_start - 12; grow if needed)
    while True:
        hdr_bytes = json.dumps(header, separators=(",", ":")).encode("utf-8")
        new_start = align_up(12 + len(hdr_bytes))
        if new_start == data_start:
            pad_hdr = data_start - (12 + len(hdr_bytes))
            break
        delta = new_start - data_start
        for desc in header["tensors"].values():
            desc["offset"] += delta
            if "scale_offset" in desc:
                desc["scale_offset"] += delta
        data_start = new_start

    out_path.parent.mkdir(parents=True, exist_ok=True)
    with out_path.open("wb") as f:
        f.write(b"FREC0001")
        f.write(struct.pack("<I", len(hdr_bytes)))
        f.write(hdr_bytes)
        if pad_hdr:
            f.write(b"\x00" * pad_hdr)
        for chunk in blobs:
            f.write(chunk)

    mb = out_path.stat().st_size / (1024 * 1024)
    digest = hashlib.sha256(out_path.read_bytes()).hexdigest()[:16]
    print(f"[Export] {out_path} ({mb:.2f} MiB, sha256={digest}…, {len(descriptors)} tensors)", flush=True)


def export_lm_gz(lm_json: Path, out_path: Path):
    raw = json.loads(lm_json.read_text())
    compact = json.dumps(raw, separators=(",", ":")).encode("utf-8")
    out_path.parent.mkdir(parents=True, exist_ok=True)
    with gzip.open(out_path, "wb", compresslevel=9) as f:
        f.write(compact)
    mb = out_path.stat().st_size / (1024 * 1024)
    print(f"[Export] {out_path} ({mb:.2f} MiB gzipped LM)", flush=True)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--out-dir", type=Path, default=Path("recognition"))
    parser.add_argument("--math-ckpt", type=Path, default=Path("artifacts/stroke-math-experiment/run_v2_conformer/best.pt"))
    parser.add_argument("--math-vocab", type=Path, default=Path("artifacts/stroke-math-experiment/cache_v2/vocab.json"))
    parser.add_argument("--htr-ckpt", type=Path, default=Path("artifacts/stroke-htr-experiment/run_conformer_v2/best.pt"))
    parser.add_argument("--htr-vocab", type=Path, default=Path("artifacts/stroke-htr-experiment/cache_v2/vocab.json"))
    parser.add_argument("--htr-lm", type=Path, default=Path("artifacts/stroke-htr-experiment/cache/lm_v2.json"))
    args = parser.parse_args()

    args.out_dir.mkdir(parents=True, exist_ok=True)
    if args.math_ckpt.is_file():
        m_vocab = json.loads(args.math_vocab.read_text())["id_to_token"]
        export_frec(args.math_ckpt, m_vocab, "math_conformer_v2", args.out_dir / "math_v2.frec")
    if args.htr_ckpt.is_file():
        h_vocab = json.loads(args.htr_vocab.read_text())["id_to_char"]
        export_frec(args.htr_ckpt, h_vocab, "htr_conformer_v2", args.out_dir / "htr_v2.frec")
    if args.htr_lm.is_file():
        export_lm_gz(args.htr_lm, args.out_dir / "htr_lm.json.gz")

    pack = {
        "backend": "folio-stroke-v2",
        "math": "math_v2.frec",
        "htr": "htr_v2.frec",
        "lm": "htr_lm.json.gz",
    }
    (args.out_dir / "pack.json").write_text(json.dumps(pack, indent=2) + "\n")
    print(f"[Export] Wrote {args.out_dir / 'pack.json'}", flush=True)


if __name__ == "__main__":
    main()
