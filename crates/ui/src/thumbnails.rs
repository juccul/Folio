//! Bounded page and cover thumbnail cache and async rasterization.
use super::*;
use folio_document::Page;
use std::{collections::HashMap, sync::Arc};

#[derive(Default)]
pub(super) struct Thumbnails {
    entries: HashMap<Id, Thumbnail>,
    covers: HashMap<Id, CachedCover>,
    pending: HashMap<Id, u64>,
    clock: u64,
}
struct CachedCover {
    updated_at: u64,
    cover_page: Option<Id>,
    page_id: Id,
    page_count: usize,
    key: ThumbnailKey,
    bright_raster: bool,
}
#[derive(Clone, PartialEq)]
struct ThumbnailKey {
    revision: u64,
    properties: folio_document::PageProperties,
    theme: super::theme::CanvasTheme,
    dark: bool,
    pdf_ready: bool,
    /// Raster size in device pixels, so covers are drawn at the size shown
    /// instead of being downsampled until thin ink fades.
    edge: u32,
}
impl ThumbnailKey {
    /// Whether an existing thumbnail can stay visible while an updated revision
    /// or higher-resolution preview renders in the background.
    fn can_stand_in(&self, next: &Self) -> bool {
        self.theme == next.theme
            && self.dark == next.dark
            && self.properties.paper == next.properties.paper
            && self.properties.infinite == next.properties.infinite
            && (self.properties.width - next.properties.width).abs() < 1e-3
            && (self.properties.height - next.properties.height).abs() < 1e-3
    }
}
struct Thumbnail {
    key: ThumbnailKey,
    generation: u64,
    image: Option<Arc<RenderImage>>,
    error: Option<String>,
    aspect: f32,
    touched: u64,
}
impl Thumbnails {
    pub(super) fn clear(&mut self, window: &mut Window) {
        if self.entries.is_empty() && self.pending.is_empty() && self.covers.is_empty() {
            return;
        }
        for (_, entry) in self.entries.drain() {
            if let Some(image) = entry.image {
                let _ = window.drop_image(image);
            }
        }
        self.covers.clear();
        self.pending.clear();
    }
    fn touch(&mut self, id: Id, key: &ThumbnailKey) -> (u64, bool) {
        self.clock += 1;
        let touched = self.clock;
        let valid = self.entries.get(&id).is_some_and(|entry| &entry.key == key);
        (touched, valid)
    }
    fn start(
        &mut self,
        id: Id,
        key: ThumbnailKey,
        aspect: f32,
        touched: u64,
        mut release: impl FnMut(Arc<RenderImage>),
    ) {
        if !self.entries.contains_key(&id)
            && self.entries.len() >= 24
            && let Some(oldest) = self
                .entries
                .iter()
                .filter(|(id, _)| !self.pending.contains_key(id))
                .min_by_key(|(_, entry)| entry.touched)
                .map(|(id, _)| *id)
        {
            self.covers.retain(|_, cover| cover.page_id != oldest);
            if let Some(image) = self.entries.remove(&oldest).and_then(|entry| entry.image) {
                release(image);
            }
        }
        let previous = self.entries.remove(&id);
        let (retained_image, retained_aspect) = match previous {
            Some(old) if old.image.is_some() && old.key.can_stand_in(&key) => {
                (old.image, old.aspect)
            }
            Some(old) => {
                if let Some(image) = old.image {
                    release(image);
                }
                (None, aspect)
            }
            None => (None, aspect),
        };
        self.entries.insert(
            id,
            Thumbnail {
                key,
                generation: touched,
                image: retained_image,
                error: None,
                aspect: retained_aspect,
                touched,
            },
        );
        self.pending.insert(id, touched);
    }
    fn finish(
        &mut self,
        id: Id,
        generation: u64,
        result: Result<(Arc<RenderImage>, f32), String>,
        mut release: impl FnMut(Arc<RenderImage>),
    ) -> bool {
        self.pending.remove(&id);
        let Some(entry) = self.entries.get_mut(&id) else {
            if let Ok((image, _)) = result {
                release(image);
            }
            return false;
        };
        if entry.generation != generation {
            if let Ok((image, _)) = result {
                release(image);
            }
            return false;
        }
        match result {
            Ok((image, aspect)) => {
                if let Some(previous) = entry.image.replace(image) {
                    release(previous);
                }
                entry.aspect = aspect;
                entry.error = None;
                true
            }
            Err(error) => {
                entry.error = Some(error);
                false
            }
        }
    }
}

pub(super) enum ThumbnailSource {
    Cover {
        note: Id,
        updated_at: u64,
        cover_page: Option<Id>,
        page: Arc<Page>,
        page_count: usize,
    },
    Current(usize),
}

fn thumbnail_edge(properties: &folio_document::PageProperties, max_height: f32, scale: f32) -> u32 {
    let edge = ((max_height * scale).ceil() as u32).clamp(128, 768);
    // PDF previews are already rasters; resampling them small blurs text.
    if properties.pdf.is_some() {
        edge.max(384)
    } else {
        edge
    }
}

fn thumbnail_frame(
    width: f32,
    max_height: f32,
    aspect: f32,
    corner: f32,
    paper: u32,
) -> Div {
    let height = (width / aspect.max(0.01)).min(max_height);
    div()
        .rounded(px(corner))
        .w(px((height * aspect).min(width)))
        .h(px(height))
        .overflow_hidden()
        .relative()
        .bg(rgb(paper))
        .flex()
        .items_center()
        .justify_center()
}

fn attach_thumbnail_image(
    frame: Div,
    image: Arc<RenderImage>,
    corner: f32,
    bright_raster: bool,
) -> Div {
    // White PDF pages and images would otherwise glare beside dark notebooks;
    // a dark veil brings them to about 80% brightness.
    let veil = bright_raster.then(|| {
        div()
            .absolute()
            .inset_0()
            .rounded(px(corner))
            .bg(rgba(0x00000040))
    });
    frame
        .child(
            img(image)
                .size_full()
                .object_fit(ObjectFit::Contain)
                .rounded(px(corner)),
        )
        .children(veil)
}

impl NotesView {
    pub(super) fn cached_cover_thumbnail(
        &mut self,
        note: &folio_document::NoteMetadata,
        width: f32,
        max_height: f32,
        window: &Window,
    ) -> Option<(Div, usize)> {
        if self.controller.sessions.contains_key(&note.id) {
            return None;
        }
        let cover = self.thumbnails.covers.get(&note.id)?;
        if cover.updated_at != note.updated_at || cover.cover_page != note.cover_page {
            return None;
        }
        let theme = Theme::new(&self.controller.settings);
        let palette = theme.canvas_for_page(&cover.key.properties);
        let pdf_ready = cover
            .key
            .properties
            .pdf
            .as_ref()
            .and_then(|p| p.preview_asset.as_ref())
            .is_none_or(|a| self.controller.assets.join(a).is_file());
        let edge = thumbnail_edge(&cover.key.properties, max_height, window.scale_factor());
        if cover.key.theme != palette
            || cover.key.dark != theme.dark
            || cover.key.pdf_ready != pdf_ready
            || cover.key.edge != edge
        {
            return None;
        }
        let page_id = cover.page_id;
        let page_count = cover.page_count;
        let bright_raster = cover.bright_raster;
        let expected_key = cover.key.clone();
        self.thumbnails.clock += 1;
        let touched = self.thumbnails.clock;
        let entry = self.thumbnails.entries.get_mut(&page_id)?;
        if entry.key != expected_key {
            return None;
        }
        let image = entry.image.clone()?;
        entry.touched = touched;
        let aspect = entry.aspect;
        let corner = (theme.radius - 1.).max(0.);
        let frame = thumbnail_frame(width, max_height, aspect, corner, palette.paper);
        Some((
            attach_thumbnail_image(frame, image, corner, bright_raster),
            page_count,
        ))
    }

    pub(super) fn thumbnail(
        &mut self,
        source: ThumbnailSource,
        width: f32,
        max_height: f32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Div {
        let cover_meta = match &source {
            ThumbnailSource::Cover {
                note,
                updated_at,
                cover_page,
                page_count,
                ..
            } => Some((*note, *updated_at, *cover_page, *page_count)),
            ThumbnailSource::Current(_) => None,
        };
        let page = match &source {
            ThumbnailSource::Cover { page, .. } => page.as_ref(),
            ThumbnailSource::Current(index) => &self.controller.session().document.pages[*index],
        };
        let theme = Theme::new(&self.controller.settings);
        let palette = theme.canvas_for_page(&page.properties);
        let bright_raster = theme.dark
            && (page.properties.pdf.is_some()
                || page
                    .objects
                    .values()
                    .any(|o| matches!(o.as_ref(), folio_document::Object::Image(_))));
        let pdf_ready = page
            .properties
            .pdf
            .as_ref()
            .and_then(|p| p.preview_asset.as_ref())
            .is_none_or(|a| self.controller.assets.join(a).is_file());
        let edge = thumbnail_edge(&page.properties, max_height, window.scale_factor());
        let key = ThumbnailKey {
            revision: page.revision,
            properties: page.properties.clone(),
            theme: palette,
            dark: theme.dark,
            pdf_ready,
            edge,
        };
        let id = page.id;
        let default_aspect = page.properties.width / page.properties.height.max(1.);
        let cover_key = key.clone();
        let (touched, valid) = self.thumbnails.touch(id, &key);
        if !valid && self.thumbnails.pending.len() < 2 && !self.thumbnails.pending.contains_key(&id)
        {
            let generation = touched;
            let aspect = page.properties.width / page.properties.height.max(1.);
            self.thumbnails.start(id, key, aspect, touched, |image| {
                // Dropping the Arc alone leaves its sprite-atlas allocation alive.
                let _ = window.drop_image(image);
            });
            // Snapshot only when a raster job is needed. Cached redraws avoid
            // cloning the entire page/object map, and appearance adaptation runs
            // on the worker with the rasterization.
            let source = match source {
                ThumbnailSource::Cover { page, .. } => page,
                ThumbnailSource::Current(_) => Arc::new(page.clone()),
            };
            let assets = self.controller.assets.clone();
            let task = cx.background_executor().spawn(async move {
                let mut page = Arc::unwrap_or_clone(source);
                export_options::apply(&mut page, theme, export_options::Appearance::Visible);
                let palette = theme.canvas_for_page(&page.properties);
                for object in page.objects.values_mut() {
                    match object.as_ref() {
                        folio_document::Object::Stroke(stroke) => {
                            let adapted = folio_document::Color::from_rgb(
                                palette.preview_ink(stroke.style.color.rgb()),
                            );
                            if stroke.style.color != adapted
                                && let folio_document::Object::Stroke(s) = Arc::make_mut(object)
                            {
                                s.style.color = adapted;
                            }
                        }
                        folio_document::Object::Text(text) => {
                            let adapted = folio_document::Color::from_rgb(
                                palette.preview_ink(text.color.rgb()),
                            );
                            if text.color != adapted
                                && let folio_document::Object::Text(t) = Arc::make_mut(object)
                            {
                                t.color = adapted;
                            }
                        }
                        _ => {}
                    }
                }
                folio_export::raster_page_thumbnail(&page, &assets, edge)
                    .map(|p| {
                        let aspect = p.width() as f32 / p.height() as f32;
                        (graph::image(p.width(), p.height(), p.take()), aspect)
                    })
                    .map_err(|e| e.to_string())
            });
            cx.spawn(async move |view, cx| {
                let result = task.await;
                let _ = view.update(cx, |view, cx| {
                    let installed = view.thumbnails.finish(id, generation, result, |image| {
                        cx.drop_image(image, None);
                    });
                    if installed
                        && let Some((note, updated_at, cover_page, page_count)) = cover_meta
                    {
                        view.thumbnails.covers.insert(
                            note,
                            CachedCover {
                                updated_at,
                                cover_page,
                                page_id: id,
                                page_count,
                                key: cover_key,
                                bright_raster,
                            },
                        );
                        view.controller.release_library_preview(note);
                    }
                    cx.notify();
                });
            })
            .detach();
        }
        let entry = self.thumbnails.entries.get_mut(&id);
        let (image, error, aspect) = if let Some(entry) = entry {
            entry.touched = touched;
            (entry.image.clone(), entry.error.clone(), entry.aspect)
        } else {
            (None, None, default_aspect)
        };
        let corner = (theme.radius - 1.).max(0.);
        let frame = thumbnail_frame(width, max_height, aspect, corner, palette.paper);
        if let Some(image) = image {
            attach_thumbnail_image(frame, image, corner, bright_raster)
        } else {
            frame.child(
                div()
                    .p_2()
                    .text_xs()
                    .text_color(rgb(palette.foreground))
                    .child(if error.is_some() {
                        "Preview unavailable"
                    } else {
                        "Rendering preview…"
                    }),
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(page: &Page) -> ThumbnailKey {
        ThumbnailKey {
            revision: page.revision,
            properties: page.properties.clone(),
            theme: Theme::new(&folio_app::Settings::default()).canvas,
            dark: false,
            pdf_ready: true,
            edge: 384,
        }
    }

    #[::core::prelude::v1::test]
    fn thumbnail_cache_keys_invalidate_content_properties_theme_and_pdf_readiness() {
        let page = Page::new();
        let original = key(&page);
        let mut cache = Thumbnails::default();
        let mut atlas = std::collections::HashSet::new();
        cache.start(page.id, original.clone(), 1., 1, |_| {
            panic!("A new cache entry cannot retire an image")
        });
        cache.pending.remove(&page.id);
        let image = graph::image(1, 1, vec![0; 4]);
        let old_image = Arc::downgrade(&image);
        atlas.insert(image.id);
        cache.entries.get_mut(&page.id).unwrap().image = Some(image);
        assert!(cache.touch(page.id, &original).1);
        for change in [
            |k: &mut ThumbnailKey| k.revision += 1,
            |k: &mut ThumbnailKey| k.properties.width += 10.,
            |k: &mut ThumbnailKey| k.theme.paper ^= 0xffffff,
            |k: &mut ThumbnailKey| k.dark = true,
            |k: &mut ThumbnailKey| k.pdf_ready = false,
        ] {
            let mut changed = original.clone();
            change(&mut changed);
            assert!(!cache.touch(page.id, &changed).1);
        }
        assert!(!cache.touch(Id::new_v4(), &original).1);
        // A revision-only update keeps the previous thumbnail visible while the
        // replacement renders, retiring the old atlas image once the new one arrives.
        let mut changed = original.clone();
        changed.revision += 1;
        let (generation, _) = cache.touch(page.id, &changed);
        cache.start(page.id, changed.clone(), 1., generation, |_| {
            panic!("Revision refresh should keep the stand-in thumbnail until completion")
        });
        assert!(old_image.upgrade().is_some());
        assert!(cache.touch(page.id, &changed).1);
        let replacement = graph::image(1, 1, vec![0; 4]);
        let replacement_weak = Arc::downgrade(&replacement);
        atlas.insert(replacement.id);
        assert!(cache.finish(
            page.id,
            generation,
            Ok((replacement, 1.)),
            |image| assert!(atlas.remove(&image.id), "Image retired more than once"),
        ));
        assert!(old_image.upgrade().is_none());
        assert!(replacement_weak.upgrade().is_some());
        assert_eq!(atlas.len(), 1);

        // An incompatible theme change retires the old image immediately.
        let mut dark_changed = changed.clone();
        dark_changed.dark = true;
        let (generation, _) = cache.touch(page.id, &dark_changed);
        cache.start(page.id, dark_changed.clone(), 1., generation, |image| {
            assert!(atlas.remove(&image.id), "Image retired more than once");
        });
        assert!(replacement_weak.upgrade().is_none());
        assert!(atlas.is_empty());

        // Scrolling through many covers retires old atlas allocations while
        // keeping an in-flight replacement pinned until its result arrives.
        for _ in 0..80 {
            let next = Id::new_v4();
            let (touched, _) = cache.touch(next, &original);
            cache.start(next, original.clone(), 1., touched, |image| {
                assert!(atlas.remove(&image.id), "Image retired more than once");
            });
            let image = graph::image(1, 1, vec![0; 4]);
            atlas.insert(image.id);
            cache.entries.get_mut(&next).unwrap().image = Some(image);
            cache.pending.remove(&next);
        }
        assert!(cache.entries.contains_key(&page.id));
        assert_eq!(cache.pending.get(&page.id), Some(&generation));
        assert_eq!(
            atlas,
            cache
                .entries
                .values()
                .filter_map(|entry| entry.image.as_ref().map(|image| image.id))
                .collect(),
            "Evicted covers must not leave sprite-atlas images behind"
        );
        assert!(atlas.len() < 80, "Scrolling must evict old cover images");
    }
}
