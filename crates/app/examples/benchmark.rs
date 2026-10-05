//! Reproducible CPU geometry/storage benchmark; not a hardware-to-photon claim.
use folio_canvas::SpatialIndex;
use folio_document::*;
use folio_ink::{StrokeBuilder, outline_chunk};
use folio_storage::{Delta, Store};
use std::{sync::Arc, time::Instant};
fn distribution(mut values: Vec<f64>) -> serde_json::Value {
    values.sort_by(f64::total_cmp);
    let p = |v: f64| values[((values.len() - 1) as f64 * v) as usize];
    serde_json::json!({"p50_us":p(0.5),"p95_us":p(0.95),"p99_us":p(0.99),"max_us":p(1.)})
}
fn main() {
    let count = std::env::args()
        .nth(1)
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(10_000)
        .clamp(100, 100_000);
    let mut builder = StrokeBuilder::new(PenStyle::default());
    let mut ink_times = Vec::new();
    let mut cached_chunks = 0;
    for i in 0..12_000 {
        let now = Instant::now();
        builder.push(StrokePoint::new(
            Point::new(i as f32 * 1.2, (i as f32 * 0.09).sin() * 16.),
            0.2 + (i % 100) as f32 * 0.008,
            i * 8,
        ));
        let path = builder.path();
        let count = path.len().saturating_sub(2) / 256;
        while cached_chunks < count {
            std::hint::black_box(outline_chunk(
                path,
                cached_chunks * 256,
                (cached_chunks + 1) * 256,
            ));
            cached_chunks += 1;
        }
        if path.len() > 1 {
            std::hint::black_box(outline_chunk(path, cached_chunks * 256, path.len() - 1));
        }
        ink_times.push(now.elapsed().as_secs_f64() * 1e6);
    }
    let mut document = Document::new("10k ink benchmark");
    let page = &mut document.pages[0];
    page.properties.infinite = true;
    for n in 0..count {
        let mut ink = StrokeBuilder::new(PenStyle::default());
        for i in 0..48 {
            ink.push(StrokePoint::new(
                Point::new(
                    (n % 100) as f32 * 160. + i as f32 * 1.6,
                    (n / 100) as f32 * 64. + (i as f32 * 0.2).sin() * 12.,
                ),
                0.4 + i as f32 / 100.,
                i * 8,
            ));
        }
        let object = Arc::new(Object::Stroke(ink.finish().unwrap()));
        page.order.push(object.id());
        page.objects.insert(object.id(), object);
    }
    let mut index = SpatialIndex::default();
    let start = Instant::now();
    index.rebuild(page);
    let indexing = start.elapsed();
    let mut queries = Vec::new();
    let mut updates = Vec::new();
    for n in 0..1000 {
        let page = &document.pages[0];
        let start = Instant::now();
        std::hint::black_box(index.query(Rect::new(
            (n % 20) as f32 * 600.,
            (n / 20) as f32 * 64.,
            1200.,
            800.,
        )));
        queries.push(start.elapsed().as_secs_f64() * 1e6);
        let id = page.order[n % count];
        let before = page.objects[&id].clone();
        let mut after = before.as_ref().clone();
        after.set_transform(Transform::translate(2., 3.));
        let command = Command {
            label: "Move".into(),
            changes: vec![Change::Object {
                page: page.id,
                id,
                before: Some(before),
                after: Some(Arc::new(after)),
                index: n % count,
            }],
        };
        let start = Instant::now();
        command.apply(&mut document, true);
        index.update(&document.pages[0], &command);
        updates.push(start.elapsed().as_secs_f64() * 1e6);
    }
    let root = std::env::temp_dir().join(format!("folio-benchmark-{}", Id::new_v4()));
    let mut store = Store::open(root.join("notes.sqlite3")).unwrap();
    let start = Instant::now();
    store.save(&Delta::full(&document)).unwrap();
    let save = start.elapsed();
    let start = Instant::now();
    assert_eq!(
        store.load(document.metadata.id).unwrap().unwrap().pages[0]
            .objects
            .len(),
        count
    );
    let load = start.elapsed();
    println!("{}",serde_json::to_string_pretty(&serde_json::json!({"measurement":"CPU only; excludes GPUI tessellation, GPU presentation, compositor and physical tablet latency","strokes":count,"active_ink_push_and_chunk":distribution(ink_times),"spatial_query":distribution(queries),"incremental_command_and_index":distribution(updates),"initial_index_ms":indexing.as_secs_f64()*1000.,"initial_save_ms":save.as_secs_f64()*1000.,"load_ms":load.as_secs_f64()*1000.})).unwrap());
    drop(store);
    std::fs::remove_dir_all(root).unwrap();
}
