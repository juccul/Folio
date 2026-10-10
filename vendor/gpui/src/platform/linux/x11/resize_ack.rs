/// Identify the newest rendered resize without comparing WM-owned counter values.
#[derive(Default)]
pub(super) struct ResizeAcknowledgement {
    generation: u64,
}

impl ResizeAcknowledgement {
    pub(super) fn submitted(&mut self) -> u64 {
        self.generation = self.generation.wrapping_add(1);
        self.generation
    }

    pub(super) fn is_current(&self, generation: u64) -> bool {
        self.generation == generation
    }
}

#[cfg(test)]
mod tests {
    use super::ResizeAcknowledgement;

    #[test]
    fn newer_render_supersedes_older_gpu_completion() {
        let mut acknowledgement = ResizeAcknowledgement::default();
        let first = acknowledgement.submitted();
        assert!(acknowledgement.is_current(first));
        let second = acknowledgement.submitted();
        assert!(!acknowledgement.is_current(first));
        assert!(acknowledgement.is_current(second));
        // A late callback from the older frame cannot roll the WM counter back.
        assert!(!acknowledgement.is_current(first));
    }
}
