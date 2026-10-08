//! Offline exports: SVG/PNG and multi-page PDF with vector pressure outlines.
mod pdf;
use base64::Engine;
use folio_document::*;
use folio_ink::outline;
use std::{
    fmt::Write as _,
    io::Write as _,
    path::{Path, PathBuf},
};
#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("File system: {0}")]
    Io(#[from] std::io::Error),
    #[error("SVG rendering: {0}")]
    Svg(#[from] resvg::usvg::Error),
    #[error("Image encoding: {0}")]
    Image(#[from] image::ImageError),
    #[error("{0}")]
    Invalid(String),
}
pub type Result<T> = std::result::Result<T, Error>;
pub fn asset_path(root: &Path, name: &str) -> Result<PathBuf> {
    let p = Path::new(name);
    if p.is_absolute()
        || p.components()
            .any(|p| !matches!(p, std::path::Component::Normal(_)))
    {
        return Err(Error::Invalid("Invalid asset path".into()));
    }
    Ok(root.join(p))
}
pub fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}
fn matrix(t: Transform) -> String {
    format!("matrix({} {} {} {} {} {})", t.a, t.b, t.c, t.d, t.tx, t.ty)
}
fn polygon(points: &[Point]) -> String {
    let mut s = String::new();
    for (i, p) in points.iter().enumerate() {
        let _ = write!(s, "{}{} {} ", if i == 0 { "M" } else { "L" }, p.x, p.y);
    }
    s.push('Z');
    s
}
pub fn object_svg(object: &Object, assets: &Path) -> Result<String> {
    let t = object.transform();
    let body = match object {
        Object::Stroke(s) => format!(
            "<path d=\"{}\" fill=\"{}\" fill-rule=\"nonzero\" opacity=\"{}\"/>",
            outline(s.display_path())
                .iter()
                .map(|c| polygon(c))
                .collect::<String>(),
            s.style.color.hex(),
            s.style.opacity
        ),
        Object::Shape(s) => {
            let mut d = String::new();
            for (i, p) in s.vertices.iter().enumerate() {
                let _ = write!(d, "{}{} {} ", if i == 0 { "M" } else { "L" }, p.x, p.y);
            }
            format!(
                "<path d=\"{d}\" fill=\"none\" stroke=\"{}\" stroke-width=\"{}\" stroke-linecap=\"round\" stroke-linejoin=\"round\" opacity=\"{}\"/>",
                s.style.color.hex(),
                s.style.width,
                s.style.opacity
            )
        }
        Object::Text(o) => {
            let mut body = format!(
                "<text font-family=\"{}\" font-size=\"{}\" fill=\"{}\" font-weight=\"{}\" font-style=\"{}\" text-decoration=\"{}\">",
                escape(&o.font_family),
                o.font_size,
                o.color.hex(),
                if o.bold { "bold" } else { "normal" },
                if o.italic { "italic" } else { "normal" },
                if o.underline { "underline" } else { "none" }
            );
            let (anchor, x) = match o.alignment {
                Alignment::Left => ("start", o.rect.min.x),
                Alignment::Center => ("middle", o.rect.center().x),
                Alignment::Right => ("end", o.rect.max.x),
            };
            for (i, line) in text_lines(o).iter().enumerate() {
                let prefix = match o.list {
                    ListStyle::None => String::new(),
                    ListStyle::Bullet => "• ".into(),
                    ListStyle::Numbered => format!("{}. ", i + 1),
                };
                let _ = write!(
                    body,
                    "<tspan x=\"{x}\" y=\"{}\" text-anchor=\"{anchor}\" xml:space=\"preserve\">{}{}</tspan>",
                    o.rect.min.y + o.font_size * (1. + i as f32 * 1.4),
                    escape(&prefix),
                    escape(line)
                );
            }
            body.push_str("</text>");
            body
        }
        Object::Image(o) => {
            if let Some(crop) = o.crop {
                let r = o.rect;
                let source = Rect::new(
                    r.min.x - crop.min.x * r.width() / crop.width(),
                    r.min.y - crop.min.y * r.height() / crop.height(),
                    r.width() / crop.width(),
                    r.height() / crop.height(),
                );
                let image = image_svg(&o.asset, source, assets)?;
                format!(
                    "<svg x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" viewBox=\"{} {} {} {}\" overflow=\"hidden\">{image}</svg>",
                    r.min.x,
                    r.min.y,
                    r.width(),
                    r.height(),
                    r.min.x,
                    r.min.y,
                    r.width(),
                    r.height()
                )
            } else {
                image_svg(&o.asset, o.rect, assets)?
            }
        }
        Object::Equation(e) => {
            if let Some(svg) = &e.rendered_svg {
                let encoded = base64::engine::general_purpose::STANDARD.encode(svg);
                format!(
                    "<image x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" href=\"data:image/svg+xml;base64,{encoded}\"/>",
                    e.rect.min.x,
                    e.rect.min.y,
                    e.rect.width(),
                    e.rect.height()
                )
            } else {
                format!(
                    "<text x=\"{}\" y=\"{}\" font-size=\"24\" fill=\"#2b3934\">{}</text>",
                    e.rect.min.x,
                    e.rect.min.y + 24.,
                    escape(&e.latex)
                )
            }
        }
    };
    Ok(format!("<g transform=\"{}\">{body}</g>", matrix(t)))
}
/// Word wrapping uses the same local font shaping as SVG/PDF export. Explicit
/// newlines remain paragraph boundaries; long individual words are retained.
fn text_lines(o: &TextBlock) -> Vec<String> {
    let width = o.rect.width().max(o.font_size);
    let measure = |text: &str| {
        let svg = format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"1\" height=\"1\"><text y=\"100\" font-family=\"{}\" font-size=\"{}\" font-weight=\"{}\" font-style=\"{}\">{}</text></svg>",
            escape(&o.font_family),
            o.font_size,
            if o.bold { "bold" } else { "normal" },
            if o.italic { "italic" } else { "normal" },
            escape(text)
        );
        resvg::usvg::Tree::from_str(&svg, &svg_options())
            .ok()
            .map(|t| t.root().abs_bounding_box().width())
            .unwrap_or(text.chars().count() as f32 * o.font_size * 0.6)
    };
    let reserve = if o.list == ListStyle::None {
        0.
    } else {
        o.font_size * 2.
    };
    text_wrap_ranges(&o.text, (width - reserve).max(o.font_size), |text| {
        measure(text.trim_end())
    })
    .into_iter()
    .map(|range| o.text[range].to_string())
    .collect()
}
fn image_svg(name: &str, r: Rect, assets: &Path) -> Result<String> {
    let data = std::fs::read(asset_path(assets, name)?)?;
    let mime = if name.ends_with(".jpg") || name.ends_with(".jpeg") {
        "image/jpeg"
    } else if name.ends_with(".webp") {
        "image/webp"
    } else {
        "image/png"
    };
    let encoded = base64::engine::general_purpose::STANDARD.encode(data);
    Ok(format!(
        "<image x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" href=\"data:{mime};base64,{encoded}\"/>",
        r.min.x,
        r.min.y,
        r.width(),
        r.height()
    ))
}
pub fn page_svg(page: &Page, assets: &Path, paper: bool) -> Result<String> {
    let p = &page.properties;
    let region = if p.infinite {
        page.objects
            .values()
            .map(|o| o.bounds())
            .reduce(|a, b| a.union(b))
            .unwrap_or(Rect::new(0., 0., p.width, p.height))
            .expand(24.)
    } else {
        Rect::new(0., 0., p.width, p.height)
    };
    let mut body = if paper {
        paper_svg(p, region, assets)?
    } else {
        String::new()
    };
    let hidden = page.hidden_sources();
    for o in page.ordered_objects().filter(|o| !hidden.contains(&o.id())) {
        body.push_str(&object_svg(o, assets)?)
    }
    Ok(wrap_svg(region, region.width(), region.height(), &body))
}
fn wrap_svg(r: Rect, w: f32, h: f32, body: &str) -> String {
    format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{w}\" height=\"{h}\" viewBox=\"{} {} {} {}\">{body}</svg>",
        r.min.x,
        r.min.y,
        r.width(),
        r.height()
    )
}
fn paper_svg(p: &PageProperties, r: Rect, assets: &Path) -> Result<String> {
    let color = if p.pdf.is_some() {
        "#ffffff".into()
    } else {
        p.color.map(|c| c.hex()).unwrap_or_else(|| "#fffefa".into())
    };
    let mut body = format!(
        "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"{color}\"/>",
        r.min.x,
        r.min.y,
        r.width(),
        r.height()
    );
    if let Some(pdf) = &p.pdf {
        if let Some(asset) = &pdf.preview_asset {
            body.push_str(&image_svg(
                asset,
                Rect::new(0., 0., p.width, p.height),
                assets,
            )?)
        }
        return Ok(body);
    }
    let dark = p
        .color
        .is_some_and(|c| 0.2126 * c.r as f32 + 0.7152 * c.g as f32 + 0.0722 * (c.b as f32) < 128.);
    let line = if dark { "#ffffff" } else { "#000000" };
    let pattern = match p.paper {
        Paper::Blank => String::new(),
        Paper::Ruled => format!(
            "<path d='M0 31.5H32' stroke='{line}' stroke-opacity='0.12' stroke-width='0.7'/>"
        ),
        Paper::Grid => format!(
            "<path d='M0 31.5H32 M31.5 0V32' stroke='{line}' stroke-opacity='0.12' stroke-width='0.6'/>"
        ),
        Paper::Dots => format!("<circle cx='16' cy='16' r='1' fill='{line}' fill-opacity='0.22'/>"),
    };
    if !pattern.is_empty() {
        let _ = write!(
            body,
            "<defs><pattern id='paper' width='32' height='32' patternUnits='userSpaceOnUse'>{pattern}</pattern></defs><rect x='{}' y='{}' width='{}' height='{}' fill='url(#paper)'/>",
            r.min.x,
            r.min.y,
            r.width(),
            r.height()
        );
    }
    Ok(body)
}
pub fn raster_object(object: &Object, assets: &Path) -> Result<(Rect, resvg::tiny_skia::Pixmap)> {
    raster_object_limited(object, assets, 32_000_000)
}
pub fn raster_object_limited(
    object: &Object,
    assets: &Path,
    max_pixels: u32,
) -> Result<(Rect, resvg::tiny_skia::Pixmap)> {
    let body = object_svg(object, assets)?;
    let mut bounds = object.bounds().expand(3.);
    if matches!(object, Object::Text(_)) {
        // Wrapped text may extend vertically beyond its frame. Use
        // actual shaped glyph bounds rather than clipping to the nominal frame.
        let measured = wrap_svg(Rect::new(0., 0., 1., 1.), 1., 1., &body);
        let tree = resvg::usvg::Tree::from_str(&measured, &svg_options())?;
        let r = tree.root().abs_bounding_box();
        if r.width() > 0. && r.height() > 0. {
            bounds = bounds.union(Rect::new(r.left(), r.top(), r.width(), r.height()).expand(3.));
        }
    }
    let svg = wrap_svg(bounds, bounds.width(), bounds.height(), &body);
    let scale = 2.0_f32
        .min(((max_pixels.max(1) as f32) / (bounds.width() * bounds.height()).max(1.)).sqrt())
        .min(4096. / bounds.width().max(1.))
        .min(4096. / bounds.height().max(1.));
    Ok((bounds, raster_svg(&svg, scale)?))
}
fn svg_options() -> resvg::usvg::Options<'static> {
    static FONTS: std::sync::OnceLock<std::sync::Arc<resvg::usvg::fontdb::Database>> =
        std::sync::OnceLock::new();
    let fonts = FONTS
        .get_or_init(|| {
            let mut db = resvg::usvg::fontdb::Database::new();
            db.load_system_fonts();
            let names = db
                .faces()
                .flat_map(|f| f.families.iter().map(|f| f.0.clone()))
                .collect::<std::collections::HashSet<_>>();
            let choose = |preferences: &[&str]| {
                preferences
                    .iter()
                    .find(|n| names.contains(**n))
                    .map(|n| n.to_string())
                    .or_else(|| names.iter().next().cloned())
                    .unwrap_or_else(|| "sans-serif".into())
            };
            db.set_sans_serif_family(choose(&[
                "Adwaita Sans",
                "Noto Sans",
                "Liberation Sans",
                "DejaVu Sans",
            ]));
            db.set_serif_family(choose(&["Noto Serif", "Liberation Serif", "DejaVu Serif"]));
            db.set_monospace_family(choose(&[
                "Adwaita Mono",
                "Liberation Mono",
                "DejaVu Sans Mono",
            ]));
            std::sync::Arc::new(db)
        })
        .clone();
    resvg::usvg::Options {
        fontdb: fonts,
        font_family: "sans-serif".into(),
        ..Default::default()
    }
}
/// Bounded rendering shared by export previews and document thumbnails.
pub fn raster_page_limited(
    page: &Page,
    assets: &Path,
    edge: u32,
) -> Result<resvg::tiny_skia::Pixmap> {
    let svg = page_svg(page, assets, true)?;
    let tree = resvg::usvg::Tree::from_str(&svg, &svg_options())?;
    let scale = edge.clamp(1, 2048) as f32 / tree.size().width().max(tree.size().height()).max(1.);
    raster_svg(&svg, scale)
}
pub fn raster_svg(svg: &str, scale: f32) -> Result<resvg::tiny_skia::Pixmap> {
    let tree = resvg::usvg::Tree::from_str(svg, &svg_options())?;
    let w = (tree.size().width() * scale).ceil() as u32;
    let h = (tree.size().height() * scale).ceil() as u32;
    if w == 0 || h == 0 || w > 8192 || h > 8192 || w as u64 * h as u64 > 32_000_000 {
        return Err(Error::Invalid(
            "Export exceeds 32 million pixels; reduce scale or page size".into(),
        ));
    }
    let mut pixmap = resvg::tiny_skia::Pixmap::new(w, h)
        .ok_or_else(|| Error::Invalid("Cannot allocate export image".into()))?;
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    Ok(pixmap)
}
pub fn svg(page: &Page, assets: &Path, path: &Path) -> Result<()> {
    atomic_write(path, page_svg(page, assets, true)?.as_bytes())
}
pub fn png(page: &Page, assets: &Path, path: &Path, scale: f32) -> Result<()> {
    let data = raster_svg(&page_svg(page, assets, true)?, scale)?
        .encode_png()
        .map_err(|e| Error::Invalid(e.to_string()))?;
    atomic_write(path, &data)
}
pub fn text(doc: &Document) -> String {
    doc.pages
        .iter()
        .map(|p| p.text())
        .collect::<Vec<_>>()
        .join("\n\n")
}
pub fn atomic_write(path: &Path, data: &[u8]) -> Result<()> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    std::fs::create_dir_all(parent)?;
    let temporary = parent.join(format!(".folio-export-{}.tmp", Id::new_v4()));
    let result = (|| {
        let mut file = std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temporary)?;
        file.write_all(data)?;
        file.sync_all()?;
        drop(file);
        folio_platform::publish_file(&temporary, path)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result
}

/// Export vector content and preserve original PDF background objects.
pub fn pdf(doc: &Document, assets: &Path, path: &Path) -> Result<()> {
    pdf::export(doc, assets, path)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn custom_paper_color_and_pattern_are_present_in_svg_and_png_exports() {
        let mut page = Page::new();
        page.properties.color = Some(Color::from_rgb(0xfff7e6));
        page.properties.paper = Paper::Dots;
        let svg = page_svg(&page, Path::new("."), true).unwrap();
        assert!(svg.contains("fill=\"#fff7e6\""));
        assert!(svg.contains("<circle"));
        let tree = resvg::usvg::Tree::from_str(&svg, &svg_options()).unwrap();
        let mut pixmap = resvg::tiny_skia::Pixmap::new(794, 1123).unwrap();
        resvg::render(
            &tree,
            resvg::tiny_skia::Transform::identity(),
            &mut pixmap.as_mut(),
        );
        assert_eq!(&pixmap.data()[..4], &[255, 247, 230, 255]);
    }
    #[test]
    fn svg_escapes_text_and_png_has_pixels() {
        let mut p = Page::new();
        let t = Object::Text(TextBlock {
            id: Id::new_v4(),
            text: "<hello> & café".into(),
            rect: Rect::new(40., 40., 300., 100.),
            transform: Transform::default(),
            font_family: "sans-serif".into(),
            font_size: 20.,
            color: Color::INK,
            bold: false,
            italic: false,
            underline: false,
            alignment: Alignment::Left,
            list: ListStyle::None,
        });
        p.order.push(t.id());
        p.objects.insert(t.id(), std::sync::Arc::new(t));
        let svg = page_svg(&p, Path::new("."), true).unwrap();
        assert!(svg.contains("&lt;hello&gt; &amp; café"));
        let pixmap = raster_svg(&svg, 0.5).unwrap();
        assert_eq!(pixmap.width(), 397);
        let object = p.ordered_objects().next().unwrap();
        let (_, text) = raster_object(object, Path::new(".")).unwrap();
        let mut short = object.as_ref().clone();
        if let Object::Text(t) = &mut short {
            t.rect = Rect::new(40., 40., 15., 15.);
        }
        let (bounds, overflow) = raster_object(&short, Path::new(".")).unwrap();
        assert!(
            bounds.height() > 50.,
            "Wrapping must retain all lines beyond a short text frame"
        );
        assert!(overflow.pixels().iter().any(|p| p.alpha() > 0));
        assert!(
            text.pixels().iter().any(|p| p.alpha() > 0),
            "Text must produce visible glyphs"
        );
    }
    #[test]
    fn assets_cannot_escape_root() {
        assert!(asset_path(Path::new("/tmp"), "../secret").is_err());
        assert!(asset_path(Path::new("/tmp"), "/secret").is_err());
    }
}

#[cfg(test)]
mod overlap_tests {
    use super::*;

    #[test]
    fn reversals_crossings_and_pressure_joints_have_no_holes_or_double_opacity() {
        for positions in [
            vec![
                Point::new(30., 110.),
                Point::new(150., 25.),
                Point::new(95., 130.),
            ],
            vec![
                Point::new(20., 75.),
                Point::new(180., 75.),
                Point::new(20., 75.),
            ],
            vec![
                Point::new(30., 30.),
                Point::new(160., 130.),
                Point::new(160., 30.),
                Point::new(30., 130.),
            ],
        ] {
            let raw = positions
                .iter()
                .enumerate()
                .map(|(i, &p)| StrokePoint::new(p, 0.7, i as u64 * 10))
                .collect::<Vec<_>>();
            let mut builder = folio_ink::StrokeBuilder::new(PenStyle::default());
            for &p in &raw {
                builder.push(p);
            }
            let mut stroke = builder.finish().unwrap();
            stroke.path = positions
                .iter()
                .enumerate()
                .map(|(i, &position)| PathPoint {
                    position,
                    radius: 4. + i as f32,
                })
                .collect::<Vec<_>>()
                .into();
            stroke.style.opacity = 0.4;
            let body = object_svg(&Object::Stroke(stroke.clone()), Path::new(".")).unwrap();
            let svg = format!(
                "<svg xmlns='http://www.w3.org/2000/svg' width='220' height='160'>{body}</svg>"
            );
            let pixels = raster_svg(&svg, 1.).unwrap();
            for pair in positions.windows(2) {
                for i in 0..=100 {
                    let p = pair[0].lerp(pair[1], i as f32 / 100.);
                    let alpha = pixels.pixel(p.x as u32, p.y as u32).unwrap().alpha();
                    assert!(
                        (100..=104).contains(&alpha),
                        "Ink hole/doubled opacity at {p:?}: {alpha}"
                    );
                }
            }
            assert_eq!(stroke.raw.as_ref(), &raw);
            assert!(pixels.pixels().iter().all(|p| p.alpha() <= 104));
        }
    }
}
