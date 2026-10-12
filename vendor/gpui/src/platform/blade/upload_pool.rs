//! Frame-scoped upload storage. Capacity follows the recent working set instead
//! of retaining every smaller allocation made while a scene grows.

struct Chunk<B, S> {
    buffer: B,
    capacity: u64,
    offset: u64,
    sync: Option<S>,
    last_used: u64,
}

pub(super) struct UploadPool<B, S> {
    chunks: Vec<Chunk<B, S>>,
    frame: u64,
    minimum: u64,
    alignment: u64,
}

impl<B: Copy, S: Clone> UploadPool<B, S> {
    pub fn new(minimum: u64, alignment: u64) -> Self {
        assert!(minimum > 0 && alignment > 0);
        Self {
            chunks: Vec::new(),
            frame: 0,
            minimum,
            alignment,
        }
    }

    pub fn allocate(
        &mut self,
        size: u64,
        mut create: impl FnMut(u64) -> B,
        mut completed: impl FnMut(&S) -> bool,
        mut destroy: impl FnMut(B),
    ) -> (B, u64) {
        assert!(size > 0);
        for chunk in &mut self.chunks {
            let offset = chunk.offset.next_multiple_of(self.alignment);
            if chunk.sync.is_none() && size <= chunk.capacity.saturating_sub(offset) {
                chunk.offset = offset + size;
                return (chunk.buffer, offset);
            }
        }
        let reusable = self
            .chunks
            .iter()
            .enumerate()
            .filter(|(_, c)| c.capacity >= size && c.sync.as_ref().is_some_and(&mut completed))
            .min_by_key(|(_, c)| c.capacity)
            .map(|(i, _)| i);
        if let Some(index) = reusable {
            let chunk = &mut self.chunks[index];
            chunk.sync = None;
            chunk.offset = size;
            chunk.last_used = self.frame;
            return (chunk.buffer, 0);
        }
        // The scene outgrew the reusable buffers. Release completed smaller
        // chunks before allocating their replacement; never touch in-flight data.
        self.chunks.retain(|chunk| {
            if chunk.capacity < size && chunk.sync.as_ref().is_some_and(&mut completed) {
                destroy(chunk.buffer);
                false
            } else {
                true
            }
        });
        let capacity = size
            .max(self.minimum)
            .checked_next_power_of_two()
            .unwrap_or(size);
        let buffer = create(capacity);
        self.chunks.push(Chunk {
            buffer,
            capacity,
            offset: size,
            sync: None,
            last_used: self.frame,
        });
        (buffer, 0)
    }

    pub fn finish(
        &mut self,
        sync: &S,
        mut completed: impl FnMut(&S) -> bool,
        mut destroy: impl FnMut(B),
    ) {
        for chunk in &mut self.chunks {
            if chunk.sync.is_none() {
                chunk.sync = Some(sync.clone());
                chunk.last_used = self.frame;
            }
        }
        // Allow two frames for reuse. Large scenes and texture uploads should
        // relinquish their spare capacity once the user leaves that working set.
        self.chunks.retain(|chunk| {
            if self.frame.saturating_sub(chunk.last_used) > 2
                && chunk.sync.as_ref().is_some_and(&mut completed)
            {
                destroy(chunk.buffer);
                false
            } else {
                true
            }
        });
        self.frame += 1;
    }

    /// Caller must first wait for all submitted work.
    pub fn destroy(&mut self, mut destroy: impl FnMut(B)) {
        for chunk in self.chunks.drain(..) {
            destroy(chunk.buffer);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::collections::HashMap;

    #[test]
    fn growing_ink_uploads_and_shrinking_working_sets_release_memory() {
        let allocations = RefCell::new(HashMap::new());
        let mut next_id = 0;
        let mut pool = UploadPool::new(4096, 64);
        for vertices in 1..=4096 {
            let size = vertices * 192;
            pool.allocate(
                size,
                |capacity| {
                    next_id += 1;
                    allocations.borrow_mut().insert(next_id, capacity);
                    next_id
                },
                |_: &bool| true,
                |id| {
                    allocations.borrow_mut().remove(&id);
                },
            );
            pool.finish(
                &true,
                |_| true,
                |id| {
                    allocations.borrow_mut().remove(&id);
                },
            );
            // Includes recently unused chunks, not the sum of all prior sizes.
            assert!(allocations.borrow().values().sum::<u64>() <= 4 * size.max(4096));
        }
        let peak = allocations.borrow().values().sum::<u64>();
        for _ in 0..4 {
            pool.finish(
                &true,
                |_| true,
                |id| {
                    allocations.borrow_mut().remove(&id);
                },
            );
        }
        assert!(peak > 0);
        assert!(allocations.borrow().is_empty());
    }

    #[test]
    fn alignment_reuse_and_retirement_respect_gpu_completion() {
        let mut pool = UploadPool::new(256, 64);
        let mut next_id = 0;
        let destroyed = RefCell::new(Vec::new());
        let mut allocate = |pool: &mut UploadPool<usize, u64>, size, completed_frame| {
            pool.allocate(
                size,
                |_| {
                    next_id += 1;
                    next_id
                },
                |sync| *sync <= completed_frame,
                |id| destroyed.borrow_mut().push(id),
            )
        };
        assert_eq!(allocate(&mut pool, 65, 0), (1, 0));
        assert_eq!(allocate(&mut pool, 100, 0), (1, 128));
        pool.finish(&1, |_| false, |_| panic!("in-flight buffer retired"));
        assert_eq!(allocate(&mut pool, 65, 0), (2, 0));
        pool.finish(&2, |_| false, |_| panic!("in-flight buffer retired"));
        assert_eq!(allocate(&mut pool, 65, 1), (1, 0));
        pool.finish(&3, |_| false, |_| panic!("in-flight buffer retired"));
        assert_eq!(allocate(&mut pool, 1024, 2), (3, 0));
        assert_eq!(*destroyed.borrow(), vec![2]);
        pool.finish(&4, |_| false, |_| panic!("in-flight buffer retired"));
        for _ in 0..4 {
            pool.finish(&5, |_| false, |_| panic!("in-flight buffer retired"));
        }
        pool.finish(&6, |_| true, |id| destroyed.borrow_mut().push(id));
        assert!(pool.chunks.is_empty());
        assert_eq!(destroyed.borrow().len(), 3);
    }
}
