//! Small session-only result cache. Weak source identities do not retain ink buffers.
use super::*;
use sha2::{Digest, Sha256};
use std::{collections::VecDeque, sync::Weak};

pub(super) struct Identity {
    digest: [u8; 32],
    sources: Vec<Weak<Object>>,
}
impl Identity {
    pub fn new(task: &Task, pack: &[u8]) -> Option<Self> {
        if task.review.sources.len() > 512 {
            return None;
        }
        let mut hash = Sha256::new();
        hash.update(b"folio-recognition-v3");
        hash.update(task.pack.as_os_str().as_encoded_bytes());
        hash.update(pack);
        hash.update(task.review.note.as_bytes());
        hash.update(task.review.page.as_bytes());
        hash.update([task.review.kind as u8]);
        let mut sources = Vec::with_capacity(task.review.sources.len());
        for object in &task.review.sources {
            hash.update(object.id().as_bytes());
            hash.update((Arc::as_ptr(object) as usize).to_le_bytes());
            sources.push(Arc::downgrade(object));
        }
        match &task.image {
            Some(ImageSource::Object(object, assets)) => {
                let Object::Image(image) = object.as_ref() else {
                    return None;
                };
                stamp(
                    &mut hash,
                    &folio_export::asset_path(assets, &image.asset).ok()?,
                )?;
            }
            Some(ImageSource::Pdf(pdf, assets, crop)) => {
                hash.update(serde_json::to_vec(pdf).ok()?);
                for value in [crop.min.x, crop.min.y, crop.max.x, crop.max.y] {
                    hash.update(value.to_bits().to_le_bytes());
                }
                stamp(
                    &mut hash,
                    &folio_export::asset_path(assets, &pdf.asset).ok()?,
                )?;
            }
            None => {}
        }
        Some(Self {
            digest: hash.finalize().into(),
            sources,
        })
    }
    fn alive(&self) -> bool {
        self.sources.iter().all(|source| source.strong_count() > 0)
    }
}
fn stamp(hash: &mut Sha256, path: &std::path::Path) -> Option<()> {
    let metadata = std::fs::metadata(path).ok()?;
    let modified = metadata
        .modified()
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?;
    hash.update(path.as_os_str().as_encoded_bytes());
    hash.update(metadata.len().to_le_bytes());
    hash.update(modified.as_nanos().to_le_bytes());
    Some(())
}
#[derive(Default)]
pub(super) struct Results(VecDeque<(Identity, String)>);
impl Results {
    pub fn get(&mut self, key: &Identity) -> Option<String> {
        self.0.retain(|(key, _)| key.alive());
        let index = self
            .0
            .iter()
            .position(|(old, _)| old.digest == key.digest)?;
        let entry = self.0.remove(index)?;
        let text = entry.1.clone();
        self.0.push_back(entry);
        Some(text)
    }
    pub fn insert(&mut self, key: Identity, text: String) {
        self.0
            .retain(|(old, _)| old.alive() && old.digest != key.digest);
        self.0.push_back((key, text));
        while self.0.len() > 8
            || self
                .0
                .iter()
                .map(|(key, _)| key.sources.len())
                .sum::<usize>()
                > 512
            || self.0.iter().map(|(_, text)| text.len()).sum::<usize>() > 256 * 1024
        {
            self.0.pop_front();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_does_not_retain_source_buffers_and_rejects_new_versions() {
        let make_task = || {
            let object = Arc::new(Object::Text(TextBlock {
                id: Id::new_v4(),
                text: "large source".repeat(10000),
                rect: Rect::new(0., 0., 100., 20.),
                transform: Transform::default(),
                font_family: "sans-serif".into(),
                font_size: 16.,
                color: Color::from_rgb(0),
                bold: false,
                italic: false,
                underline: false,
                alignment: Alignment::Left,
                list: ListStyle::None,
            }));
            Task {
                submitted: Instant::now(),
                queued: Instant::now(),
                generation: 1,
                pack: "pack.json".into(),
                auto_setup: false,
                image: None,
                review: RecognitionReview {
                    kind: RecognitionKind::Text,
                    text: String::new(),
                    note: Id::new_v4(),
                    page: Id::new_v4(),
                    bounds: object.bounds(),
                    sources: vec![object],
                    pdf_source: None,
                },
            }
        };
        let mut cache = Results::default();
        let task = make_task();
        let weak = Arc::downgrade(&task.review.sources[0]);
        let lookup = Identity::new(&task, b"model-v1").unwrap();
        cache.insert(
            Identity::new(&task, b"model-v1").unwrap(),
            "recognized".into(),
        );
        assert_eq!(cache.get(&lookup).as_deref(), Some("recognized"));
        assert!(
            cache
                .get(&Identity::new(&task, b"model-v2").unwrap())
                .is_none()
        );
        let mut edited = task.clone();
        edited.review.sources[0] = Arc::new(task.review.sources[0].as_ref().clone());
        assert!(
            cache
                .get(&Identity::new(&edited, b"model-v1").unwrap())
                .is_none()
        );
        drop(edited);
        drop(task);
        assert_eq!(
            weak.strong_count(),
            0,
            "Caching must release source strings and ink buffers"
        );
        assert!(cache.get(&lookup).is_none());
        let mut retained = Vec::new();
        for _ in 0..32 {
            let task = make_task();
            cache.insert(
                Identity::new(&task, b"model-v1").unwrap(),
                "recognized".into(),
            );
            retained.push(task);
        }
        assert!(
            cache
                .get(&Identity::new(&retained[0], b"model-v1").unwrap())
                .is_none()
        );
        assert_eq!(
            cache
                .get(&Identity::new(retained.last().unwrap(), b"model-v1").unwrap())
                .as_deref(),
            Some("recognized")
        );
    }
}
