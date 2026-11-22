//! Rejoin immutable ink buffers as records are decoded, without retaining a
//! second owner or truncating durable undo history.
use folio_document::*;
use std::{
    collections::{HashMap, hash_map::DefaultHasher},
    hash::{Hash, Hasher},
    sync::{Arc, Weak},
};

type Pool<T> = HashMap<(usize, u64), Vec<Weak<T>>>;
trait SampleBuffer {
    fn sample_len(&self) -> usize;
    fn sample_bytes(&self) -> &[u8];
}
impl SampleBuffer for RawSamples {
    fn sample_len(&self) -> usize {
        self.len()
    }
    fn sample_bytes(&self) -> &[u8] {
        self.as_bytes()
    }
}
impl SampleBuffer for PathSamples {
    fn sample_len(&self) -> usize {
        self.len()
    }
    fn sample_bytes(&self) -> &[u8] {
        self.as_bytes()
    }
}
fn intern<T: SampleBuffer>(pool: &mut Pool<T>, buffer: &mut Arc<T>) {
    if buffer.sample_len() == 0 {
        return;
    }
    let bytes = buffer.sample_bytes();
    let mut hash = DefaultHasher::new();
    bytes.hash(&mut hash);
    let candidates = pool.entry((buffer.sample_len(), hash.finish())).or_default();
    candidates.retain(|candidate| candidate.strong_count() > 0);
    for candidate in candidates.iter().filter_map(Weak::upgrade) {
        // Canonical compressed bytes match if and only if every bit matches,
        // including signed zero and sensor metadata.
        if Arc::ptr_eq(&candidate, buffer) || candidate.sample_bytes() == bytes {
            *buffer = candidate;
            return;
        }
    }
    candidates.push(Arc::downgrade(buffer));
}

#[derive(Default)]
pub(super) struct StrokeInterner {
    raw: Pool<RawSamples>,
    path: Pool<PathSamples>,
}
impl StrokeInterner {
    pub fn object(&mut self, object: &mut Object) {
        match object {
            Object::Stroke(stroke) => {
                if let Some(path) = &mut stroke.fragment_path {
                    path.shrink_to_fit();
                }
                if let Some(path) = &mut stroke.refined_path {
                    path.shrink_to_fit();
                }
                intern(&mut self.raw, &mut stroke.raw);
                intern(&mut self.path, &mut stroke.path);
            }
            Object::Shape(shape) => {
                shape.vertices.shrink_to_fit();
                shape.source_strokes.shrink_to_fit();
            }
            Object::Text(text) => {
                text.text.shrink_to_fit();
                text.font_family.shrink_to_fit();
            }
            Object::Image(image) => {
                image.asset.shrink_to_fit();
            }
            Object::Equation(equation) => {
                equation.latex.shrink_to_fit();
                if let Some(svg) = &mut equation.rendered_svg {
                    svg.shrink_to_fit();
                }
                equation.source_strokes.shrink_to_fit();
            }
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
