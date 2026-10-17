use super::*;

/// Monotonic request timings. Frame completion is not physical display latency.
#[derive(Clone, Debug, Serialize)]
pub struct RecognitionTiming {
    pub request_id: Id,
    pub generation: u64,
    pub submission_ms: f64,
    pub queue_ms: f64,
    pub preparation_ms: f64,
    pub model_load_ms: f64,
    pub inference_ms: f64,
    pub server_prompt_ms: f64,
    pub server_decode_ms: f64,
    pub review_ms: Option<f64>,
    pub frame_ms: Option<f64>,
    pub cache_hit: bool,
    #[serde(skip)]
    pub(super) submitted: Instant,
}
impl RecognitionTiming {
    pub(super) fn new(task: &Task) -> Self {
        Self {
            request_id: Id::new_v4(),
            generation: task.generation,
            submission_ms: task.queued.duration_since(task.submitted).as_secs_f64() * 1000.,
            queue_ms: task.queued.elapsed().as_secs_f64() * 1000.,
            preparation_ms: 0.,
            model_load_ms: 0.,
            inference_ms: 0.,
            server_prompt_ms: 0.,
            server_decode_ms: 0.,
            review_ms: None,
            frame_ms: None,
            cache_hit: false,
            submitted: task.submitted,
        }
    }
    pub(super) fn trace(&self, stage: &str) {
        let Some(destination) = std::env::var_os("FOLIO_OCR_TRACE") else {
            return;
        };
        let record = serde_json::json!({"pid": std::process::id(), "stage": stage, "timing": self});
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(destination)
        {
            if let Ok(mut bytes) = serde_json::to_vec(&record) {
                bytes.push(b'\n');
                let _ = file.write_all(&bytes);
            }
        }
    }
}
