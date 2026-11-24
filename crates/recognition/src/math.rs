//! Math v2: 10-feature ink preprocessing + RoPE Conformer Encoder + KV-cached Decoder -> LaTeX.
use crate::{
    frec::FrecFile,
    ops::{RopeTable, conformer_block, conv1d, gelu, layer_norm, linear, softmax_inplace},
};

const FEATURE_DIM: usize = 10;
const MAX_POINTS: usize = 576;
const PAD_ID: usize = 0;
const BOS_ID: usize = 1;
const EOS_ID: usize = 2;
const UNK_ID: usize = 3;

pub struct MathModel {
    frec: FrecFile,
    rope: RopeTable,
    sin_pe: Vec<f32>,
}

fn resample_stroke(pts: &[[f32; 2]], step: f32) -> Vec<[f32; 2]> {
    if pts.len() <= 1 {
        let p = pts.first().copied().unwrap_or([0.0, 0.0]);
        return vec![p, p, p];
    }
    let mut seg_lens = Vec::with_capacity(pts.len() - 1);
    let mut total = 0.0_f32;
    for i in 0..pts.len() - 1 {
        let l = (pts[i + 1][0] - pts[i][0]).hypot(pts[i + 1][1] - pts[i][1]);
        seg_lens.push(l);
        total += l;
    }
    if total < 1e-5 {
        return vec![pts[0], pts[0], pts[0]];
    }
    let num_out = ((total / step).ceil() as usize + 1).max(3);
    let mut cum_k = Vec::with_capacity(pts.len());
    let mut pts_k = Vec::with_capacity(pts.len());
    cum_k.push(0.0_f32);
    pts_k.push(pts[0]);
    let mut acc = 0.0_f32;
    for (i, &l) in seg_lens.iter().enumerate() {
        acc += l;
        if l > 1e-7 {
            cum_k.push(acc);
            pts_k.push(pts[i + 1]);
        }
    }
    if pts_k.len() < 2 {
        return vec![pts[0], pts[0], pts[0]];
    }
    let last_c = *cum_k.last().unwrap();
    let mut out = Vec::with_capacity(num_out);
    let mut j = 0usize;
    for i in 0..num_out {
        let target = if num_out == 1 {
            0.0
        } else {
            last_c * (i as f32) / ((num_out - 1) as f32)
        };
        while j + 1 < cum_k.len() - 1 && cum_k[j + 1] < target {
            j += 1;
        }
        let c0 = cum_k[j];
        let c1 = cum_k[j + 1];
        let alpha = if c1 > c0 { ((target - c0) / (c1 - c0)).clamp(0.0, 1.0) } else { 0.0 };
        out.push([
            pts_k[j][0] + alpha * (pts_k[j + 1][0] - pts_k[j][0]),
            pts_k[j][1] + alpha * (pts_k[j + 1][1] - pts_k[j][1]),
        ]);
    }
    out
}

pub fn strokes_to_features(strokes: &[Vec<[f32; 2]>]) -> (Vec<f32>, usize) {
    let valid: Vec<&Vec<[f32; 2]>> = strokes.iter().filter(|s| !s.is_empty()).collect();
    if valid.is_empty() {
        return (vec![0.0; 4 * FEATURE_DIM], 4);
    }
    let (mut x0, mut y0, mut x1, mut y1) = (f32::INFINITY, f32::INFINITY, f32::NEG_INFINITY, f32::NEG_INFINITY);
    for s in &valid {
        for &[x, y] in s.iter() {
            if x.is_finite() && y.is_finite() {
                x0 = x0.min(x);
                y0 = y0.min(y);
                x1 = x1.max(x);
                y1 = y1.max(y);
            }
        }
    }
    if !x0.is_finite() {
        return (vec![0.0; 4 * FEATURE_DIM], 4);
    }
    let w = (x1 - x0).max(1e-4);
    let h = (y1 - y0).max(1e-4);
    let cx = 0.5 * (x0 + x1);
    let cy = 0.5 * (y0 + y1);
    let scale = h.max(w * 0.16).max(1e-4);

    let norm_strokes: Vec<Vec<[f32; 2]>> = valid
        .iter()
        .map(|s| {
            s.iter()
                .filter(|p| p[0].is_finite() && p[1].is_finite())
                .map(|&[x, y]| [(x - cx) / scale, (y - cy) / scale])
                .collect()
        })
        .filter(|s: &Vec<[f32; 2]>| !s.is_empty())
        .collect();

    let mut total_len = 0.0_f32;
    for s in &norm_strokes {
        for i in 0..s.len().saturating_sub(1) {
            total_len += (s[i + 1][0] - s[i][0]).hypot(s[i + 1][1] - s[i][1]);
        }
    }
    let budget = (MAX_POINTS as isize - 3 * norm_strokes.len() as isize).max(48) as f32;
    let base_step = 0.05_f32;
    let step = if total_len > 0.0 {
        base_step.max(total_len / budget)
    } else {
        base_step
    };
    let mut resampled: Vec<Vec<[f32; 2]>> = norm_strokes.iter().map(|s| resample_stroke(s, step)).collect();
    let mut total_pts: usize = resampled.iter().map(Vec::len).sum();
    if total_pts > MAX_POINTS {
        let step2 = step * (total_pts as f32 / (MAX_POINTS as f32 * 0.90));
        resampled = norm_strokes.iter().map(|s| resample_stroke(s, step2)).collect();
        total_pts = resampled.iter().map(Vec::len).sum();
    }

    let t_raw = total_pts.min(MAX_POINTS).max(4);
    let t_pad = ((t_raw + 3) / 4) * 4;
    let mut feats = vec![0.0_f32; t_pad * FEATURE_DIM];
    let n_strokes = resampled.len();
    let denom = (n_strokes.saturating_sub(1)).max(1) as f32;
    let mut pos = 0usize;
    let mut prev_xy: Option<[f32; 2]> = None;

    for (si, s) in resampled.iter().enumerate() {
        if pos >= MAX_POINTS {
            break;
        }
        let k = s.len().min(MAX_POINTS - pos);
        let s_pos_val = si as f32 / denom;
        for i in 0..k {
            let row = &mut feats[(pos + i) * FEATURE_DIM..(pos + i + 1) * FEATURE_DIM];
            row[0] = s[i][0].clamp(-10.0, 10.0);
            row[1] = s[i][1].clamp(-5.0, 5.0);
            if i == 0 {
                if let Some(prev) = prev_xy {
                    let jdx = (s[0][0] - prev[0]).clamp(-5.0, 5.0);
                    let jdy = (s[0][1] - prev[1]).clamp(-5.0, 5.0);
                    row[2] = jdx;
                    row[3] = jdy;
                    row[6] = jdx.hypot(jdy).ln_1p();
                }
                row[8] = 1.0;
            } else {
                let dx = (s[i][0] - s[i - 1][0]).clamp(-3.0, 3.0);
                let dy = (s[i][1] - s[i - 1][1]).clamp(-3.0, 3.0);
                let norm = dx.hypot(dy);
                let safe = norm.max(1e-5);
                row[2] = dx * 4.0;
                row[3] = dy * 4.0;
                row[4] = dx / safe;
                row[5] = dy / safe;
                row[6] = (norm * 4.0).ln_1p();
            }
            row[7] = if i + 1 == k { 0.0 } else { 1.0 };
            row[9] = s_pos_val;
        }
        prev_xy = Some(s[k - 1]);
        pos += k;
    }
    (feats, t_pad)
}

fn detokenize_latex(tokens: &[&str]) -> String {
    let mut out = String::new();
    let mut prev_cmd_alpha = false;
    for &tok in tokens {
        if prev_cmd_alpha && tok.chars().next().is_some_and(|c| c.is_ascii_alphabetic()) {
            out.push(' ');
        }
        out.push_str(tok);
        prev_cmd_alpha = tok.starts_with('\\')
            && tok.len() > 1
            && tok[1..].bytes().all(|b| b.is_ascii_alphabetic());
    }
    out
}

fn build_sinusoidal_pe(max_len: usize, d_model: usize) -> Vec<f32> {
    let mut pe = vec![0.0_f32; max_len * d_model];
    let half = d_model / 2;
    let mut div = vec![0.0_f32; half];
    for i in 0..half {
        div[i] = (-((2 * i) as f32) * (10000.0_f32.ln()) / d_model as f32).exp();
    }
    for pos in 0..max_len {
        let p = pos as f32;
        for i in 0..half {
            let ang = p * div[i];
            pe[pos * d_model + 2 * i] = ang.sin() * 0.25;
            pe[pos * d_model + 2 * i + 1] = ang.cos() * 0.25;
        }
    }
    pe
}

impl MathModel {
    pub fn load(frec: FrecFile) -> Result<Self, String> {
        if frec.header.kind != "math_conformer_v2" {
            return Err(format!("Expected math_conformer_v2 model, got {}", frec.header.kind));
        }
        let d = frec.header.config.d_model;
        let hd = d / frec.header.config.nhead;
        let rope = RopeTable::new(hd, 2048);
        let sin_pe = build_sinusoidal_pe(1024, d);
        Ok(Self { frec, rope, sin_pe })
    }

    pub fn recognize(&self, strokes: &[Vec<[f32; 2]>], max_len: usize) -> Result<String, String> {
        let cfg = &self.frec.header.config;
        let d = cfg.d_model;
        let nhead = cfg.nhead;
        let hd = d / nhead;
        let dim_ff = cfg.dim_ff;
        let vocab_size = cfg.vocab_size;

        let (feats, t_in) = strokes_to_features(strokes);

        // 4x Conv1d stem
        let (w0, _) = self.frec.f32_slice("stem.0.weight")?;
        let (b0, _) = self.frec.f32_slice("stem.0.bias")?;
        let (h0, t1) = conv1d(&feats, t_in, cfg.in_features, w0, b0, d / 2, 5, 2, 2, true);
        let (w2, _) = self.frec.f32_slice("stem.2.weight")?;
        let (b2, _) = self.frec.f32_slice("stem.2.bias")?;
        let (h1, s_len) = conv1d(&h0, t1, d / 2, w2, b2, d, 5, 2, 2, true);

        let mut memory = vec![0.0_f32; s_len * d];
        let (in_w, _) = self.frec.f32_slice("enc_in_norm.weight")?;
        let (in_b, _) = self.frec.f32_slice("enc_in_norm.bias")?;
        layer_norm(&h1, s_len, d, in_w, in_b, &mut memory);

        for l in 0..cfg.num_encoder_layers {
            conformer_block(
                &self.frec,
                &format!("encoder_layers.{l}"),
                false,
                &mut memory,
                s_len,
                d,
                nhead,
                dim_ff,
                cfg.conv_kernel,
                &self.rope,
            )?;
        }

        let (out_w, _) = self.frec.f32_slice("enc_out_norm.weight")?;
        let (out_b, _) = self.frec.f32_slice("enc_out_norm.bias")?;
        let mut mem_normed = vec![0.0_f32; s_len * d];
        layer_norm(&memory, s_len, d, out_w, out_b, &mut mem_normed);

        // Precompute cross-attention K/V once per decoder layer: [nhead, s_len, hd]
        let n_dec = cfg.num_decoder_layers;
        let mut cross_ks: Vec<Vec<f32>> = Vec::with_capacity(n_dec);
        let mut cross_vs: Vec<Vec<f32>> = Vec::with_capacity(n_dec);
        let mut ckv_buf = vec![0.0_f32; s_len * 2 * d];
        for l in 0..n_dec {
            let w = self.frec.linear(&format!("decoder_layers.{l}.cross_kv.weight"))?;
            let b = self.frec.opt_f32(&format!("decoder_layers.{l}.cross_kv.bias"));
            linear(&mem_normed, s_len, &w, b, &mut ckv_buf);
            let mut ck = vec![0.0_f32; nhead * s_len * hd];
            let mut cv = vec![0.0_f32; nhead * s_len * hd];
            for pos in 0..s_len {
                let base = pos * 2 * d;
                for h in 0..nhead {
                    let dst = (h * s_len + pos) * hd;
                    ck[dst..dst + hd].copy_from_slice(&ckv_buf[base + h * hd..base + (h + 1) * hd]);
                    cv[dst..dst + hd].copy_from_slice(&ckv_buf[base + d + h * hd..base + d + (h + 1) * hd]);
                }
            }
            cross_ks.push(ck);
            cross_vs.push(cv);
        }

        // Autoregressive KV-cached beam search decoding (width = 3)
        let (tok_emb, _) = self.frec.f32_slice("tok_emb.weight")?;
        let (dec_out_w, _) = self.frec.f32_slice("dec_out_norm.weight")?;
        let (dec_out_b, _) = self.frec.f32_slice("dec_out_norm.bias")?;
        let lm_head_w = self.frec.linear("lm_head.weight")?;

        // Pre-resolve decoder layer weights once outside the decoding loop.
        struct DecLayerWeights<'a> {
            norm1_w: &'a [f32],
            norm1_b: &'a [f32],
            self_qkv_w: crate::frec::WeightMatrix<'a>,
            self_qkv_b: Option<&'a [f32]>,
            self_proj_w: crate::frec::WeightMatrix<'a>,
            self_proj_b: Option<&'a [f32]>,
            norm2_w: &'a [f32],
            norm2_b: &'a [f32],
            cross_q_w: crate::frec::WeightMatrix<'a>,
            cross_q_b: Option<&'a [f32]>,
            cross_proj_w: crate::frec::WeightMatrix<'a>,
            cross_proj_b: Option<&'a [f32]>,
            norm3_w: &'a [f32],
            norm3_b: &'a [f32],
            ff1_w: crate::frec::WeightMatrix<'a>,
            ff1_b: Option<&'a [f32]>,
            ff2_w: crate::frec::WeightMatrix<'a>,
            ff2_b: Option<&'a [f32]>,
        }
        let mut dec_layers = Vec::with_capacity(n_dec);
        for l in 0..n_dec {
            let pfx = format!("decoder_layers.{l}");
            let (norm1_w, _) = self.frec.f32_slice(&format!("{pfx}.norm1.weight"))?;
            let (norm1_b, _) = self.frec.f32_slice(&format!("{pfx}.norm1.bias"))?;
            let (norm2_w, _) = self.frec.f32_slice(&format!("{pfx}.norm2.weight"))?;
            let (norm2_b, _) = self.frec.f32_slice(&format!("{pfx}.norm2.bias"))?;
            let (norm3_w, _) = self.frec.f32_slice(&format!("{pfx}.norm3.weight"))?;
            let (norm3_b, _) = self.frec.f32_slice(&format!("{pfx}.norm3.bias"))?;
            dec_layers.push(DecLayerWeights {
                norm1_w,
                norm1_b,
                self_qkv_w: self.frec.linear(&format!("{pfx}.self_qkv.weight"))?,
                self_qkv_b: self.frec.opt_f32(&format!("{pfx}.self_qkv.bias")),
                self_proj_w: self.frec.linear(&format!("{pfx}.self_proj.weight"))?,
                self_proj_b: self.frec.opt_f32(&format!("{pfx}.self_proj.bias")),
                norm2_w,
                norm2_b,
                cross_q_w: self.frec.linear(&format!("{pfx}.cross_q.weight"))?,
                cross_q_b: self.frec.opt_f32(&format!("{pfx}.cross_q.bias")),
                cross_proj_w: self.frec.linear(&format!("{pfx}.cross_proj.weight"))?,
                cross_proj_b: self.frec.opt_f32(&format!("{pfx}.cross_proj.bias")),
                norm3_w,
                norm3_b,
                ff1_w: self.frec.linear(&format!("{pfx}.ff.0.weight"))?,
                ff1_b: self.frec.opt_f32(&format!("{pfx}.ff.0.bias")),
                ff2_w: self.frec.linear(&format!("{pfx}.ff.3.weight"))?,
                ff2_b: self.frec.opt_f32(&format!("{pfx}.ff.3.bias")),
            });
        }

        const BEAM_WIDTH: usize = 3;
        #[derive(Clone)]
        struct Beam {
            tokens: Vec<usize>,
            cur_tok: usize,
            score: f32,
            finished: bool,
            // [layer][head][step * hd]
            self_k: Vec<Vec<Vec<f32>>>,
            self_v: Vec<Vec<Vec<f32>>>,
        }

        let mut beams = vec![Beam {
            tokens: Vec::with_capacity(64),
            cur_tok: BOS_ID,
            score: 0.0,
            finished: false,
            self_k: (0..n_dec)
                .map(|_| (0..nhead).map(|_| Vec::with_capacity(32 * hd)).collect())
                .collect(),
            self_v: (0..n_dec)
                .map(|_| (0..nhead).map(|_| Vec::with_capacity(32 * hd)).collect())
                .collect(),
        }];

        let scale = 1.0 / (hd as f32).sqrt();
        let mut x = vec![0.0_f32; d];
        let mut norm_buf = vec![0.0_f32; d];
        let mut qkv = vec![0.0_f32; 3 * d];
        let mut cq = vec![0.0_f32; d];
        let mut attn_cat = vec![0.0_f32; d];
        let mut proj_buf = vec![0.0_f32; d];
        let mut ff_mid = vec![0.0_f32; dim_ff];
        let mut logits = vec![0.0_f32; vocab_size];
        let mut scores = Vec::with_capacity(s_len.max(max_len));

        for step in 0..max_len {
            if beams.iter().all(|b| b.finished) {
                break;
            }
            // Candidates: (beam_idx, next_tok, new_score)
            let active_beams = beams.len();
            let mut candidates: Vec<(usize, usize, f32)> = Vec::with_capacity(active_beams * (BEAM_WIDTH + 1));

            for (bi, beam) in beams.iter_mut().enumerate() {
                if beam.finished {
                    candidates.push((bi, EOS_ID, beam.score));
                    continue;
                }
                let emb_row = &tok_emb[beam.cur_tok * d..(beam.cur_tok + 1) * d];
                let pe_row = &self.sin_pe[step * d..(step + 1) * d];
                for i in 0..d {
                    x[i] = emb_row[i] + pe_row[i];
                }

                let past_len = step + 1;
                for (l, lw) in dec_layers.iter().enumerate() {
                    // 1. Causal self-attention with KV cache
                    layer_norm(&x, 1, d, lw.norm1_w, lw.norm1_b, &mut norm_buf);
                    linear(&norm_buf, 1, &lw.self_qkv_w, lw.self_qkv_b, &mut qkv);

                    scores.resize(past_len, 0.0);
                    for h in 0..nhead {
                        let qh = &qkv[h * hd..(h + 1) * hd];
                        let kh_new = &qkv[d + h * hd..d + (h + 1) * hd];
                        let vh_new = &qkv[2 * d + h * hd..2 * d + (h + 1) * hd];
                        beam.self_k[l][h].extend_from_slice(kh_new);
                        beam.self_v[l][h].extend_from_slice(vh_new);
                        let kh_all = &beam.self_k[l][h];
                        let vh_all = &beam.self_v[l][h];
                        for p in 0..past_len {
                            let kp = &kh_all[p * hd..(p + 1) * hd];
                            let mut dot = 0.0_f32;
                            for k in 0..hd {
                                dot += qh[k] * kp[k];
                            }
                            scores[p] = dot * scale;
                        }
                        softmax_inplace(&mut scores[..past_len]);
                        let oh = &mut attn_cat[h * hd..(h + 1) * hd];
                        oh.fill(0.0);
                        for p in 0..past_len {
                            let wp = scores[p];
                            let vp = &vh_all[p * hd..(p + 1) * hd];
                            for k in 0..hd {
                                oh[k] += wp * vp[k];
                            }
                        }
                    }
                    linear(&attn_cat, 1, &lw.self_proj_w, lw.self_proj_b, &mut proj_buf);
                    for i in 0..d {
                        x[i] += proj_buf[i];
                    }

                    // 2. Cross-attention against precomputed K/V
                    layer_norm(&x, 1, d, lw.norm2_w, lw.norm2_b, &mut norm_buf);
                    linear(&norm_buf, 1, &lw.cross_q_w, lw.cross_q_b, &mut cq);

                    scores.resize(s_len, 0.0);
                    for h in 0..nhead {
                        let qh = &cq[h * hd..(h + 1) * hd];
                        let ck = &cross_ks[l][h * s_len * hd..(h + 1) * s_len * hd];
                        let cv = &cross_vs[l][h * s_len * hd..(h + 1) * s_len * hd];
                        for p in 0..s_len {
                            let kp = &ck[p * hd..(p + 1) * hd];
                            let mut dot = 0.0_f32;
                            for k in 0..hd {
                                dot += qh[k] * kp[k];
                            }
                            scores[p] = dot * scale;
                        }
                        softmax_inplace(&mut scores[..s_len]);
                        let oh = &mut attn_cat[h * hd..(h + 1) * hd];
                        oh.fill(0.0);
                        for p in 0..s_len {
                            let wp = scores[p];
                            let vp = &cv[p * hd..(p + 1) * hd];
                            for k in 0..hd {
                                oh[k] += wp * vp[k];
                            }
                        }
                    }
                    linear(&attn_cat, 1, &lw.cross_proj_w, lw.cross_proj_b, &mut proj_buf);
                    for i in 0..d {
                        x[i] += proj_buf[i];
                    }

                    // 3. FFN (GELU)
                    layer_norm(&x, 1, d, lw.norm3_w, lw.norm3_b, &mut norm_buf);
                    linear(&norm_buf, 1, &lw.ff1_w, lw.ff1_b, &mut ff_mid);
                    for v in &mut ff_mid {
                        *v = gelu(*v);
                    }
                    linear(&ff_mid, 1, &lw.ff2_w, lw.ff2_b, &mut proj_buf);
                    for i in 0..d {
                        x[i] += proj_buf[i];
                    }
                }

                layer_norm(&x, 1, d, dec_out_w, dec_out_b, &mut norm_buf);
                linear(&norm_buf, 1, &lm_head_w, None, &mut logits);

                // Log-softmax + top-BEAM_WIDTH extraction
                let mut max_v = f32::NEG_INFINITY;
                for &v in &logits {
                    if v > max_v {
                        max_v = v;
                    }
                }
                let mut sum_exp = 0.0_f32;
                for &v in &logits {
                    sum_exp += (v - max_v).exp();
                }
                let log_z = max_v + sum_exp.max(1e-12).ln();

                // Pick top BEAM_WIDTH tokens (skipping PAD/BOS/UNK so `<unk>` is never emitted)
                let mut top = [(0usize, f32::NEG_INFINITY); BEAM_WIDTH];
                for (tid, &val) in logits.iter().enumerate() {
                    if tid == PAD_ID || tid == BOS_ID || tid == UNK_ID {
                        continue;
                    }
                    let lp = val - log_z;
                    if lp > top[BEAM_WIDTH - 1].1 {
                        top[BEAM_WIDTH - 1] = (tid, lp);
                        let mut k = BEAM_WIDTH - 1;
                        while k > 0 && top[k].1 > top[k - 1].1 {
                            top.swap(k, k - 1);
                            k -= 1;
                        }
                    }
                }
                // Confidence-gated fast path: if top-1 has >97% probability and there is only 1 active beam,
                // avoid unnecessary beam branching on completely unambiguous tokens.
                let expand = if active_beams == 1 && top[0].1 > -0.03 {
                    1
                } else {
                    BEAM_WIDTH
                };
                for &(tid, lp) in &top[..expand] {
                    if lp.is_finite() {
                        candidates.push((bi, tid, beam.score + lp));
                    }
                }
            }

            candidates.sort_by(|a, b| b.2.total_cmp(&a.2));
            candidates.truncate(BEAM_WIDTH);

            let mut next_beams = Vec::with_capacity(candidates.len());
            for (bi, tid, sc) in candidates {
                let mut nb = beams[bi].clone();
                nb.score = sc;
                if !nb.finished {
                    if tid == EOS_ID {
                        nb.finished = true;
                    } else {
                        nb.tokens.push(tid);
                        nb.cur_tok = tid;
                    }
                }
                next_beams.push(nb);
            }
            beams = next_beams;
        }

        let best = beams
            .into_iter()
            .max_by(|a, b| {
                let sa = a.score / (((a.tokens.len() + 1) as f32).powf(0.6));
                let sb = b.score / (((b.tokens.len() + 1) as f32).powf(0.6));
                sa.total_cmp(&sb)
            })
            .unwrap();
        let out_ids = best.tokens;

        let vocab = &self.frec.header.vocab;
        let toks: Vec<&str> = out_ids
            .iter()
            .filter(|&&id| id != PAD_ID && id != BOS_ID && id != EOS_ID && id < vocab.len())
            .map(|&id| vocab[id].as_str())
            .collect();
        Ok(detokenize_latex(&toks))
    }
}
