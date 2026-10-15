//! Rejoin immutable ink buffers as records are decoded, without retaining a
//! second owner or truncating durable undo history.
use folio_document::*;
use std::{
    collections::{HashMap, hash_map::DefaultHasher},
    hash::{Hash, Hasher},
    sync::{Arc, Weak},
};

type Pool<T> = HashMap<(usize, u64), Vec<Weak<Vec<T>>>>;
trait Sample {
    type Bits: Hash + Eq;
    fn bits(&self) -> Self::Bits;
}
impl Sample for StrokePoint {
    type Bits = ([u32; 5], u64, u32);
    fn bits(&self) -> Self::Bits {
        (
            [self.x, self.y, self.pressure, self.tilt_x, self.tilt_y].map(f32::to_bits),
            self.timestamp,
            self.buttons,
        )
    }
}
impl Sample for PathPoint {
    type Bits = [u32; 3];
    fn bits(&self) -> Self::Bits {
        [self.position.x, self.position.y, self.radius].map(f32::to_bits)
    }
}
fn intern<T: Sample>(pool: &mut Pool<T>, buffer: &mut Arc<Vec<T>>) {
    if buffer.is_empty() {
        return;
    }
    let mut hash = DefaultHasher::new();
    for sample in buffer.iter() {
        sample.bits().hash(&mut hash);
    }
    let candidates = pool.entry((buffer.len(), hash.finish())).or_default();
    candidates.retain(|candidate| candidate.strong_count() > 0);
    for candidate in candidates.iter().filter_map(Weak::upgrade) {
        // Check exact bits after hashing, including signed zero and sensor data.
        if Arc::ptr_eq(&candidate, buffer)
            || candidate
                .iter()
                .zip(buffer.iter())
                .all(|(a, b)| a.bits() == b.bits())
        {
            *buffer = candidate;
            return;
        }
    }
    candidates.push(Arc::downgrade(buffer));
}

#[derive(Default)]
pub(super) struct StrokeInterner {
    raw: Pool<StrokePoint>,
    path: Pool<PathPoint>,
}
impl StrokeInterner {
    pub fn object(&mut self, object: &mut Object) {
        if let Object::Stroke(stroke) = object {
            intern(&mut self.raw, &mut stroke.raw);
            intern(&mut self.path, &mut stroke.path);
        }
    }
    pub fn seed(&mut self, document: &Document) {
        for object in document.pages.iter().flat_map(|page| page.objects.values()) {
            if let Object::Stroke(stroke) = object.as_ref() {
                intern(&mut self.raw, &mut stroke.raw.clone());
                intern(&mut self.path, &mut stroke.path.clone());
            }
        }
    }
    pub fn command(&mut self, command: &mut Command) {
        let mut object = |object: &mut Arc<Object>| {
            if matches!(object.as_ref(), Object::Stroke(_)) {
                self.object(Arc::make_mut(object));
            }
        };
        for change in &mut command.changes {
            match change {
                Change::Object { before, after, .. } => {
                    for value in before.iter_mut().chain(after.iter_mut()) {
                        object(value);
                    }
                }
                Change::Page { before, after, .. } => {
                    for page in before.iter_mut().chain(after.iter_mut()) {
                        for value in page.objects.values_mut() {
                            object(value);
                        }
                    }
                }
                _ => {}
            }
        }
    }
}
