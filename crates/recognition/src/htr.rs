//! Handwriting v2: multi-line vector segmentation + 8-feature RoPE Conformer-CTC + Bigram LM beam search.
use crate::{
    frec::FrecFile,
    ops::{RopeTable, conformer_block, conv1d, layer_norm, linear},
};
use flate2::read::GzDecoder;
use serde::Deserialize;
use std::{
    collections::{HashMap, HashSet},
    fs::File,
    io::BufReader,
    path::Path,
};

const FEATURE_DIM: usize = 8;
const MAX_POINTS: usize = 1024;
const DEL1_BUCKET_CAP: usize = 16;

/// Common multi-character handwriting visual confusions (ligatures, splits, strokes).
const MULTI_CHAR_CONFUSIONS: &[(&str, &str)] = &[
    ("rn", "m"),
    ("m", "rn"),
    ("nn", "m"),
    ("m", "nn"),
    ("cl", "d"),
    ("d", "cl"),
    ("vv", "w"),
    ("w", "vv"),
    ("ri", "n"),
    ("n", "ri"),
    ("ii", "u"),
    ("u", "ii"),
    ("ll", "tt"),
    ("tt", "ll"),
    ("ll", "tl"),
    ("in", "m"),
    ("m", "in"),
    ("ni", "m"),
    ("m", "ni"),
    ("ce", "nce"),
    ("e", "ce"),
    ("st", "sc"),
    ("sc", "st"),
    ("ci", "oo"),
    ("ol", "d"),
    ("d", "ol"),
];

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
    lower_counts: HashMap<String, u32>,
    bigram_lp: HashMap<String, f32>,
    char4: HashMap<String, u32>,
    char3: HashMap<String, u32>,
    log_tot: f32,
    /// Corpus-attested lowercase words sorted by descending frequency (id -> (word, count)).
    lex_words: Vec<(String, u32)>,
    /// SymSpell 1-delete inverted index mapping delete-key -> top word ids in `lex_words`.
    del1: HashMap<String, Vec<u32>>,
}

impl BigramLexiconLm {
    pub fn load_gz(path: &Path) -> Result<Self, String> {
        let file = File::open(path).map_err(|e| format!("Cannot open LM {}: {e}", path.display()))?;
        let reader = BufReader::new(GzDecoder::new(file));
        let raw: LmStatsRaw =
            serde_json::from_reader(reader).map_err(|e| format!("Invalid LM {}: {e}", path.display()))?;
        Ok(Self::from_raw(raw))
    }

    fn from_raw(raw: LmStatsRaw) -> Self {
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

        // Build compact SymSpell 1-delete index over corpus-attested words (count >= 3).
        let mut lex_words: Vec<(String, u32)> = lower_counts
            .iter()
            .filter(|(w, c)| {
                **c >= 3
                    && w.len() >= 2
                    && w.len() <= 20
                    && w.bytes().all(|b| b.is_ascii_alphabetic() || b == b'\'')
            })
            .map(|(w, &c)| (w.clone(), c))
            .collect();
        lex_words.sort_unstable_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));

        let mut del1: HashMap<String, Vec<u32>> = HashMap::with_capacity(lex_words.len() * 4);
        let mut key_buf = String::with_capacity(20);
        for (idx, (w, _)) in lex_words.iter().enumerate() {
            let wid = idx as u32;
            let bytes = w.as_bytes();
            for i in 0..bytes.len() {
                key_buf.clear();
                key_buf.push_str(&w[..i]);
                key_buf.push_str(&w[i + 1..]);
                if let Some(bucket) = del1.get_mut(&key_buf) {
                    if bucket.len() < DEL1_BUCKET_CAP && *bucket.last().unwrap() != wid {
                        bucket.push(wid);
                    }
                } else {
                    del1.insert(key_buf.clone(), vec![wid]);
                }
            }
        }

        Self {
            word_lp,
            lower_lp,
            lower_counts,
            bigram_lp,
            char4: raw.char4,
            char3: raw.char3,
            log_tot,
            lex_words,
            del1,
        }
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

    fn split_word_affixes(word: &str) -> Option<(&str, &str, &str)> {
        let bytes = word.as_bytes();
        let start = bytes.iter().position(|b| b.is_ascii_alphabetic())?;
        let end = bytes.iter().rposition(|b| b.is_ascii_alphabetic())? + 1;
        let core = &word[start..end];
        if !core
            .bytes()
            .all(|b| b.is_ascii_alphabetic() || b == b'\'')
        {
            return None;
        }
        Some((&word[..start], core, &word[end..]))
    }

    fn lower_count(&self, w_low: &str) -> u32 {
        self.lower_counts.get(w_low).copied().unwrap_or(0)
    }

    fn rescore_unigram(&self, w_low: &str, oov_pen: f32) -> f32 {
        let c = self.lower_count(w_low);
        if c == 0 {
            -oov_pen
        } else {
            let bonus = if c <= 2 { 10.3 } else { 11.2 };
            ((c as f32) + 1.0).ln() - self.log_tot + bonus
        }
    }

    fn rescore_bigram(&self, prev_low: &str, next_low: &str) -> f32 {
        let key = format!("{prev_low} {next_low}");
        self.bigram_lp
            .get(&key)
            .map_or(0.0, |&lp| (lp + 7.5).max(0.0))
    }

    /// Generate up to `max_cands` corpus-attested edit-distance 1–2 & visual-confusion candidates.
    fn generate_candidates<'a>(&'a self, w_low: &str, allow_ed2: bool, max_cands: usize) -> Vec<&'a str> {
        let mut seen: HashSet<&'a str> = HashSet::with_capacity(64);
        let insert_if_attested = |cand: &str, set: &mut HashSet<&'a str>| {
            if let Some((w, &c)) = self.lower_counts.get_key_value(cand)
                && c >= 3
            {
                set.insert(w.as_str());
            }
        };
        if let Some(bucket) = self.del1.get(w_low) {
            for &wid in bucket {
                seen.insert(self.lex_words[wid as usize].0.as_str());
            }
        }
        let bytes = w_low.as_bytes();
        let mut d1_buf = String::with_capacity(w_low.len());
        let mut d2_buf = String::with_capacity(w_low.len());
        let mut seen_d1: HashSet<String> = HashSet::new();

        for i in 0..bytes.len() {
            d1_buf.clear();
            d1_buf.push_str(&w_low[..i]);
            d1_buf.push_str(&w_low[i + 1..]);
            insert_if_attested(&d1_buf, &mut seen);
            if let Some(bucket) = self.del1.get(&d1_buf) {
                for &wid in bucket {
                    seen.insert(self.lex_words[wid as usize].0.as_str());
                }
            }
            if allow_ed2 && bytes.len() >= 4 && seen_d1.insert(d1_buf.clone()) {
                let d1_len = d1_buf.len();
                for j in i..d1_len {
                    d2_buf.clear();
                    d2_buf.push_str(&d1_buf[..j]);
                    d2_buf.push_str(&d1_buf[j + 1..]);
                    insert_if_attested(&d2_buf, &mut seen);
                    if let Some(bucket) = self.del1.get(&d2_buf) {
                        for &wid in bucket {
                            seen.insert(self.lex_words[wid as usize].0.as_str());
                        }
                    }
                }
            }
        }

        if allow_ed2 && bytes.len() >= 4 {
            for &(from, to) in MULTI_CHAR_CONFUSIONS {
                let mut pos = 0usize;
                while let Some(rel) = w_low[pos..].find(from) {
                    let idx = pos + rel;
                    let mut alt = String::with_capacity(w_low.len() + 2);
                    alt.push_str(&w_low[..idx]);
                    alt.push_str(to);
                    alt.push_str(&w_low[idx + from.len()..]);
                    insert_if_attested(&alt, &mut seen);
                    if let Some(bucket) = self.del1.get(&alt) {
                        for &wid in bucket {
                            seen.insert(self.lex_words[wid as usize].0.as_str());
                        }
                    }
                    for k in 0..alt.len() {
                        d1_buf.clear();
                        d1_buf.push_str(&alt[..k]);
                        d1_buf.push_str(&alt[k + 1..]);
                        if let Some(bucket) = self.del1.get(&d1_buf) {
                            for &wid in bucket {
                                seen.insert(self.lex_words[wid as usize].0.as_str());
                            }
                        }
                    }
                    pos = idx + 1;
                }
            }
        }

        seen.remove(w_low);
        let mut out: Vec<(&str, u32)> = seen
            .into_iter()
            .filter(|c| c.len().abs_diff(w_low.len()) <= 2)
            .map(|c| (c, self.lower_count(c)))
            .collect();
        out.sort_unstable_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(b.0)));
        out.truncate(max_cands);
        out.into_iter().map(|(w, _)| w).collect()
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

fn ascii_edit_distance(a: &str, b: &str) -> usize {
    let ab = a.as_bytes();
    let bb = b.as_bytes();
    let mut dp: Vec<usize> = (0..=bb.len()).collect();
    for (i, &ca) in ab.iter().enumerate() {
        let mut prev = dp[0];
        dp[0] = i + 1;
        for (j, &cb) in bb.iter().enumerate() {
            let tmp = dp[j + 1];
            let cost = usize::from(ca != cb);
            dp[j + 1] = (dp[j + 1] + 1).min(dp[j] + 1).min(prev + cost);
            prev = tmp;
        }
    }
    dp[bb.len()]
}

fn match_word_case(orig: &str, cand_low: &str) -> String {
    if orig.len() > 1 && orig.bytes().all(|b| !b.is_ascii_lowercase()) {
        return cand_low.to_ascii_uppercase();
    }
    if orig.as_bytes().first().is_some_and(u8::is_ascii_uppercase) {
        let mut out = String::with_capacity(cand_low.len());
        let mut chars = cand_low.chars();
        if let Some(first) = chars.next() {
            out.push(first.to_ascii_uppercase());
            out.push_str(chars.as_str());
        }
        return out;
    }
    cand_low.to_string()
}

/// Exact CTC forward log-probability `log P(token_ids | lp[t0..t1])`.
fn ctc_forward_score(lp: &[f32], v: usize, t0: usize, t1: usize, token_ids: &[usize]) -> f32 {
    if t1 <= t0 {
        return f32::NEG_INFINITY;
    }
    let l = token_ids.len();
    if l == 0 {
        let mut s = 0.0_f32;
        for t in t0..t1 {
            s += lp[t * v];
        }
        return s;
    }
    let s_len = 2 * l + 1;
    let mut ext = vec![0usize; s_len];
    for (i, &tid) in token_ids.iter().enumerate() {
        ext[2 * i + 1] = tid;
    }
    let mut alpha = vec![f32::NEG_INFINITY; s_len];
    let mut next = vec![f32::NEG_INFINITY; s_len];
    let row0 = &lp[t0 * v..(t0 + 1) * v];
    alpha[0] = row0[0];
    if s_len > 1 {
        alpha[1] = row0[ext[1]];
    }
    for t in (t0 + 1)..t1 {
        let row = &lp[t * v..(t + 1) * v];
        for s in 0..s_len {
            let mut acc = alpha[s];
            if s > 0 {
                acc = logsumexp2(acc, alpha[s - 1]);
            }
            if s > 1 && ext[s] != 0 && ext[s] != ext[s - 2] {
                acc = logsumexp2(acc, alpha[s - 2]);
            }
            next[s] = acc + row[ext[s]];
        }
        std::mem::swap(&mut alpha, &mut next);
    }
    logsumexp2(alpha[s_len - 1], alpha[s_len - 2])
}

/// Forced CTC Viterbi alignment mapping each whitespace-delimited token to `[t0, t1)`.
fn ctc_viterbi_word_spans(
    lp: &[f32],
    t_len: usize,
    v: usize,
    text: &str,
    char_to_id: &HashMap<char, usize>,
) -> Vec<(String, usize, usize)> {
    let chars: Vec<char> = text.chars().filter(|c| char_to_id.contains_key(c)).collect();
    if chars.is_empty() || t_len == 0 {
        return Vec::new();
    }
    let l = chars.len();
    let s_len = 2 * l + 1;
    let mut ext = vec![0usize; s_len];
    for (i, &ch) in chars.iter().enumerate() {
        ext[2 * i + 1] = char_to_id[&ch];
    }

    let mut dp = vec![f32::NEG_INFINITY; t_len * s_len];
    let mut bp = vec![0u32; t_len * s_len];
    dp[0] = lp[0];
    if s_len > 1 {
        dp[1] = lp[ext[1]];
    }
    for t in 1..t_len {
        let row = &lp[t * v..(t + 1) * v];
        let prev_off = (t - 1) * s_len;
        let cur_off = t * s_len;
        for s in 0..s_len {
            let mut best_v = dp[prev_off + s];
            let mut best_p = s;
            if s > 0 && dp[prev_off + s - 1] > best_v {
                best_v = dp[prev_off + s - 1];
                best_p = s - 1;
            }
            if s > 1 && ext[s] != 0 && ext[s] != ext[s - 2] && dp[prev_off + s - 2] > best_v {
                best_v = dp[prev_off + s - 2];
                best_p = s - 2;
            }
            dp[cur_off + s] = best_v + row[ext[s]];
            bp[cur_off + s] = best_p as u32;
        }
    }

    let last_off = (t_len - 1) * s_len;
    let mut cur = if dp[last_off + s_len - 1] >= dp[last_off + s_len - 2] {
        s_len - 1
    } else {
        s_len - 2
    };
    let mut c_start = vec![t_len; l];
    let mut c_end = vec![0usize; l];
    let mut c_seen = vec![false; l];
    for t in (0..t_len).rev() {
        if cur % 2 == 1 {
            let ci = cur / 2;
            if t < c_start[ci] {
                c_start[ci] = t;
            }
            if !c_seen[ci] || t > c_end[ci] {
                c_end[ci] = t;
            }
            c_seen[ci] = true;
        }
        cur = bp[t * s_len + cur] as usize;
    }

    let mut words = Vec::new();
    let mut i = 0usize;
    while i < l {
        if chars[i] == ' ' {
            i += 1;
            continue;
        }
        let mut j = i;
        while j < l && chars[j] != ' ' {
            j += 1;
        }
        let w: String = chars[i..j].iter().collect();
        let mut t0 = t_len;
        let mut t1 = 0usize;
        let mut any = false;
        for ci in i..j {
            if c_seen[ci] {
                t0 = t0.min(c_start[ci]);
                t1 = t1.max(c_end[ci] + 1);
                any = true;
            }
        }
        if any {
            let prev_end = if i > 0 && c_seen[i - 1] {
                c_end[i - 1] + 1
            } else {
                0
            };
            let next_start = if j < l && c_seen[j] {
                c_start[j]
            } else {
                t_len
            };
            t0 = t0.saturating_sub(2).max(prev_end);
            t1 = (t1 + 2).min(next_start);
            if t1 > t0 {
                words.push((w, t0, t1));
            }
        }
        i = j;
    }
    words
}

/// Acoustic-anchored OOV word rescorer using exact CTC forward log-probabilities.
/// Fixes 1–2 letter handwriting near-misses ("estaped"->"escaped", "assistece"->"assistance")
/// without overriding real names/acronyms whose strokes do not match dictionary candidates.
fn rescore_oov_words(
    logits: &[f32],
    t_len: usize,
    v: usize,
    beam_text: &str,
    lm: &BigramLexiconLm,
    char_to_id: &HashMap<char, usize>,
) -> String {
    if beam_text.is_empty() || t_len == 0 {
        return beam_text.to_string();
    }
    // Fast path: skip Viterbi + CTC forward completely if every alphabetic word is already attested.
    let needs_rescore = beam_text.split_whitespace().any(|w| {
        BigramLexiconLm::split_word_affixes(w).is_some_and(|(_, core, _)| {
            let len = core.len();
            let all_upper = core.bytes().all(|b| b.is_ascii_uppercase());
            len >= 3 && !(all_upper && len <= 5) && lm.lower_count(&core.to_ascii_lowercase()) == 0
        })
    });
    if !needs_rescore {
        return beam_text.to_string();
    }

    // Calibrated log-probabilities for CTC forward scoring (softens overconfident CTC spikes).
    let temp = 1.80_f32;
    let mut lp = vec![0.0_f32; t_len * v];
    for t in 0..t_len {
        let src = &logits[t * v..(t + 1) * v];
        let dst = &mut lp[t * v..(t + 1) * v];
        let mut max_v = f32::NEG_INFINITY;
        for &val in src {
            let s = val / temp;
            if s > max_v {
                max_v = s;
            }
        }
        let mut sum_e = 0.0_f32;
        for i in 0..v {
            let e = (src[i] / temp - max_v).exp();
            dst[i] = e;
            sum_e += e;
        }
        let log_z = max_v + sum_e.max(1e-12).ln();
        for i in 0..v {
            dst[i] = src[i] / temp - log_z;
        }
    }

    let spans = ctc_viterbi_word_spans(&lp, t_len, v, beam_text, char_to_id);
    if spans.is_empty() {
        return beam_text.to_string();
    }

    let mut out_words: Vec<String> = spans.iter().map(|(w, _, _)| w.clone()).collect();
    let alpha_uni = 1.20_f32;
    let alpha_bi = 0.75_f32;
    let oov_pen = 3.00_f32;
    let mut orig_ids = Vec::with_capacity(32);
    let mut cand_ids = Vec::with_capacity(32);

    for (idx, (w, t0, t1)) in spans.iter().enumerate() {
        let Some((pfx, core, sfx)) = BigramLexiconLm::split_word_affixes(w) else {
            continue;
        };
        if core.len() < 3 {
            continue;
        }
        if core.bytes().all(|b| b.is_ascii_uppercase()) && core.len() <= 5 {
            continue;
        }
        let w_low = core.to_ascii_lowercase();
        if lm.lower_count(&w_low) > 0 {
            continue;
        }
        let mid_sentence_proper =
            idx > 0 && core.as_bytes()[0].is_ascii_uppercase() && core.bytes().skip(1).any(|b| b.is_ascii_lowercase());

        let prev_low = if idx > 0 {
            BigramLexiconLm::strip_word_core(&out_words[idx - 1])
                .map(str::to_ascii_lowercase)
                .unwrap_or_else(|| "<s>".to_string())
        } else {
            "<s>".to_string()
        };
        let next_low = if idx + 1 < spans.len() {
            BigramLexiconLm::strip_word_core(&spans[idx + 1].0)
                .map(str::to_ascii_lowercase)
                .unwrap_or_else(|| "</s>".to_string())
        } else {
            "</s>".to_string()
        };

        orig_ids.clear();
        for ch in w.chars() {
            if let Some(&cid) = char_to_id.get(&ch) {
                orig_ids.push(cid);
            }
        }
        let orig_ctc = ctc_forward_score(&lp, v, *t0, *t1, &orig_ids);
        let orig_score = orig_ctc
            + alpha_uni * lm.rescore_unigram(&w_low, oov_pen)
            + alpha_bi * (lm.rescore_bigram(&prev_low, &w_low) + lm.rescore_bigram(&w_low, &next_low));

        let cands = lm.generate_candidates(&w_low, !mid_sentence_proper, 64);
        let mut best_w = w.clone();
        let mut best_score = orig_score + if mid_sentence_proper { 0.90 } else { 0.45 };

        for cand_low in cands {
            let d = ascii_edit_distance(&w_low, cand_low);
            if d == 0 || d > 2 {
                continue;
            }
            if mid_sentence_proper && d > 1 {
                continue;
            }
            let cand_core = match_word_case(core, cand_low);
            let mut cand_full = String::with_capacity(pfx.len() + cand_core.len() + sfx.len());
            cand_full.push_str(pfx);
            cand_full.push_str(&cand_core);
            cand_full.push_str(sfx);

            cand_ids.clear();
            let mut ok = true;
            for ch in cand_full.chars() {
                if let Some(&cid) = char_to_id.get(&ch) {
                    cand_ids.push(cid);
                } else {
                    ok = false;
                    break;
                }
            }
            if !ok {
                continue;
            }

            let ctc_s = ctc_forward_score(&lp, v, *t0, *t1, &cand_ids);
            let max_drop = if mid_sentence_proper {
                4.2
            } else if d == 1 {
                6.8
            } else {
                5.2
            };
            if orig_ctc - ctc_s > max_drop {
                continue;
            }
            let ed_pen = if d == 2 { 1.35 } else { 0.0 };
            let sc = ctc_s
                + alpha_uni * lm.rescore_unigram(cand_low, oov_pen)
                + alpha_bi * (lm.rescore_bigram(&prev_low, cand_low) + lm.rescore_bigram(cand_low, &next_low))
                - ed_pen;
            if sc > best_score {
                best_score = sc;
                best_w = cand_full;
            }
        }
        out_words[idx] = best_w;
    }
    out_words.join(" ")
}

pub struct HtrModel {
    frec: FrecFile,
    rope: RopeTable,
    lm: Option<BigramLexiconLm>,
    char_to_id: HashMap<char, usize>,
}

impl HtrModel {
    pub fn load(frec: FrecFile, lm: Option<BigramLexiconLm>) -> Result<Self, String> {
        if frec.header.kind != "htr_conformer_v2" {
            return Err(format!("Expected htr_conformer_v2 model, got {}", frec.header.kind));
        }
        let d = frec.header.config.d_model;
        let hd = d / frec.header.config.nhead;
        let rope = RopeTable::new(hd, 2048);
        let mut char_to_id = HashMap::with_capacity(frec.header.vocab.len());
        for (idx, s) in frec.header.vocab.iter().enumerate() {
            if idx > 0 {
                let mut chars = s.chars();
                if let (Some(c), None) = (chars.next(), chars.next()) {
                    char_to_id.insert(c, idx);
                }
            }
        }
        Ok(Self {
            frec,
            rope,
            lm,
            char_to_id,
        })
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
        let beam = ctc_beam_decode(
            &logits,
            valid_out,
            vocab,
            self.lm.as_ref(),
            20,
            0.70,
            1.25,
        );
        if let Some(lm) = self.lm.as_ref() {
            Ok(rescore_oov_words(
                &logits,
                valid_out,
                vocab.len(),
                &beam,
                lm,
                &self.char_to_id,
            ))
        } else {
            Ok(beam)
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn test_lm() -> BigramLexiconLm {
        let mut words = HashMap::new();
        let mut words_lower = HashMap::new();
        for &(w, c) in &[
            ("the", 50_000u32),
            ("escaped", 800),
            ("assistance", 600),
            ("extend", 500),
            ("prisoner", 400),
            ("quickly", 700),
            ("famous", 900),
        ] {
            words.insert(w.to_string(), c);
            words_lower.insert(w.to_string(), c);
        }
        let mut bigrams = HashMap::new();
        bigrams.insert("the prisoner".to_string(), 120);
        bigrams.insert("prisoner escaped".to_string(), 80);
        BigramLexiconLm::from_raw(LmStatsRaw {
            total_words: 500_000,
            words,
            words_lower,
            bigrams,
            char4: HashMap::new(),
            char3: HashMap::new(),
        })
    }

    #[test]
    fn rescorer_fixes_near_miss_when_acoustics_support_candidate_and_preserves_true_oov() {
        let lm = test_lm();
        let mut vocab = vec!["<blank>".to_string(), " ".to_string()];
        for ch in 'a'..='z' {
            vocab.push(ch.to_string());
        }
        let mut char_to_id = HashMap::new();
        for (i, s) in vocab.iter().enumerate().skip(1) {
            char_to_id.insert(s.chars().next().unwrap(), i);
        }
        let v = vocab.len();

        // Synthesize CTC logits for "estaped" where the 't' frame also has strong 'c' logit (near-miss).
        let text = "estaped";
        let t_len = text.len() * 2 + 1;
        let mut logits = vec![-6.0_f32; t_len * v];
        for (ci, ch) in text.chars().enumerate() {
            let blank_t = ci * 2;
            logits[blank_t * v] = 5.0;
            let char_t = ci * 2 + 1;
            let cid = char_to_id[&ch];
            logits[char_t * v + cid] = 5.0;
            if ch == 't' {
                logits[char_t * v + char_to_id[&'c']] = 4.1;
            }
        }
        logits[(t_len - 1) * v] = 5.0;

        let fixed = rescore_oov_words(&logits, t_len, v, "estaped", &lm, &char_to_id);
        assert_eq!(fixed, "escaped", "expected near-miss OOV word to snap to acoustically supported 'escaped'");

        // Now synthesize sharp unambiguous logits for a genuine unknown name "zxqvkj" -> must NOT be corrupted.
        let oov = "zxqvkj";
        let t2 = oov.len() * 2 + 1;
        let mut logits2 = vec![-12.0_f32; t2 * v];
        for (ci, ch) in oov.chars().enumerate() {
            logits2[(ci * 2) * v] = 8.0;
            logits2[(ci * 2 + 1) * v + char_to_id[&ch]] = 8.0;
        }
        logits2[(t2 - 1) * v] = 8.0;
        let kept = rescore_oov_words(&logits2, t2, v, oov, &lm, &char_to_id);
        assert_eq!(kept, oov, "genuine OOV token must stay untouched");
    }
}
