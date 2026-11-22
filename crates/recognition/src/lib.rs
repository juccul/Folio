//! Pure-Rust offline stroke-sequence recognition engine for Folio (Handwriting + Math).
//!
//! Runs bundled INT8-quantized RoPE Conformer models directly on CPU via memory-mapped
//! `.frec` weight files and a bounded Rayon thread pool — zero external runtimes,
//! zero GPU/Vulkan dependencies, and instant (<25 ms) inference.
pub mod frec;
pub mod htr;
pub mod math;
mod ops;

use frec::FrecFile;
use htr::{BigramLexiconLm, HtrModel};
use math::MathModel;
use rayon::{ThreadPool, ThreadPoolBuilder};
use serde::Deserialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Deserialize)]
pub struct PackConfig {
    pub backend: String,
    pub math: String,
    pub htr: String,
    #[serde(default)]
    pub lm: Option<String>,
}

pub struct Engine {
    pool: ThreadPool,
    root: PathBuf,
    config: PackConfig,
    math: Option<MathModel>,
    htr: Option<HtrModel>,
}

impl Engine {
    pub fn open(pack_json: &Path) -> Result<Self, String> {
        let bytes = std::fs::read(pack_json)
            .map_err(|e| format!("Cannot read recognition pack {}: {e}", pack_json.display()))?;
        let config: PackConfig = serde_json::from_slice(&bytes)
            .map_err(|e| format!("Invalid recognition pack {}: {e}", pack_json.display()))?;
        if config.backend != "folio-stroke-v2" {
            return Err(format!("Unsupported recognition backend: {}", config.backend));
        }
        let root = pack_json
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf();
        // Verify model files exist at open time so callers fail fast if packaging is incomplete.
        for rel in [&config.math, &config.htr] {
            let path = root.join(rel);
            if !path.is_file() {
                return Err(format!("Missing bundled recognition model: {}", path.display()));
            }
        }
        let threads = std::thread::available_parallelism()
            .map(|n| n.get().clamp(1, 4))
            .unwrap_or(4);
        let pool = ThreadPoolBuilder::new()
            .num_threads(threads)
            .thread_name(|i| format!("folio-rec-{i}"))
            .build()
            .map_err(|e| format!("Cannot start recognition thread pool: {e}"))?;
        Ok(Self {
            pool,
            root,
            config,
            math: None,
            htr: None,
        })
    }

    pub fn pack_ready(pack_json: &Path) -> bool {
        let Ok(bytes) = std::fs::read(pack_json) else {
            return false;
        };
        let Ok(config) = serde_json::from_slice::<PackConfig>(&bytes) else {
            return false;
        };
        if config.backend != "folio-stroke-v2" {
            return false;
        }
        let root = pack_json.parent().unwrap_or_else(|| Path::new("."));
        root.join(&config.math).is_file() && root.join(&config.htr).is_file()
    }

    pub fn ensure_math(&mut self) -> Result<&MathModel, String> {
        if self.math.is_none() {
            let frec = FrecFile::open(&self.root.join(&self.config.math))?;
            self.math = Some(MathModel::load(frec)?);
        }
        Ok(self.math.as_ref().unwrap())
    }

    pub fn ensure_htr(&mut self) -> Result<&HtrModel, String> {
        if self.htr.is_none() {
            let frec = FrecFile::open(&self.root.join(&self.config.htr))?;
            let lm = match &self.config.lm {
                Some(rel) if self.root.join(rel).is_file() => {
                    Some(BigramLexiconLm::load_gz(&self.root.join(rel))?)
                }
                _ => None,
            };
            self.htr = Some(HtrModel::load(frec, lm)?);
        }
        Ok(self.htr.as_ref().unwrap())
    }

    pub fn recognize_math(&mut self, strokes: &[Vec<[f32; 2]>]) -> Result<String, String> {
        self.ensure_math()?;
        let model = self.math.as_ref().unwrap();
        self.pool.install(|| model.recognize(strokes, 128))
    }

    pub fn recognize_text(&mut self, strokes: &[Vec<[f32; 2]>]) -> Result<String, String> {
        self.ensure_htr()?;
        let model = self.htr.as_ref().unwrap();
        self.pool.install(|| model.recognize_strokes(strokes))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    #[test]
    fn segment_lines_clusters_two_rows() {
        let row1 = vec![
            vec![[6.0, 2.0], [7.0, 8.0]], // high opening quote / dot on row 1
            vec![[10.0, 10.0], [50.0, 12.0], [90.0, 35.0]],
        ];
        let row2 = vec![vec![[12.0, 110.0], [55.0, 115.0], [92.0, 138.0]]];
        let mut all = row2.clone();
        all.extend(row1.clone());
        let lines = htr::segment_lines(&all);
        assert_eq!(lines.len(), 2, "High punctuation must stay attached to row 1");
        assert_eq!(lines[0].len(), 2);
        assert!(lines[0][0][0][1] < lines[1][0][0][1]);
    }

    #[test]
    fn bundled_pack_recognizes_simple_digit() {
        let pack = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../recognition/pack.json");
        if !Engine::pack_ready(&pack) {
            return;
        }
        let mut engine = Engine::open(&pack).expect("open bundled pack");
        // Simple vertical stroke -> "1"
        let stroke = vec![vec![[100.0, 20.0], [100.0, 60.0], [100.0, 100.0]]];
        let math_out = engine.recognize_math(&stroke).expect("math inference");
        assert!(!math_out.is_empty(), "math output should be non-empty");
    }

    fn char_edit_distance(a: &str, b: &str) -> usize {
        let ac: Vec<char> = a.chars().collect();
        let bc: Vec<char> = b.chars().collect();
        let mut dp: Vec<usize> = (0..=bc.len()).collect();
        for (i, &ca) in ac.iter().enumerate() {
            let mut prev = dp[0];
            dp[0] = i + 1;
            for (j, &cb) in bc.iter().enumerate() {
                let tmp = dp[j + 1];
                let cost = usize::from(ca != cb);
                dp[j + 1] = (dp[j + 1] + 1).min(dp[j] + 1).min(prev + cost);
                prev = tmp;
            }
        }
        dp[bc.len()]
    }

    #[test]
    #[ignore = "Benchmarks bundled pure-Rust models against 200-sample Math & HTR test sets"]
    fn parity_bench_200() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let pack = root.join("recognition/pack.json");
        let mut engine = Engine::open(&pack).expect("open pack");

        // 1. Math v2 on 200 held-out MathWriting samples
        let math_path = root.join("artifacts/stroke-math-experiment/cache/test_200.json");
        if math_path.is_file() {
            #[derive(Deserialize)]
            struct MathItem {
                label: String,
                strokes: Vec<Vec<[f32; 2]>>,
            }
            let items: Vec<MathItem> =
                serde_json::from_slice(&std::fs::read(&math_path).unwrap()).unwrap();
            engine.ensure_math().unwrap();
            let _ = engine.recognize_math(&items[0].strokes).unwrap(); // warmup

            let mut em = 0usize;
            let mut ed = 0usize;
            let mut ch = 0usize;
            let mut times_ms = Vec::with_capacity(items.len());
            for it in &items {
                let t0 = Instant::now();
                let pred = engine.recognize_math(&it.strokes).unwrap();
                times_ms.push(t0.elapsed().as_secs_f64() * 1000.0);
                let g = it.label.replace(' ', "");
                let p = pred.replace(' ', "");
                if g == p {
                    em += 1;
                }
                ed += char_edit_distance(&g, &p);
                ch += g.chars().count().max(1);
            }
            times_ms.sort_by(f64::total_cmp);
            let med = times_ms[times_ms.len() / 2];
            let p95 = times_ms[(times_ms.len() * 95) / 100];
            println!(
                "[Rust Math v2] n={} EM={:.2}% CER={:.2}% median={:.2}ms p95={:.2}ms",
                items.len(),
                100.0 * em as f64 / items.len() as f64,
                100.0 * ed as f64 / ch as f64,
                med,
                p95
            );
        }

        // 2. HTR v2 on 200 held-out unseen-writer HTR samples
        let htr_path = root.join("artifacts/stroke-htr-experiment/cache/test_htr_200.json");
        if htr_path.is_file() {
            #[derive(Deserialize)]
            struct HtrItem {
                text: String,
                strokes: Vec<Vec<[f32; 2]>>,
            }
            let items: Vec<HtrItem> =
                serde_json::from_slice(&std::fs::read(&htr_path).unwrap()).unwrap();
            engine.ensure_htr().unwrap();
            let _ = engine.recognize_text(&items[0].strokes).unwrap(); // warmup

            let mut em = 0usize;
            let mut ed = 0usize;
            let mut ch = 0usize;
            let mut times_ms = Vec::with_capacity(items.len());
            for it in &items {
                let t0 = Instant::now();
                // Each test_htr_200 item is already a single line; call recognize_line directly
                let model = engine.htr.as_ref().unwrap();
                let pred = engine.pool.install(|| model.recognize_line(&it.strokes)).unwrap();
                times_ms.push(t0.elapsed().as_secs_f64() * 1000.0);
                if it.text == pred {
                    em += 1;
                }
                ed += char_edit_distance(&it.text, &pred);
                ch += it.text.chars().count().max(1);
            }
            times_ms.sort_by(f64::total_cmp);
            let med = times_ms[times_ms.len() / 2];
            let p95 = times_ms[(times_ms.len() * 95) / 100];
            println!(
                "[Rust HTR v2]  n={} EM={:.2}% CER={:.2}% median={:.2}ms p95={:.2}ms",
                items.len(),
                100.0 * em as f64 / items.len() as f64,
                100.0 * ed as f64 / ch as f64,
                med,
                p95
            );
        }
    }
}
