//! Structural validation of documents read from storage or the clipboard.
use super::*;

impl Document {
    /// Reject malformed geometry, styles and references before they reach
    /// rendering or editing.
    pub fn validate(&self) -> Result<(), String> {
        if self.version == 0 || self.version > FORMAT_VERSION {
            return Err(format!("Unsupported document version {}", self.version));
        }
        if self.pages.is_empty() {
            return Err("A document needs at least one page".into());
        }
        let mut ids = std::collections::HashSet::new();
        for page in &self.pages {
            if !ids.insert(page.id) {
                return Err("Duplicate page ID".into());
            }
            for entry in &page.ink_text {
                if entry.text.trim().is_empty()
                    || entry.text.len() > 65536
                    || (!entry.stale && entry.sources.is_empty())
                    || entry.sources.len() > 4096
                    || entry
                        .sources
                        .iter()
                        .collect::<std::collections::HashSet<_>>()
                        .len()
                        != entry.sources.len()
                    || !entry.sources.iter().all(|id| {
                        page.objects
                            .get(id)
                            .is_some_and(|o| matches!(o.as_ref(), Object::Stroke(_)))
                    })
                    || ![
                        entry.bounds.min.x,
                        entry.bounds.min.y,
                        entry.bounds.max.x,
                        entry.bounds.max.y,
                    ]
                    .iter()
                    .all(|v| v.is_finite())
                    || entry.bounds.width() <= 0.
                    || entry.bounds.height() <= 0.
                {
                    return Err("Invalid handwriting search annotation".into());
                }
            }
            let p = &page.properties;
            if !p.width.is_finite() || !p.height.is_finite() || p.width <= 0. || p.height <= 0. {
                return Err("Invalid page size".into());
            }
            if page.order.len() != page.objects.len()
                || page
                    .order
                    .iter()
                    .collect::<std::collections::HashSet<_>>()
                    .len()
                    != page.order.len()
            {
                return Err("Invalid object order".into());
            }
            for id in &page.order {
                let Some(o) = page.objects.get(id) else {
                    return Err("Missing object".into());
                };
                if !ids.insert(*id) {
                    return Err("Duplicate object ID across pages".into());
                }
                if *id != o.id() {
                    return Err("Mismatched object ID".into());
                }
                let rect = match o.as_ref() {
                    Object::Text(text) => {
                        if !text.font_size.is_finite() || text.font_size <= 0. {
                            return Err("Invalid text font size".into());
                        }
                        Some(text.rect)
                    }
                    Object::Image(image) => Some(image.rect),
                    Object::Equation(equation) => Some(equation.rect),
                    Object::Stroke(stroke) => {
                        if !stroke.style.valid() {
                            return Err("Invalid stroke style".into());
                        }
                        None
                    }
                    Object::Shape(shape) => {
                        if !shape.style.valid()
                            || shape.vertices.len() < 2
                            || shape
                                .vertices
                                .iter()
                                .any(|point| !point.x.is_finite() || !point.y.is_finite())
                        {
                            return Err("Invalid shape geometry or style".into());
                        }
                        None
                    }
                };
                if rect.is_some_and(|rect| {
                    ![
                        rect.min.x,
                        rect.min.y,
                        rect.max.x,
                        rect.max.y,
                        rect.width(),
                        rect.height(),
                    ]
                    .iter()
                    .all(|value| value.is_finite())
                        || rect.width() <= 0.
                        || rect.height() <= 0.
                }) {
                    return Err("Invalid object rectangle".into());
                }
                let bounds = o.bounds();
                if ![
                    bounds.min.x,
                    bounds.min.y,
                    bounds.max.x,
                    bounds.max.y,
                    bounds.width(),
                    bounds.height(),
                ]
                .iter()
                .all(|value| value.is_finite())
                {
                    return Err("Object geometry overflows its transform".into());
                }
                if let Object::Equation(e) = o.as_ref()
                    && let Some(link) = &e.math_link
                {
                    if link.expression.len() > 8192
                        || link.variable.len() > 32
                        || link.sources.len() > 4096
                        || !link.x_min.is_finite()
                        || !link.x_max.is_finite()
                        || link.x_min >= link.x_max
                        || !matches!(
                            link.operation.as_str(),
                            "auto"
                                | "evaluate"
                                | "simplify"
                                | "factor"
                                | "solve"
                                | "differentiate"
                                | "integrate"
                                | "graph"
                                | "assign"
                        )
                        || !matches!(link.domain.as_str(), "real" | "complex")
                        || !matches!(link.angle.as_str(), "radians" | "degrees")
                    {
                        return Err("Invalid linked mathematical calculation".into());
                    }
                    if let Some(region) = link.ink_region
                        && (![region.min.x, region.min.y, region.max.x, region.max.y]
                            .iter()
                            .all(|v| v.is_finite())
                            || region.width() <= 0.
                            || region.height() <= 0.)
                    {
                        return Err("Invalid live mathematics region".into());
                    }
                }
                if let Object::Image(image) = o.as_ref()
                    && let Some(crop) = image.crop
                    && (!crop.min.x.is_finite()
                        || !crop.min.y.is_finite()
                        || !crop.max.x.is_finite()
                        || !crop.max.y.is_finite()
                        || crop.min.x < 0.
                        || crop.min.y < 0.
                        || crop.max.x > 1.
                        || crop.max.y > 1.
                        || crop.width() <= 0.
                        || crop.height() <= 0.)
                {
                    return Err("Invalid image crop".into());
                }
                let t = o.transform();
                if ![t.a, t.b, t.c, t.d, t.tx, t.ty]
                    .iter()
                    .all(|v| v.is_finite())
                    || t.inverse().is_none()
                {
                    return Err("Invalid transform".into());
                }
                if let Object::Stroke(s) = o.as_ref()
                    && (s.raw.is_empty()
                        || s.raw.iter().any(|p| !p.valid())
                        || s.path.is_empty()
                        || s.path
                            .iter()
                            .chain(s.fragment_path.iter().flatten())
                            .chain(s.refined_path.iter().flatten())
                            .any(|p| {
                                !p.position.x.is_finite()
                                    || !p.position.y.is_finite()
                                    || !p.radius.is_finite()
                                    || p.radius <= 0.
                            }))
                {
                    return Err("Invalid stroke".into());
                }
            }
        }
        Ok(())
    }
}
