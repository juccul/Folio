//! Handwriting v2: multi-line vector segmentation + 8-feature RoPE Conformer-CTC + Bigram LM beam search.
use crate::{
    frec::FrecFile,
    ops::{RopeTable, conformer_block, conv1d, layer_norm, linear},
};
use flate2::read::GzDecoder;
use serde::Deserialize;
use std::{collections::HashMap, fs::File, io::BufReader, path::Path};

const FEATURE_DIM: usize = 8;
const MAX_POINTS: usize = 1024;

#[derive(Deserialize)]
struct LmStatsRaw {
    total_words: u64,
    words: HashMap<String, u32>,
    words_lower: HashMap<String, u32>,
    #[serde(default)]
    bigrams: HashMap<String, u32>,
    #[serde(default)]
    char4: HashMap<String, u32>,
    #[serde(default)]
    char3: HashMap<String, u32>,
}

pub struct BigramLexiconLm {
    word_lp: HashMap<String, f32>,
    lower_lp: HashMap<String, f32>,
    bigram_lp: HashMap<String, f32>,
    char4: HashMap<String, u32>,
    char3: HashMap<String, u32>,
}

impl BigramLexiconLm {
    pub fn load_gz(path: &Path) -> Result<Self, String> {
        let file = File::open(path).map_err(|e| format!("Cannot open LM {}: {e}", path.display()))?;
        let reader = BufReader::new(GzDecoder::new(file));
        let raw: LmStatsRaw =
            serde_json::from_reader(reader).map_err(|e| format!("Invalid LM {}: {e}", path.display()))?;
        let log_tot = ((raw.total_words.max(1) as f64) + 20000.0).ln() as f32;
        let word_lp: HashMap<String, f32> = raw
            .words
            .into_iter()
            .map(|(w, c)| (w, ((c as f32) + 1.0).ln() - log_tot))
            .collect();
        let lower_counts = raw.words_lower;
        let lower_lp: HashMap<String, f32> = lower_counts
            .iter()
            .map(|(w, &c)| (w.clone(), ((c as f32) + 1.0).ln() - log_tot))
            .collect();
        let bigram_lp: HashMap<String, f32> = raw
            .bigrams
            .into_iter()
            .map(|(bg, c)| {
                let prev_w = bg.split(' ').next().unwrap_or("");
                let denom = (lower_counts.get(prev_w).copied().unwrap_or(50) as f32) + 100.0;
                (bg, (((c as f32) + 0.5) / denom).ln())
            })
            .collect();
        Ok(Self {
            word_lp,
            lower_lp,
            bigram_lp,
            char4: raw.char4,
            char3: raw.char3,
        })
    }

    fn strip_word_core(word: &str) -> Option<&str> {
        let trimmed = word.trim_matches(|c: char| !c.is_ascii_alphabetic());
        if trimmed.is_empty() {
            return None;
        }
        if trimmed
            .bytes()
            .all(|b| b.is_ascii_alphabetic() || b == b'\'')
        {
            Some(trimmed)
        } else {
            None
        }
    }

    pub fn score_word(&self, word: &str, prev_word: Option<&str>) -> f32 {
        let Some(core) = Self::strip_word_core(word) else {
            return 0.0;
        };
        let cl = core.to_ascii_lowercase();
        let uni = if let Some(&lp) = self.word_lp.get(core) {
            1.65 + 0.14 * (lp + 12.0)
        } else if let Some(&lp) = self.lower_lp.get(&cl) {
            1.25 + 0.12 * (lp + 12.0)
        } else if cl.len() <= 2 {
            -0.8
        } else {
            let padded = format!("^{cl}$");
            let bytes = padded.as_bytes();
            let ng = (bytes.len().saturating_sub(3)).max(1);
            let mut gs = 0.0_f32;
            for i in 0..bytes.len().saturating_sub(3) {
                let g4 = &padded[i..i + 4];
                if let Some(&c4) = self.char4.get(g4) {
                    let c3 = self.char3.get(&g4[..3]).copied().unwrap_or(c4);
                    gs += ((c4 as f32 + 0.5) / (c3 as f32 + 5.0)).ln();
                } else {
                    gs += -4.5;
                }
            }
            ((gs / ng as f32) * 0.45).max(-2.2)
        };

        let prev_key = prev_word
            .and_then(Self::strip_word_core)
            .map(|s| s.to_ascii_lowercase())
            .unwrap_or_else(|| "<s>".to_string());
        let bg_key = format!("{prev_key} {cl}");
        let bi = self
            .bigram_lp
            .get(&bg_key)
            .map_or(0.0, |&lp| ((lp + 7.5) * 0.28).max(0.0));
        uni + bi
    }
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

fn reorder_delayed_strokes(strokes: Vec<Vec<[f32; 2]>>) -> Vec<Vec<[f32; 2]>> {
    if strokes.len() <= 1 {
        return strokes;
    }
    let mut out: Vec<(f32, f32, Vec<[f32; 2]>)> = Vec::with_capacity(strokes.len());
    for s in strokes {
        let (mut x0, mut x1) = (f32::INFINITY, f32::NEG_INFINITY);
        for &[x, _] in &s {
            x0 = x0.min(x);
            x1 = x1.max(x);
        }
        let mut insert_at = out.len();
        for i in (0..out.len()).rev() {
            let (prev_x0, prev_x1, _) = out[i];
            if x1 < prev_x0 {
                insert_at = i;
            } else if x0 > prev_x1 {
                break;
            }
        }
        out.insert(insert_at, (x0, x1, s));
    }
    out.into_iter().map(|(_, _, s)| s).collect()
}

/// Cluster a page or selection of strokes into top-to-bottom horizontal text lines.
///
/// Uses a two-pass scale-invariant algorithm:
/// 1. Tall/body strokes seed horizontal line bands (preventing high punctuation like
///    quotes `"`, apostrophes `'`, or `i`-dots from spawning spurious lines above text).
/// 2. Small marks attach to their nearest line band, preferring bands they horizontally overlap.
pub fn segment_lines(strokes: &[Vec<[f32; 2]>]) -> Vec<Vec<Vec<[f32; 2]>>> {
    let valid: Vec<Vec<[f32; 2]>> = strokes
        .iter()
        .map(|s| {
            s.iter()
                .copied()
                .filter(|p| p[0].is_finite() && p[1].is_finite())
                .collect::<Vec<_>>()
        })
        .filter(|s| !s.is_empty())
        .collect();
    if valid.is_empty() {
        return Vec::new();
    }
    if valid.len() == 1 {
        return vec![valid];
    }

    struct StrokeInfo {
        yc: f32,
        y0: f32,
        y1: f32,
        x0: f32,
        x1: f32,
        idx: usize,
        pts: Vec<[f32; 2]>,
    }

    let infos: Vec<StrokeInfo> = valid
        .into_iter()
        .enumerate()
        .map(|(idx, pts)| {
            let (mut x0, mut y0, mut x1, mut y1) =
                (f32::INFINITY, f32::INFINITY, f32::NEG_INFINITY, f32::NEG_INFINITY);
            for &[x, y] in &pts {
                x0 = x0.min(x);
                y0 = y0.min(y);
                x1 = x1.max(x);
                y1 = y1.max(y);
            }
            StrokeInfo {
                yc: 0.5 * (y0 + y1),
                y0,
                y1,
                x0,
                x1,
                idx,
                pts,
            }
        })
        .collect();

    let max_h = infos
        .iter()
        .map(|s| s.y1 - s.y0)
        .fold(0.0_f32, f32::max);
    let mut non_tiny: Vec<f32> = infos
        .iter()
        .map(|s| s.y1 - s.y0)
        .filter(|&h| h > (max_h * 0.15).max(1e-4))
        .collect();
    non_tiny.sort_by(f32::total_cmp);
    let med_h = if non_tiny.is_empty() {
        max_h.max(1e-3)
    } else {
        non_tiny[non_tiny.len() / 2].max(1e-3)
    };
    let merge_tol = med_h * 0.45;

    let mut body_strokes: Vec<StrokeInfo> = Vec::new();
    let mut small_strokes: Vec<StrokeInfo> = Vec::new();
    for s in infos {
        let h = s.y1 - s.y0;
        let w = s.x1 - s.x0;
        if h >= med_h * 0.60 || w >= med_h * 1.2 {
            body_strokes.push(s);
        } else {
            small_strokes.push(s);
        }
    }
    if body_strokes.is_empty() {
        body_strokes = small_strokes;
        small_strokes = Vec::new();
    }
    body_strokes.sort_by(|a, b| a.yc.total_cmp(&b.yc));

    struct LineBand {
        y0: f32,
        y1: f32,
        x0: f32,
        x1: f32,
        yc_sum: f32,
        count: usize,
        items: Vec<(usize, Vec<[f32; 2]>)>,
    }

    // Pass 1: cluster body strokes top-to-bottom into line bands.
    let mut lines: Vec<LineBand> = Vec::new();
    for s in body_strokes {
        let mut best_idx = None;
        let mut best_dist = f32::INFINITY;
        for (li, ln) in lines.iter().enumerate() {
            let overlap = s.y1.min(ln.y1) - s.y0.max(ln.y0);
            if overlap >= -merge_tol {
                let dist = (s.yc - ln.yc_sum / ln.count as f32).abs();
                let max_gap = ((ln.y1 - ln.y0) * 0.65).max(med_h * 0.85);
                if dist <= max_gap && dist < best_dist {
                    best_dist = dist;
                    best_idx = Some(li);
                }
            }
        }
        if let Some(li) = best_idx {
            let ln = &mut lines[li];
            ln.y0 = ln.y0.min(s.y0);
            ln.y1 = ln.y1.max(s.y1);
            ln.x0 = ln.x0.min(s.x0);
            ln.x1 = ln.x1.max(s.x1);
            ln.yc_sum += s.yc;
            ln.count += 1;
            ln.items.push((s.idx, s.pts));
        } else {
            lines.push(LineBand {
                y0: s.y0,
                y1: s.y1,
                x0: s.x0,
                x1: s.x1,
                yc_sum: s.yc,
                count: 1,
                items: vec![(s.idx, s.pts)],
            });
        }
    }

    // Pass 2: attach small marks (dots, quotes, apostrophes, commas, crossbars) to nearest line.
    for s in small_strokes {
        let mut best_idx = None;
        let mut best_score = f32::INFINITY;
        for (li, ln) in lines.iter().enumerate() {
            let v_gap = if s.y1 < ln.y0 {
                ln.y0 - s.y1
            } else if s.y0 > ln.y1 {
                s.y0 - ln.y1
            } else {
                0.0
            };
            let v_center = (s.yc - ln.yc_sum / ln.count as f32).abs();
            let h_gap = if s.x1 < ln.x0 {
                ln.x0 - s.x1
            } else if s.x0 > ln.x1 {
                s.x0 - ln.x1
            } else {
                0.0
            };
            let max_v_gap = ((ln.y1 - ln.y0) * 1.0).max(med_h * 1.8);
            if v_gap <= max_v_gap {
                let score = v_gap * 2.0 + v_center * 0.5 + h_gap * 0.15;
                if score < best_score {
                    best_score = score;
                    best_idx = Some(li);
                }
            }
        }
        if let Some(li) = best_idx {
            let ln = &mut lines[li];
            ln.x0 = ln.x0.min(s.x0);
            ln.x1 = ln.x1.max(s.x1);
            ln.items.push((s.idx, s.pts));
        } else {
            lines.push(LineBand {
                y0: s.y0,
                y1: s.y1,
                x0: s.x0,
                x1: s.x1,
                yc_sum: s.yc,
                count: 1,
                items: vec![(s.idx, s.pts)],
            });
        }
    }

    lines.sort_by(|a, b| (a.yc_sum / a.count as f32).total_cmp(&(b.yc_sum / b.count as f32)));
    lines
        .into_iter()
        .map(|mut ln| {
            ln.items.sort_by_key(|(idx, _)| *idx);
            let ordered = ln.items.into_iter().map(|(_, s)| s).collect();
            reorder_delayed_strokes(ordered)
        })
        .collect()
}

fn percentile_sorted(sorted: &[f32], q: f32) -> f32 {
    if sorted.is_empty() {
        return 0.0;
    }
    let idx = q * (sorted.len() - 1) as f32;
    let lo = idx.floor() as usize;
    let hi = (lo + 1).min(sorted.len() - 1);
    let frac = idx - lo as f32;
    sorted[lo] + frac * (sorted[hi] - sorted[lo])
}

pub fn line_strokes_to_features(strokes: &[Vec<[f32; 2]>]) -> (Vec<f32>, usize, usize) {
    let valid: Vec<Vec<[f32; 2]>> = strokes.iter().filter(|s| !s.is_empty()).cloned().collect();
    if valid.is_empty() {
        return (vec![0.0; 4 * FEATURE_DIM], 4, 4);
    }
    let ordered = reorder_delayed_strokes(valid);
    let mut ys: Vec<f32> = Vec::new();
    let mut x_origin = f32::INFINITY;
    for s in &ordered {
        for &[x, y] in s {
            x_origin = x_origin.min(x);
            ys.push(y);
        }
    }
    let mean_y = ys.iter().sum::<f32>() / ys.len() as f32;
    let var_y = ys.iter().map(|&y| (y - mean_y) * (y - mean_y)).sum::<f32>() / ys.len() as f32;
    let std_y = var_y.sqrt();
    ys.sort_by(f32::total_cmp);
    let med_y = percentile_sorted(&ys, 0.5);
    let q05 = percentile_sorted(&ys, 0.05);
    let q95 = percentile_sorted(&ys, 0.95);
    let scale = (2.2 * std_y).max((q95 - q05) * 0.65).max(1e-3);

    let norm_strokes: Vec<Vec<[f32; 2]>> = ordered
        .iter()
        .map(|s| s.iter().map(|&[x, y]| [(x - x_origin) / scale, (y - med_y) / scale]).collect())
        .collect();

    let mut total_len = 0.0_f32;
    for s in &norm_strokes {
        for i in 0..s.len().saturating_sub(1) {
            total_len += (s[i + 1][0] - s[i][0]).hypot(s[i + 1][1] - s[i][1]);
        }
    }
    let budget = (MAX_POINTS as isize - 3 * norm_strokes.len() as isize).max(64) as f32;
    let base_step = 0.08_f32;
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
    let mut pos = 0usize;
    let mut prev_xy: Option<[f32; 2]> = None;

    for s in &resampled {
        if pos >= MAX_POINTS {
            break;
        }
        let k = s.len().min(MAX_POINTS - pos);
        for i in 0..k {
            let row = &mut feats[(pos + i) * FEATURE_DIM..(pos + i + 1) * FEATURE_DIM];
            row[0] = s[i][1].clamp(-4.0, 4.0);
            if i == 0 {
                if let Some(prev) = prev_xy {
                    let jdx = s[0][0] - prev[0];
                    let jdy = s[0][1] - prev[1];
                    row[1] = jdx.clamp(-4.0, 4.0);
                    row[2] = jdy.clamp(-4.0, 4.0);
                    row[7] = jdx.hypot(jdy).ln_1p();
                }
                row[6] = 1.0;
            } else {
                let dx = (s[i][0] - s[i - 1][0]).clamp(-3.0, 3.0);
                let dy = (s[i][1] - s[i - 1][1]).clamp(-3.0, 3.0);
                let norm = dx.hypot(dy);
                let safe = norm.max(1e-5);
                row[1] = dx;
                row[2] = dy;
                row[3] = dx / safe;
                row[4] = dy / safe;
                row[7] = norm.ln_1p();
            }
            row[5] = if i + 1 == k { 0.0 } else { 1.0 };
        }
        prev_xy = Some(s[k - 1]);
        pos += k;
    }
    (feats, t_raw, t_pad)
}

#[inline(always)]
fn logsumexp2(a: f32, b: f32) -> f32 {
    if a == f32::NEG_INFINITY {
        return b;
    }
    if b == f32::NEG_INFINITY {
        return a;
    }
    if a > b {
        a + (b - a).exp().ln_1p()
    } else {
        b + (a - b).exp().ln_1p()
    }
}

fn extract_last_two_words(prefix: &str) -> (Option<&str>, &str) {
    let trimmed = prefix.trim_end();
    let mut parts = trimmed.rsplitn(3, ' ');
    let last = parts.next().unwrap_or("");
    let prev = parts.next();
    (prev, last)
}

fn ctc_beam_decode(
    logits: &[f32],
    t_len: usize,
    vocab: &[String],
    lm: Option<&BigramLexiconLm>,
    beam_width: usize,
    lm_weight: f32,
    temperature: f32,
) -> String {
    let v = vocab.len();
    if t_len == 0 || v == 0 {
        return String::new();
    }
    let space_id = vocab.iter().position(|s| s == " ");
    let mut beams: HashMap<String, (f32, f32, f32)> = HashMap::with_capacity(beam_width * 2);
    beams.insert(String::new(), (0.0, f32::NEG_INFINITY, 0.0));
    let mut lp_t = vec![0.0_f32; v];
    let mut top_idx: Vec<(usize, f32)> = Vec::with_capacity(v);

    for t in 0..t_len {
        let row = &logits[t * v..(t + 1) * v];
        let mut max_v = f32::NEG_INFINITY;
        for &val in row {
            let scaled = val / temperature;
            if scaled > max_v {
                max_v = scaled;
            }
        }
        let mut sum_exp = 0.0_f32;
        for i in 0..v {
            let e = (row[i] / temperature - max_v).exp();
            lp_t[i] = e;
            sum_exp += e;
        }
        let log_z = max_v + sum_exp.max(1e-12).ln();
        for i in 0..v {
            lp_t[i] = row[i] / temperature - log_z;
        }

        let p_blank_t = lp_t[0];
        if p_blank_t > -0.008 {
            for (_, (pb, pnb, _)) in beams.iter_mut() {
                *pb = logsumexp2(*pb, *pnb) + p_blank_t;
                *pnb = f32::NEG_INFINITY;
            }
            continue;
        }

        top_idx.clear();
        for cid in 1..v {
            if lp_t[cid] >= -9.5 {
                top_idx.push((cid, lp_t[cid]));
            }
        }
        top_idx.sort_by(|a, b| b.1.total_cmp(&a.1));
        top_idx.truncate(8);

        let mut next_beams: HashMap<String, (f32, f32, f32)> = HashMap::with_capacity(beam_width * 4);
        let acc = |map: &mut HashMap<String, (f32, f32, f32)>, pref: String, pb: f32, pnb: f32, lms: f32| {
            map.entry(pref)
                .and_modify(|cur| {
                    cur.0 = logsumexp2(cur.0, pb);
                    cur.1 = logsumexp2(cur.1, pnb);
                    if lms > cur.2 {
                        cur.2 = lms;
                    }
                })
                .or_insert((pb, pnb, lms));
        };

        for (pref, &(pb, pnb, lms)) in &beams {
            let p_tot = logsumexp2(pb, pnb);
            acc(&mut next_beams, pref.clone(), p_tot + p_blank_t, f32::NEG_INFINITY, lms);

            let last_ch = pref.chars().last();
            for &(cid, c_lp) in &top_idx {
                let ch_str = vocab[cid].as_str();
                let ch = ch_str.chars().next();
                if ch == last_ch && ch.is_some() {
                    acc(&mut next_beams, pref.clone(), f32::NEG_INFINITY, pnb + c_lp, lms);
                    if pb > f32::NEG_INFINITY {
                        let mut np = pref.clone();
                        np.push_str(ch_str);
                        acc(&mut next_beams, np, f32::NEG_INFINITY, pb + c_lp, lms);
                    }
                } else {
                    let mut np = pref.clone();
                    np.push_str(ch_str);
                    let mut new_lms = lms;
                    if let Some(lm) = lm
                        && Some(cid) == space_id
                        && !pref.is_empty()
                        && last_ch != Some(' ')
                    {
                        let (prev_w, last_w) = extract_last_two_words(pref);
                        new_lms = lms + lm_weight * lm.score_word(last_w, prev_w);
                    }
                    acc(&mut next_beams, np, f32::NEG_INFINITY, p_tot + c_lp, new_lms);
                }
            }
        }

        let mut ranked: Vec<(String, (f32, f32, f32))> = next_beams.into_iter().collect();
        ranked.sort_by(|a, b| {
            let sa = logsumexp2(a.1.0, a.1.1) + a.1.2;
            let sb = logsumexp2(b.1.0, b.1.1) + b.1.2;
            sb.total_cmp(&sa)
        });
        ranked.truncate(beam_width);
        beams = ranked.into_iter().collect();
    }

    let mut best_pref = String::new();
    let mut best_score = f32::NEG_INFINITY;
    for (pref, (pb, pnb, lms)) in beams {
        let mut score = logsumexp2(pb, pnb) + lms;
        if let Some(lm) = lm
            && !pref.is_empty()
            && !pref.ends_with(' ')
        {
            let (prev_w, last_w) = extract_last_two_words(&pref);
            score += lm_weight * lm.score_word(last_w, prev_w);
        }
        if score > best_score {
            best_score = score;
            best_pref = pref;
        }
    }
    best_pref.trim().to_string()
}

pub struct HtrModel {
    frec: FrecFile,
    rope: RopeTable,
    lm: Option<BigramLexiconLm>,
}

impl HtrModel {
    pub fn load(frec: FrecFile, lm: Option<BigramLexiconLm>) -> Result<Self, String> {
        if frec.header.kind != "htr_conformer_v2" {
            return Err(format!("Expected htr_conformer_v2 model, got {}", frec.header.kind));
        }
        let d = frec.header.config.d_model;
        let hd = d / frec.header.config.nhead;
        let rope = RopeTable::new(hd, 2048);
        Ok(Self { frec, rope, lm })
    }

    pub fn recognize_line(&self, strokes: &[Vec<[f32; 2]>]) -> Result<String, String> {
        let cfg = &self.frec.header.config;
        let d = cfg.d_model;
        let nhead = cfg.nhead;
        let dim_ff = cfg.dim_ff;
        let vocab = &self.frec.header.vocab;

        let (feats, t_raw, t_pad) = line_strokes_to_features(strokes);
        let (w0, _) = self.frec.f32_slice("stem.0.weight")?;
        let (b0, _) = self.frec.f32_slice("stem.0.bias")?;
        let (h0, t1) = conv1d(&feats, t_pad, cfg.in_features, w0, b0, d / 2, 5, 2, 2, true);
        let (w2, _) = self.frec.f32_slice("stem.2.weight")?;
        let (b2, _) = self.frec.f32_slice("stem.2.bias")?;
        let (h1, t_enc) = conv1d(&h0, t1, d / 2, w2, b2, d, 5, 2, 2, true);

        let mut x = vec![0.0_f32; t_enc * d];
        let (in_w, _) = self.frec.f32_slice("in_norm.weight")?;
        let (in_b, _) = self.frec.f32_slice("in_norm.bias")?;
        layer_norm(&h1, t_enc, d, in_w, in_b, &mut x);

        for l in 0..cfg.num_layers {
            conformer_block(
                &self.frec,
                &format!("layers.{l}"),
                true,
                &mut x,
                t_enc,
                d,
                nhead,
                dim_ff,
                cfg.conv_kernel,
                &self.rope,
            )?;
        }

        let head_w = self.frec.linear("ctc_head.weight")?;
        let head_b = self.frec.opt_f32("ctc_head.bias");
        let mut logits = vec![0.0_f32; t_enc * vocab.len()];
        linear(&x, t_enc, &head_w, head_b, &mut logits);

        let valid_out = ((t_raw + 3) / 4).min(t_enc);
        Ok(ctc_beam_decode(
            &logits,
            valid_out,
            vocab,
            self.lm.as_ref(),
            20,
            0.70,
            1.25,
        ))
    }

    pub fn recognize_strokes(&self, strokes: &[Vec<[f32; 2]>]) -> Result<String, String> {
        let lines = segment_lines(strokes);
        if lines.is_empty() {
            return Ok(String::new());
        }
        let mut out = Vec::with_capacity(lines.len());
        for line in &lines {
            let text = self.recognize_line(line)?;
            if !text.is_empty() {
                out.push(text);
            }
        }
        Ok(out.join("\n"))
    }
}
