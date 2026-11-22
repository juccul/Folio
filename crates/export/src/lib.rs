//! Offline exports: SVG/PNG and multi-page PDF with vector pressure outlines.
mod pdf;
use base64::Engine;
use folio_document::*;
use folio_ink::{OutlineScratch, visit_outline};
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
    let mut escaped = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&apos;"),
            _ => escaped.push(character),
        }
    }
    escaped
}

fn write_polygon(s: &mut String, points: &[Point]) {
    for (i, p) in points.iter().enumerate() {
        let _ = write!(s, "{}{} {} ", if i == 0 { "M" } else { "L" }, p.x, p.y);
    }
    s.push('Z');
}
fn write_object_svg(
    out: &mut String,
    object: &Object,
    assets: &Path,
    path_buf: &mut Vec<PathPoint>,
    scratch: &mut OutlineScratch,
) -> Result<()> {
    let t = object.transform();
    let _ = write!(
        out,
        "<g transform=\"matrix({} {} {} {} {} {})\">",
        t.a, t.b, t.c, t.d, t.tx, t.ty
    );
    match object {
        Object::Stroke(s) => {
            out.push_str("<path d=\"");
            visit_outline(s.decode_display_path(path_buf), scratch, |contour| {
                write_polygon(out, contour);
            });
            let _ = write!(
                out,
                "\" fill=\"{}\" fill-rule=\"nonzero\" opacity=\"{}\"/>",
                s.style.color.hex(),
                s.style.opacity
            );
        }
        Object::Shape(s) => {
            out.push_str("<path d=\"");
            for (i, p) in s.vertices.iter().enumerate() {
                let _ = write!(out, "{}{} {} ", if i == 0 { "M" } else { "L" }, p.x, p.y);
            }
            let _ = write!(
                out,
                "\" fill=\"none\" stroke=\"{}\" stroke-width=\"{}\" stroke-linecap=\"round\" stroke-linejoin=\"round\" opacity=\"{}\"/>",
                s.style.color.hex(),
                s.style.width,
                s.style.opacity
            );
        }
        Object::Text(o) => {
            let _ = write!(
                out,
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
                    out,
                    "<tspan x=\"{x}\" y=\"{}\" text-anchor=\"{anchor}\" xml:space=\"preserve\">{}{}</tspan>",
                    o.rect.min.y + o.font_size * (1. + i as f32 * 1.4),
                    escape(&prefix),
                    escape(line)
                );
            }
            out.push_str("</text>");
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
                let _ = write!(
                    out,
                    "<svg x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" viewBox=\"{} {} {} {}\" overflow=\"hidden\">{image}</svg>",
                    r.min.x,
                    r.min.y,
                    r.width(),
                    r.height(),
                    r.min.x,
                    r.min.y,
                    r.width(),
                    r.height()
                );
            } else {
                out.push_str(&image_svg(&o.asset, o.rect, assets)?);
            }
        }
        Object::Equation(e) => {
            if let Some(svg) = &e.rendered_svg {
                let encoded = base64::engine::general_purpose::STANDARD.encode(svg);
                let _ = write!(
                    out,
                    "<image x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" href=\"data:image/svg+xml;base64,{encoded}\"/>",
                    e.rect.min.x,
                    e.rect.min.y,
                    e.rect.width(),
                    e.rect.height()
                );
            } else {
                let _ = write!(
                    out,
                    "<text x=\"{}\" y=\"{}\" font-size=\"24\" fill=\"#2b3934\">{}</text>",
                    e.rect.min.x,
                    e.rect.min.y + 24.,
                    escape(&e.latex)
                );
            }
        }
    }
    out.push_str("</g>");
    Ok(())
}

pub fn object_svg(object: &Object, assets: &Path) -> Result<String> {
    let mut out = String::new();
    let mut path_buf = Vec::new();
    let mut scratch = OutlineScratch::default();
    write_object_svg(&mut out, object, assets, &mut path_buf, &mut scratch)?;
    Ok(out)
}
/// Word wrapping uses the same local font shaping as SVG/PDF export. Explicit
/// newlines remain paragraph boundaries; long individual words are retained.
fn text_lines(o: &TextBlock) -> Vec<String> {
    let width = o.rect.width().max(o.font_size);
    let options = text_block_options(o);
    let measure = |text: &str| {
        let svg = format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"1\" height=\"1\"><text y=\"100\" font-family=\"{}\" font-size=\"{}\" font-weight=\"{}\" font-style=\"{}\">{}</text></svg>",
            escape(&o.font_family),
            o.font_size,
            if o.bold { "bold" } else { "normal" },
            if o.italic { "italic" } else { "normal" },
            escape(text)
        );
        resvg::usvg::Tree::from_str(&svg, &options)
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
/// The area a page exports: the paper, or everything drawn on an infinite page.
fn export_region(page: &Page) -> Rect {
    let p = &page.properties;
    if p.infinite {
        page.objects
            .values()
            .map(|o| o.bounds())
            .reduce(|a, b| a.union(b))
            .unwrap_or(Rect::new(0., 0., p.width, p.height))
            .expand(24.)
    } else {
        Rect::new(0., 0., p.width, p.height)
    }
}
/// Infinite thumbnails show the opening content in a page-shaped window,
/// between half and one paper size, rather than shrinking an unbounded canvas.
fn preview_region(page: &Page) -> Rect {
    let p = &page.properties;
    if !p.infinite {
        return export_region(page);
    }
    let hidden = page.hidden_sources();
    let mut bounds = page
        .ordered_objects()
        .filter(|o| !hidden.contains(&o.id()))
        .map(|o| o.bounds());
    let Some(mut content) = bounds.next() else {
        return Rect::new(0., 0., p.width, p.height);
    };
    let mut first = content;
    for bounds in bounds {
        content = content.union(bounds);
        if bounds
            .min
            .y
            .total_cmp(&first.min.y)
            .then(bounds.min.x.total_cmp(&first.min.x))
            .is_lt()
        {
            first = bounds;
        }
    }
    let content = content.expand(24.);
    let aspect = (p.width / p.height.max(1.)).clamp(0.2, 5.);
    let width = content
        .width()
        .min(content.height() * aspect)
        .clamp(p.width * 0.5, p.width);
    let height = width / aspect;
    // Center compact content and crop larger drawings from the upper left.
    let x = if content.width() <= width {
        content.min.x - (width - content.width()) / 2.
    } else {
        content.min.x
    };
    let y = if content.height() <= height {
        content.min.y - (height - content.height()) / 2.
    } else {
        content.min.y
    };
    let region = Rect::new(x, y, width, height);
    if page
        .ordered_objects()
        .filter(|o| !hidden.contains(&o.id()))
        .any(|o| o.bounds().intersects(region))
    {
        region
    } else {
        // Opposite corners of a sparse canvas can have no content between
        // them. In that case, anchor the crop on the topmost visible object.
        Rect::new(first.min.x - 24., first.min.y - 24., width, height)
    }
}
pub fn page_svg(page: &Page, assets: &Path, paper: bool) -> Result<String> {
    page_svg_in(page, assets, paper, export_region(page), false)
}
fn page_svg_in(
    page: &Page,
    assets: &Path,
    paper: bool,
    region: Rect,
    clip_to_region: bool,
) -> Result<String> {
    let p = &page.properties;
    let mut body = if paper {
        paper_svg(p, region, assets)?
    } else {
        String::new()
    };
    let hidden = page.hidden_sources();
    let mut path_buf = Vec::new();
    let mut scratch = OutlineScratch::default();
    for o in page
        .ordered_objects()
        .filter(|o| !hidden.contains(&o.id()) && (!clip_to_region || o.bounds().intersects(region)))
    {
        write_object_svg(&mut body, o, assets, &mut path_buf, &mut scratch)?;
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
    if let Object::Image(img) = object {
        return raster_image_object(img, assets, max_pixels);
    }
    let body = object_svg(object, assets)?;
    let needs_fonts = match object {
        Object::Stroke(_) | Object::Shape(_) | Object::Image(_) => false,
        Object::Equation(e) => e
            .rendered_svg
            .as_deref()
            .is_none_or(|svg| svg.contains("<text")),
        Object::Text(_) => true,
    };
    let options = if needs_fonts {
        svg_options()
    } else {
        resvg::usvg::Options::default()
    };
    let mut bounds = object.bounds().expand(3.);
    if matches!(object, Object::Text(_)) {
        // Wrapped text may extend vertically beyond its frame. Use
        // actual shaped glyph bounds rather than clipping to the nominal frame.
        let measured = wrap_svg(Rect::new(0., 0., 1., 1.), 1., 1., &body);
        let tree = resvg::usvg::Tree::from_str(&measured, &options)?;
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
    let tree = resvg::usvg::Tree::from_str(&svg, &options)?;
    Ok((bounds, raster_tree(&tree, scale)?))
}

/// Rasterize an `ImageObject` directly without base64-encoding into SVG or loading system fonts.
fn raster_image_object(
    img: &ImageObject,
    assets: &Path,
    max_pixels: u32,
) -> Result<(Rect, resvg::tiny_skia::Pixmap)> {
    let path = asset_path(assets, &img.asset)?;
    let decoded = image::ImageReader::open(&path)?
        .with_guessed_format()?
        .decode()?;
    let (src_w, src_h) = (decoded.width(), decoded.height());
    if src_w == 0 || src_h == 0 {
        return Err(Error::Invalid("Empty image asset".into()));
    }
    let rgba = if let Some(crop) = img.crop {
        let cx = ((crop.min.x * src_w as f32).floor().max(0.) as u32).min(src_w - 1);
        let cy = ((crop.min.y * src_h as f32).floor().max(0.) as u32).min(src_h - 1);
        let cw = ((crop.width() * src_w as f32).ceil().max(1.) as u32).min(src_w - cx);
        let ch = ((crop.height() * src_h as f32).ceil().max(1.) as u32).min(src_h - cy);
        decoded.crop_imm(cx, cy, cw, ch).into_rgba8()
    } else {
        decoded.into_rgba8()
    };
    let bounds = Object::Image(img.clone()).bounds().expand(3.);
    let scale = 2.0_f32
        .min(((max_pixels.max(1) as f32) / (bounds.width() * bounds.height()).max(1.)).sqrt())
        .min(4096. / bounds.width().max(1.))
        .min(4096. / bounds.height().max(1.));
    if !scale.is_finite() || scale <= 0. {
        return Err(Error::Invalid("Export scale must be finite and positive".into()));
    }
    let out_w = (bounds.width() * scale).ceil() as u32;
    let out_h = (bounds.height() * scale).ceil() as u32;
    if out_w == 0 || out_h == 0 || out_w > 8192 || out_h > 8192 || out_w as u64 * out_h as u64 > 32_000_000 {
        return Err(Error::Invalid("Export exceeds 32 million pixels; reduce scale or page size".into()));
    }
    let (rw, rh) = rgba.dimensions();
    let mut raw = rgba.into_raw();
    // Convert straight RGBA to tiny_skia premultiplied RGBA.
    for p in raw.chunks_exact_mut(4) {
        let a = p[3] as u32;
        if a < 255 {
            p[0] = ((p[0] as u32 * a + 127) / 255) as u8;
            p[1] = ((p[1] as u32 * a + 127) / 255) as u8;
            p[2] = ((p[2] as u32 * a + 127) / 255) as u8;
        }
    }
    let src_pixmap = resvg::tiny_skia::Pixmap::from_vec(
        raw,
        resvg::tiny_skia::IntSize::from_wh(rw, rh)
            .ok_or_else(|| Error::Invalid("Invalid image dimensions".into()))?,
    )
    .ok_or_else(|| Error::Invalid("Cannot build source pixmap".into()))?;
    let mut pixmap = resvg::tiny_skia::Pixmap::new(out_w, out_h)
        .ok_or_else(|| Error::Invalid("Cannot allocate export image".into()))?;
    let r = img.rect;
    let sx = r.width() / rw as f32;
    let sy = r.height() / rh as f32;
    let local = resvg::tiny_skia::Transform::from_row(sx, 0., 0., sy, r.min.x, r.min.y);
    let t = img.transform;
    let obj_tx = resvg::tiny_skia::Transform::from_row(t.a, t.b, t.c, t.d, t.tx, t.ty);
    let view_tx = resvg::tiny_skia::Transform::from_scale(scale, scale)
        .pre_translate(-bounds.min.x, -bounds.min.y);
    let full_tx = view_tx.pre_concat(obj_tx).pre_concat(local);
    let paint = resvg::tiny_skia::PixmapPaint {
        opacity: 1.,
        blend_mode: resvg::tiny_skia::BlendMode::SourceOver,
        quality: resvg::tiny_skia::FilterQuality::Bilinear,
    };
    pixmap.draw_pixmap(0, 0, src_pixmap.as_ref(), &paint, full_tx, None);
    Ok((bounds, pixmap))
}

fn fill_ink_stroke(
    pixmap: &mut resvg::tiny_skia::Pixmap,
    paint: &mut resvg::tiny_skia::Paint,
    view_tx: resvg::tiny_skia::Transform,
    stroke: &InkStroke,
    color: Color,
    path_buf: &mut Vec<PathPoint>,
    scratch: &mut OutlineScratch,
) {
    let mut pb = resvg::tiny_skia::PathBuilder::new();
    visit_outline(
        stroke.decode_display_path(path_buf),
        scratch,
        |contour| {
            if let Some(first) = contour.first() {
                pb.move_to(first.x, first.y);
                for p in contour.iter().skip(1) {
                    pb.line_to(p.x, p.y);
                }
                pb.close();
            }
        },
    );
    let Some(path) = pb.finish() else {
        return;
    };
    let alpha = (stroke.style.opacity.clamp(0., 1.) * 255.).round() as u8;
    paint.set_color_rgba8(color.r, color.g, color.b, alpha);
    let t = stroke.transform;
    let obj_tx = resvg::tiny_skia::Transform::from_row(t.a, t.b, t.c, t.d, t.tx, t.ty);
    pixmap.fill_path(
        &path,
        paint,
        resvg::tiny_skia::FillRule::Winding,
        view_tx.pre_concat(obj_tx),
        None,
    );
}

/// Rasterize ink strokes directly into a premultiplied RGBA pixmap without SVG
/// string formatting or XML/usvg parsing.
pub fn raster_ink_strokes<'a>(
    strokes: impl IntoIterator<Item = (&'a InkStroke, Color)>,
    bounds: Rect,
    scale: f32,
) -> Result<resvg::tiny_skia::Pixmap> {
    if !scale.is_finite() || scale <= 0. {
        return Err(Error::Invalid("Export scale must be finite and positive".into()));
    }
    let w = (bounds.width() * scale - 1e-3).ceil().max(1.) as u32;
    let h = (bounds.height() * scale - 1e-3).ceil().max(1.) as u32;
    if w == 0 || h == 0 || w > 8192 || h > 8192 || w as u64 * h as u64 > 32_000_000 {
        return Err(Error::Invalid("Export exceeds 32 million pixels".into()));
    }
    let mut pixmap = resvg::tiny_skia::Pixmap::new(w, h)
        .ok_or_else(|| Error::Invalid("Cannot allocate ink layer image".into()))?;
    let view_tx = resvg::tiny_skia::Transform::from_scale(scale, scale)
        .pre_translate(-bounds.min.x, -bounds.min.y);
    let mut paint = resvg::tiny_skia::Paint {
        anti_alias: true,
        ..Default::default()
    };
    let mut path_buf = Vec::new();
    let mut scratch = OutlineScratch::default();
    for (stroke, color) in strokes {
        fill_ink_stroke(
            &mut pixmap,
            &mut paint,
            view_tx,
            stroke,
            color,
            &mut path_buf,
            &mut scratch,
        );
    }
    Ok(pixmap)
}

fn is_basic_latin_text(text: &str) -> bool {
    text.chars().all(|c| {
        c <= '\u{024F}'
            || matches!(
                c,
                '•' | '–' | '—' | '…' | '‘' | '’' | '“' | '”' | '×' | '÷' | '−' | '±'
            )
    })
}

fn is_generic_family(family: &str) -> bool {
    let trimmed = family.trim();
    trimmed.is_empty()
        || trimmed.eq_ignore_ascii_case("sans-serif")
        || trimmed.eq_ignore_ascii_case("serif")
        || trimmed.eq_ignore_ascii_case("monospace")
        || trimmed.eq_ignore_ascii_case("sans")
}

struct MinimalFonts {
    db: std::sync::Arc<resvg::usvg::fontdb::Database>,
    families: std::collections::HashSet<String>,
    has_bold: bool,
    has_italic: bool,
}

fn load_family_variants(db: &mut resvg::usvg::fontdb::Database, path: &Path) -> Option<String> {
    let before = db.len();
    let _ = db.load_font_file(path);
    if db.len() == before {
        return None;
    }
    let family = db
        .faces()
        .skip(before)
        .find(|face| face.weight == resvg::usvg::fontdb::Weight::NORMAL)
        .or_else(|| db.faces().nth(before))
        .and_then(|face| face.families.first())
        .map(|(name, _)| name.clone());
    if let (Some(parent), Some(stem), Some(ext)) = (
        path.parent(),
        path.file_stem().and_then(|s| s.to_str()),
        path.extension().and_then(|e| e.to_str()),
    ) {
        let base = stem.split('-').next().unwrap_or(stem);
        let mut candidates = Vec::with_capacity(10);
        for suffix in ["Bold", "Italic", "BoldItalic", "Oblique", "BoldOblique"] {
            candidates.push(parent.join(format!("{base}-{suffix}.{ext}")));
        }
        // Windows standard font file naming (segoeui{b,i,z}, arial{bd,i,bi}, times{bd,i,bi}, consola{b,i,z}).
        for suffix in ["b", "i", "z", "bd", "bi"] {
            candidates.push(parent.join(format!("{stem}{suffix}.{ext}")));
        }
        for candidate in candidates {
            if candidate != path && candidate.is_file() {
                let _ = db.load_font_file(&candidate);
            }
        }
    }
    family
}

fn find_default_font(candidates: &[&str], fc_generic: &str) -> Option<PathBuf> {
    for candidate in candidates {
        let path = Path::new(candidate);
        if path.is_file() {
            return Some(path.to_path_buf());
        }
    }
    #[cfg(target_os = "linux")]
    {
        let output = std::process::Command::new("fc-match")
            .args(["-f", "%{file}", fc_generic])
            .output()
            .ok()?;
        if output.status.success() {
            let path = PathBuf::from(String::from_utf8_lossy(&output.stdout).trim());
            if path.is_file() {
                return Some(path);
            }
        }
    }
    let _ = fc_generic;
    None
}

fn minimal_fonts() -> Option<&'static MinimalFonts> {
    static MINIMAL: std::sync::OnceLock<Option<MinimalFonts>> = std::sync::OnceLock::new();
    MINIMAL
        .get_or_init(|| {
            let mut db = resvg::usvg::fontdb::Database::new();
            // Match `svg_options()` family priority so wrapping and rendering agree.
            let sans = find_default_font(
                &[
                    "/usr/share/fonts/adwaita-sans-fonts/AdwaitaSans-Regular.ttf",
                    "/usr/share/fonts/Adwaita/AdwaitaSans-Regular.ttf",
                    "/usr/share/fonts/google-noto/NotoSans-Regular.ttf",
                    "/usr/share/fonts/noto/NotoSans-Regular.ttf",
                    "/usr/share/fonts/liberation-sans-fonts/LiberationSans-Regular.ttf",
                    "/usr/share/fonts/liberation/LiberationSans-Regular.ttf",
                    "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
                    "/usr/share/fonts/TTF/DejaVuSans.ttf",
                    "/System/Library/Fonts/Helvetica.ttc",
                    "C:\\Windows\\Fonts\\segoeui.ttf",
                    "C:\\Windows\\Fonts\\arial.ttf",
                ],
                "sans-serif",
            )
            .and_then(|p| load_family_variants(&mut db, &p));
            let serif = find_default_font(
                &[
                    "/usr/share/fonts/google-noto/NotoSerif-Regular.ttf",
                    "/usr/share/fonts/noto/NotoSerif-Regular.ttf",
                    "/usr/share/fonts/liberation-serif-fonts/LiberationSerif-Regular.ttf",
                    "/usr/share/fonts/liberation/LiberationSerif-Regular.ttf",
                    "/usr/share/fonts/truetype/dejavu/DejaVuSerif.ttf",
                    "/usr/share/fonts/TTF/DejaVuSerif.ttf",
                    "/System/Library/Fonts/Times.ttc",
                    "C:\\Windows\\Fonts\\times.ttf",
                ],
                "serif",
            )
            .and_then(|p| load_family_variants(&mut db, &p));
            let mono = find_default_font(
                &[
                    "/usr/share/fonts/adwaita-mono-fonts/AdwaitaMono-Regular.ttf",
                    "/usr/share/fonts/Adwaita/AdwaitaMono-Regular.ttf",
                    "/usr/share/fonts/liberation-mono-fonts/LiberationMono-Regular.ttf",
                    "/usr/share/fonts/liberation/LiberationMono-Regular.ttf",
                    "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf",
                    "/usr/share/fonts/TTF/DejaVuSansMono.ttf",
                    "/usr/share/fonts/google-noto/NotoSansMono-Regular.ttf",
                    "/System/Library/Fonts/Menlo.ttc",
                    "C:\\Windows\\Fonts\\consola.ttf",
                ],
                "monospace",
            )
            .and_then(|p| load_family_variants(&mut db, &p));
            let fallback = sans.clone().or_else(|| serif.clone()).or_else(|| mono.clone())?;
            db.set_sans_serif_family(sans.unwrap_or_else(|| fallback.clone()));
            db.set_serif_family(serif.unwrap_or_else(|| fallback.clone()));
            db.set_monospace_family(mono.unwrap_or(fallback));
            let has_bold = db
                .faces()
                .any(|f| f.weight.0 >= resvg::usvg::fontdb::Weight::BOLD.0);
            let has_italic = db.faces().any(|f| {
                matches!(
                    f.style,
                    resvg::usvg::fontdb::Style::Italic | resvg::usvg::fontdb::Style::Oblique
                )
            });
            let families = db
                .faces()
                .flat_map(|f| f.families.iter().map(|(n, _)| n.to_ascii_lowercase()))
                .collect();
            Some(MinimalFonts {
                db: std::sync::Arc::new(db),
                families,
                has_bold,
                has_italic,
            })
        })
        .as_ref()
}

fn minimal_covers_text(minimal: &MinimalFonts, text: &TextBlock) -> bool {
    is_basic_latin_text(&text.text)
        && (is_generic_family(&text.font_family)
            || minimal
                .families
                .contains(&text.font_family.trim().to_ascii_lowercase()))
        && (!text.bold || minimal.has_bold)
        && (!text.italic || minimal.has_italic)
}

fn minimal_svg_options(minimal: &MinimalFonts) -> resvg::usvg::Options<'static> {
    resvg::usvg::Options {
        fontdb: minimal.db.clone(),
        font_family: "sans-serif".into(),
        ..Default::default()
    }
}

fn text_block_options(o: &TextBlock) -> resvg::usvg::Options<'static> {
    if let Some(minimal) = minimal_fonts()
        && minimal_covers_text(minimal, o)
    {
        return minimal_svg_options(minimal);
    }
    svg_options()
}

fn page_options(page: &Page) -> resvg::usvg::Options<'static> {
    let mut needs_fonts = false;
    let mut needs_full = false;
    let minimal = minimal_fonts();
    for object in page.objects.values() {
        match object.as_ref() {
            Object::Text(text) => {
                needs_fonts = true;
                if minimal.is_none_or(|m| !minimal_covers_text(m, text)) {
                    needs_full = true;
                    break;
                }
            }
            Object::Equation(e) => {
                if let Some(svg) = e.rendered_svg.as_deref() {
                    if svg.contains("<text") || svg.contains(":text") {
                        needs_fonts = true;
                        needs_full = true;
                        break;
                    }
                } else {
                    needs_fonts = true;
                    if !is_basic_latin_text(&e.latex) || minimal.is_none() {
                        needs_full = true;
                        break;
                    }
                }
            }
            Object::Stroke(_) | Object::Shape(_) | Object::Image(_) => {}
        }
    }
    if !needs_fonts {
        resvg::usvg::Options::default()
    } else if !needs_full
        && let Some(minimal) = minimal
    {
        minimal_svg_options(minimal)
    } else {
        svg_options()
    }
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
/// Render the full exported area with a bounded output resolution.
pub fn raster_page_limited(
    page: &Page,
    assets: &Path,
    edge: u32,
) -> Result<resvg::tiny_skia::Pixmap> {
    raster_page_in(page, assets, edge, export_region(page), false)
}
/// A thumbnail of `preview_region`, with its longest edge at most `edge` pixels.
pub fn raster_page_thumbnail(
    page: &Page,
    assets: &Path,
    edge: u32,
) -> Result<resvg::tiny_skia::Pixmap> {
    raster_page_in(page, assets, edge, preview_region(page), true)
}
fn raster_page_in(
    page: &Page,
    assets: &Path,
    edge: u32,
    region: Rect,
    clip_to_region: bool,
) -> Result<resvg::tiny_skia::Pixmap> {
    let hidden = page.hidden_sources();
    let has_complex = page.ordered_objects().any(|o| {
        !hidden.contains(&o.id())
            && (!clip_to_region || o.bounds().intersects(region))
            && !matches!(o.as_ref(), Object::Stroke(_) | Object::Shape(_))
    });
    if has_complex {
        let svg = page_svg_in(page, assets, true, region, clip_to_region)?;
        return raster_svg_limited(page, &svg, edge);
    }
    // Fast path for ink/shape/blank pages: render only the compact paper SVG
    // (preserving exact pattern parity) and rasterize visible strokes/shapes
    // directly with tiny-skia, avoiding multi-megabyte SVG XML allocation.
    let paper = wrap_svg(
        region,
        region.width(),
        region.height(),
        &paper_svg(&page.properties, region, assets)?,
    );
    let tree = resvg::usvg::Tree::from_str(&paper, &resvg::usvg::Options::default())?;
    let scale = edge.clamp(1, 2048) as f32 / tree.size().width().max(tree.size().height()).max(1.);
    let mut pixmap = raster_tree(&tree, scale)?;
    let view_tx = resvg::tiny_skia::Transform::from_scale(scale, scale)
        .pre_translate(-region.min.x, -region.min.y);
    let mut paint = resvg::tiny_skia::Paint {
        anti_alias: true,
        ..Default::default()
    };
    let mut path_buf = Vec::new();
    let mut scratch = OutlineScratch::default();
    for object in page.ordered_objects().filter(|o| {
        !hidden.contains(&o.id()) && (!clip_to_region || o.bounds().intersects(region))
    }) {
        match object.as_ref() {
            Object::Stroke(stroke) => {
                fill_ink_stroke(
                    &mut pixmap,
                    &mut paint,
                    view_tx,
                    stroke,
                    stroke.style.color,
                    &mut path_buf,
                    &mut scratch,
                );
            }
            Object::Shape(shape) => {
                let mut pb = resvg::tiny_skia::PathBuilder::new();
                if let Some(first) = shape.vertices.first() {
                    pb.move_to(first.x, first.y);
                    for p in shape.vertices.iter().skip(1) {
                        pb.line_to(p.x, p.y);
                    }
                }
                let Some(path) = pb.finish() else {
                    continue;
                };
                let c = shape.style.color;
                let alpha = (shape.style.opacity.clamp(0., 1.) * 255.).round() as u8;
                paint.set_color_rgba8(c.r, c.g, c.b, alpha);
                let stroke = resvg::tiny_skia::Stroke {
                    width: shape.style.width,
                    line_cap: resvg::tiny_skia::LineCap::Round,
                    line_join: resvg::tiny_skia::LineJoin::Round,
                    ..Default::default()
                };
                let t = shape.transform;
                let obj_tx =
                    resvg::tiny_skia::Transform::from_row(t.a, t.b, t.c, t.d, t.tx, t.ty);
                pixmap.stroke_path(
                    &path,
                    &paint,
                    &stroke,
                    view_tx.pre_concat(obj_tx),
                    None,
                );
            }
            _ => {}
        }
    }
    Ok(pixmap)
}
fn raster_svg_limited(page: &Page, svg: &str, edge: u32) -> Result<resvg::tiny_skia::Pixmap> {
    let tree = resvg::usvg::Tree::from_str(svg, &page_options(page))?;
    let scale = edge.clamp(1, 2048) as f32 / tree.size().width().max(tree.size().height()).max(1.);
    raster_tree(&tree, scale)
}
pub fn raster_svg(svg: &str, scale: f32) -> Result<resvg::tiny_skia::Pixmap> {
    let has_text = svg.contains("<text")
        || svg.contains(":text")
        || svg.contains("data:image/svg+xml");
    let options = if !has_text {
        resvg::usvg::Options::default()
    } else if is_basic_latin_text(svg)
        && !svg.contains("data:image/svg+xml")
        && !svg.contains("font-family")
        && !svg.contains("font-weight")
        && !svg.contains("font-style")
        && !svg.contains("&#")
        && let Some(minimal) = minimal_fonts()
    {
        minimal_svg_options(minimal)
    } else {
        svg_options()
    };
    let tree = resvg::usvg::Tree::from_str(svg, &options)?;
    raster_tree(&tree, scale)
}
/// Rasterize generated paths/shapes without loading a system font database.
/// Text and embedded SVG images must use `raster_svg` instead.
pub fn raster_geometry_svg(svg: &str, scale: f32) -> Result<resvg::tiny_skia::Pixmap> {
    let tree = resvg::usvg::Tree::from_str(svg, &resvg::usvg::Options::default())?;
    raster_tree(&tree, scale)
}
fn raster_tree(tree: &resvg::usvg::Tree, scale: f32) -> Result<resvg::tiny_skia::Pixmap> {
    if !scale.is_finite() || scale <= 0. {
        return Err(Error::Invalid(
            "Export scale must be finite and positive".into(),
        ));
    }
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
        tree,
        resvg::tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    Ok(pixmap)
}
pub fn svg(page: &Page, assets: &Path, path: &Path) -> Result<()> {
    atomic_write(path, page_svg(page, assets, true)?.as_bytes())
}
pub fn png(page: &Page, assets: &Path, path: &Path, scale: f32) -> Result<()> {
    let tree = resvg::usvg::Tree::from_str(&page_svg(page, assets, true)?, &page_options(page))?;
    let data = raster_tree(&tree, scale)?
        .encode_png()
        .map_err(|e| Error::Invalid(e.to_string()))?;
    atomic_write(path, &data)
}
pub fn text(doc: &Document) -> String {
    let mut text = String::new();
    for (page_index, page) in doc.pages.iter().enumerate() {
        if page_index > 0 {
            text.push_str("\n\n");
        }
        for (fragment_index, fragment) in page.text_fragments().enumerate() {
            if fragment_index > 0 {
                text.push('\n');
            }
            text.push_str(fragment);
        }
    }
    text
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
    fn text_export_preserves_empty_pages_and_xml_escaping_preserves_unicode() {
        let mut document = Document::new("Text");
        document.pages.push(Page::new());
        document.pages.push(Page::new());
        assert_eq!(text(&document), "\n\n\n\n");
        assert_eq!(
            escape("Café & <x> \"quoted\" 'apostrophe'"),
            "Café &amp; &lt;x&gt; &quot;quoted&quot; &apos;apostrophe&apos;"
        );
    }

    #[test]
    fn bounded_raster_matches_single_parse_render_and_rejects_invalid_scale() {
        let page = Page::new();
        let svg = page_svg(&page, Path::new("."), true).unwrap();
        let preview = raster_page_limited(&page, Path::new("."), 128).unwrap();
        let reference = raster_svg(
            &svg,
            128. / page.properties.width.max(page.properties.height),
        )
        .unwrap();
        assert_eq!(preview.data(), reference.data());
        assert_eq!(
            raster_geometry_svg(
                &svg,
                128. / page.properties.width.max(page.properties.height)
            )
            .unwrap()
            .data(),
            reference.data()
        );
        assert!(preview.width().max(preview.height()) <= 128);
        // Paper pages preview exactly what they export.
        let thumbnail = raster_page_thumbnail(&page, Path::new("."), 128).unwrap();
        assert_eq!(thumbnail.data(), preview.data());
        for scale in [0., -1., f32::NAN, f32::INFINITY] {
            assert!(raster_svg(&svg, scale).is_err());
        }
    }

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
    fn bounded_page_preview_renders_equations_crops_and_transforms() {
        let root = std::env::temp_dir().join(format!("folio-thumbnail-{}", Id::new_v4()));
        std::fs::create_dir(&root).unwrap();
        let image = image::RgbaImage::from_fn(20, 10, |x, _| {
            if x < 10 {
                image::Rgba([255, 0, 0, 255])
            } else {
                image::Rgba([0, 0, 255, 255])
            }
        });
        image.save(root.join("source.png")).unwrap();
        let mut page = Page::new();
        page.properties.width = 100.;
        page.properties.height = 100.;
        page.properties.paper = Paper::Blank;
        page.properties.color = Some(Color::from_rgb(0xffffff));
        let object = Object::Image(ImageObject {
            id: Id::new_v4(),
            asset: "source.png".into(),
            rect: Rect::new(10., 10., 40., 40.),
            transform: Transform::translate(10., 0.),
            crop: Some(Rect::new(0.5, 0., 0.5, 1.)),
        });
        page.order.push(object.id());
        page.objects
            .insert(object.id(), std::sync::Arc::new(object));
        let equation = Object::Equation(Equation {id:Id::new_v4(),latex:"should not appear".into(),rendered_svg:Some(r##"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"><rect width="10" height="10" fill="#00ff00"/></svg>"##.into()),rect:Rect::new(60.,60.,20.,20.),source_strokes:vec![],transform:Transform::default(),math_link:None});
        page.order.push(equation.id());
        page.objects
            .insert(equation.id(), std::sync::Arc::new(equation));
        let preview = raster_page_limited(&page, &root, 100).unwrap();
        assert_eq!((preview.width(), preview.height()), (100, 100));
        let blue = preview.pixel(30, 30).unwrap();
        assert_eq!((blue.red(), blue.green(), blue.blue()), (0, 0, 255));
        let green = preview.pixel(70, 70).unwrap();
        assert_eq!((green.red(), green.green(), green.blue()), (0, 255, 0));
        let smaller = raster_page_limited(&page, &root, 24).unwrap();
        assert!(smaller.width() <= 24 && smaller.height() <= 24);
        let thumbnail = raster_page_thumbnail(&page, &root, 100).unwrap();
        assert_eq!(thumbnail.data(), preview.data());
        // Tall, wide and diagonally scattered infinite content keeps a readable
        // opening thumbnail, including when the topmost content is far right
        // or the whole drawing lives at negative coordinates.
        for (x, y) in [(0., 3000.), (3000., 0.), (3000., 3000.), (-3000., 3000.)] {
            for origin in [(0., 0.), (-4000., -5000.)] {
                let mut infinite = page.clone();
                infinite.properties.infinite = true;
                infinite.properties.height = 140.;
                let mut distant = infinite.objects[&infinite.order[1]].as_ref().clone();
                distant.set_id(Id::new_v4());
                if let Object::Equation(equation) = &mut distant {
                    equation.transform = Transform::translate(x, y);
                }
                infinite.order.push(distant.id());
                infinite
                    .objects
                    .insert(distant.id(), std::sync::Arc::new(distant));
                for object in infinite.objects.values_mut() {
                    let object = std::sync::Arc::make_mut(object);
                    let transform = match object {
                        Object::Image(image) => &mut image.transform,
                        Object::Equation(equation) => &mut equation.transform,
                        _ => unreachable!(),
                    };
                    transform.tx += origin.0;
                    transform.ty += origin.1;
                }
                let region = preview_region(&infinite);
                assert!(region.width() <= infinite.properties.width);
                assert!(region.height() <= infinite.properties.height);
                let thumbnail = raster_page_thumbnail(&infinite, &root, 128).unwrap();
                let aspect = infinite.properties.width / infinite.properties.height;
                assert!(
                    (thumbnail.width() as f32 / thumbnail.height() as f32 - aspect).abs() < 0.01
                );
                let blue_pixels = |raster: &resvg::tiny_skia::Pixmap| {
                    raster
                        .pixels()
                        .iter()
                        .filter(|p| p.blue() == 255 && p.red() == 0)
                        .count()
                };
                assert!(
                    blue_pixels(&thumbnail) > 100,
                    "Opening content must remain visible"
                );
                let exported = raster_page_limited(&infinite, &root, 128).unwrap();
                assert!(blue_pixels(&exported) < blue_pixels(&thumbnail));
                let exported_region = export_region(&infinite);
                assert!(exported_region.width() > 3000. || exported_region.height() > 3000.);
            }
        }
        // An equation can contain SVG text inside its encoded image, so page
        // classification must preserve fonts even without outer text objects.
        page.objects.clear();
        page.order.clear();
        let equation = Object::Equation(Equation { id: Id::new_v4(), latex: "8".into(), rendered_svg: Some(r##"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="100"><text x="10" y="75" font-family="sans-serif" font-size="70" fill="black">8</text></svg>"##.into()), rect: Rect::new(5.,5.,90.,90.), source_strokes: vec![], transform: Transform::default(), math_link: None });
        page.order.push(equation.id());
        page.objects
            .insert(equation.id(), std::sync::Arc::new(equation.clone()));
        let preview = raster_page_limited(&page, &root, 100).unwrap();
        assert!(
            preview
                .pixels()
                .iter()
                .any(|pixel| pixel.red() < 128 && pixel.green() < 128 && pixel.blue() < 128)
        );
        let (_, object) = raster_object_limited(&equation, &root, 4096).unwrap();
        assert!(object.pixels().iter().any(|pixel| pixel.alpha() > 0));
        std::fs::remove_dir_all(root).unwrap();
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
            stroke.path = std::sync::Arc::new(
                positions
                    .iter()
                    .enumerate()
                    .map(|(i, &position)| PathPoint {
                        position,
                        radius: 4. + i as f32,
                    })
                    .collect::<Vec<_>>()
                    .into(),
            );
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
