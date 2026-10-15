//! Keep GPU resources and their identities alive until their last frame ends.
pub(super) struct RetirementQueue<T, S>(Vec<(T, S)>);

impl<T, S> RetirementQueue<T, S> {
    pub fn new() -> Self {
        Self(Vec::new())
    }
    pub fn push(&mut self, resource: T, sync: S) {
        self.0.push((resource, sync));
    }
    pub fn collect(&mut self, mut completed: impl FnMut(&S) -> bool, mut release: impl FnMut(T)) {
        let mut index = 0;
        while index < self.0.len() {
            if completed(&self.0[index].1) {
                release(self.0.swap_remove(index).0);
            } else {
                index += 1;
            }
        }
    }
    /// The caller must first wait for all submitted work.
    pub fn destroy(&mut self, mut release: impl FnMut(T)) {
        for (resource, _) in self.0.drain(..) {
            release(resource);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn image_churn_releases_resources_and_identities_only_after_their_frames() {
        let mut retired = RetirementQueue::new();
        let mut free = Vec::new();
        retired.push(1, 10);
        retired.push(2, 11);
        retired.push(3, 10);
        retired.collect(|frame| *frame <= 9, |id| free.push(id));
        assert!(
            free.is_empty(),
            "In-flight texture identities cannot be reused"
        );
        retired.collect(|frame| *frame <= 10, |id| free.push(id));
        free.sort_unstable();
        assert_eq!(free, [1, 3]);
        retired.collect(|frame| *frame <= 10, |_| panic!("Resource released twice"));
        retired.collect(|frame| *frame <= 11, |id| free.push(id));
        assert_eq!(free, [1, 3, 2]);
        retired.destroy(|_| panic!("Completed resource survived retirement"));
    }

    #[test]
    fn shutdown_releases_waited_resources_without_another_frame() {
        let mut retired = RetirementQueue::new();
        retired.push(String::from("preview"), false);
        retired.collect(|completed| *completed, |_| panic!("Premature release"));
        let mut released = Vec::new();
        // Simulate the renderer's final GPU wait before destruction.
        retired.destroy(|resource| released.push(resource));
        assert_eq!(released, ["preview"]);
        retired.destroy(|_| panic!("Resource released twice"));
    }
}
