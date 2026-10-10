use super::*;
use folio_document::{Color, Object, Page};
use std::sync::Arc;
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub(super) enum Appearance {
    #[default]
    Original,
    Visible,
    Print,
}
#[derive(Default)]
pub(super) struct Options {
    pub appearance: Appearance,
    key: Option<(
        Id,
        u64,
        folio_document::PageProperties,
        theme::CanvasTheme,
        Appearance,
    )>,
    generation: u64,
    image: Option<Arc<RenderImage>>,
    error: Option<String>,
}
pub(super) fn apply(page: &mut Page, theme: Theme, appearance: Appearance) {
    if appearance == Appearance::Original {
        return;
    }
    let mut palette = theme.canvas_for_page(&page.properties);
    if appearance == Appearance::Print {
        palette.paper = 0xffffff;
        palette.foreground = 0x171717;
        palette.adapt_ink = true;
    }
    if page.properties.pdf.is_none() {
        page.properties.color = Some(Color::from_rgb(palette.paper));
    }
    for object in page.objects.values_mut() {
        match Arc::make_mut(object) {
            Object::Stroke(s) => s.style.color = Color::from_rgb(palette.ink(s.style.color.rgb())),
            Object::Shape(s) => s.style.color = Color::from_rgb(palette.ink(s.style.color.rgb())),
            Object::Text(t) => t.color = Color::from_rgb(palette.ink(t.color.rgb())),
            Object::Equation(e) => {
                if let Some(svg) = &e.rendered_svg {
                    let svg = if e.math_link.as_ref().is_some_and(|l| l.operation == "graph") {
                        let mut graph = theme.graph_for_page(&page.properties);
                        if appearance == Appearance::Print {
                            graph.paper = palette.paper;
                            graph.labels = palette.foreground;
                            graph.curve = palette.ink(graph.curve);
                        }
                        graph.svg(svg)
                    } else {
                        svg.clone()
                    };
                    e.rendered_svg = Some(ink_svg(&svg, palette));
                }
            }
            Object::Image(_) => {}
        }
    }
}
fn ink_svg(svg: &str, palette: theme::CanvasTheme) -> String {
    let mut output = svg.to_string();
    for quote in ['"', '\''] {
        let parts: Vec<_> = output.split(quote).collect();
        let mut rebuilt = String::new();
        for (i, part) in parts.iter().enumerate() {
            if i > 0 {
                rebuilt.push(quote);
            }
            let attribute = i % 2 == 1
                && ["fill=", "stroke=", "color="]
                    .iter()
                    .any(|a| parts[i - 1].trim_end().ends_with(a));
            let color = if attribute {
                match *part {
                    "black" | "currentColor" => Some(0),
                    "white" => Some(0xffffff),
                    p if p.starts_with('#') && p.len() == 7 => {
                        u32::from_str_radix(&p[1..], 16).ok()
                    }
                    p if p.starts_with('#') && p.len() == 4 => u32::from_str_radix(
                        &format!("{0}{0}{1}{1}{2}{2}", &p[1..2], &p[2..3], &p[3..4]),
                        16,
                    )
                    .ok(),
                    _ => None,
                }
            } else {
                None
            };
            if let Some(color) = color {
                rebuilt.push_str(&format!("#{:06x}", palette.ink(color)));
            } else {
                rebuilt.push_str(part);
            }
        }
        output = rebuilt;
    }
    output
}
impl NotesView {
    pub(super) fn export_appearance(&mut self, cx: &mut Context<Self>) -> Div {
        let theme = Theme::new(&self.controller.settings);
        let page = self.controller.page();
        let key = (
            page.id,
            page.revision,
            page.properties.clone(),
            theme.canvas_for_page(&page.properties),
            self.export_options.appearance,
        );
        if self.export_options.key.as_ref() != Some(&key) {
            self.export_options.key = Some(key);
            self.export_options.generation += 1;
            let generation = self.export_options.generation;
            self.export_options.image = None;
            self.export_options.error = None;
            let mut page = page.clone();
            let appearance = self.export_options.appearance;
            let assets = self.controller.assets.clone();
            let task = cx.background_executor().spawn(async move {
                apply(&mut page, theme, appearance);
                folio_export::raster_page_limited(&page, &assets, 512)
                    .map(|p| graph::image(p.width(), p.height(), p.take()))
                    .map_err(|e| e.to_string())
            });
            cx.spawn(async move |view, cx| {
                let result = task.await;
                let _ = view.update(cx, |view, cx| {
                    if view.export_options.generation != generation {
                        return;
                    }
                    match result {
                        Ok(image) => view.export_options.image = Some(image),
                        Err(e) => view.export_options.error = Some(e),
                    }
                    cx.notify();
                });
            })
            .detach();
        }
        let mut panel = div()
            .flex()
            .flex_col()
            .gap_2()
            .child(div().text_sm().child("PDF, SVG and PNG appearance"));
        for (id, label, mode) in [
            ("export-original", "Original colors", Appearance::Original),
            ("export-visible", "Visible appearance", Appearance::Visible),
            (
                "export-print",
                "Light paper for printing",
                Appearance::Print,
            ),
        ] {
            panel = panel.child(
                self.button(
                    id,
                    label,
                    self.export_options.appearance == mode,
                    cx,
                    move |this, _, _| this.export_options.appearance = mode,
                )
                .justify_start()
                .text_xs(),
            );
        }
        panel = panel.child(div().text_xs().child(format!(
            "Preview · page {} of {}",
            self.controller.session().page + 1,
            self.controller.session().document.pages.len()
        )));
        let mut preview = div()
            .h(px(130.))
            .flex()
            .items_center()
            .justify_center()
            .overflow_hidden();
        if let Some(image) = &self.export_options.image {
            preview = preview.child(
                img(image.clone())
                    .size_full()
                    .object_fit(ObjectFit::Contain),
            );
        } else {
            preview = preview.child(
                div().text_xs().child(
                    self.export_options
                        .error
                        .clone()
                        .unwrap_or_else(|| "Rendering preview…".into()),
                ),
            );
        }
        panel.child(preview).child(div().text_xs().child("Editable documents retain original colors and all pages. Text export contains typed content from all pages. PDF backgrounds and photographs keep their colors."))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[::core::prelude::v1::test]
    fn appearance_changes_only_export_copies_and_keeps_media() {
        let mut page = Page::new();
        page.properties.color = Some(Color::from_rgb(0x000000));
        let original = page.clone();
        let mut settings = folio_app::Settings::default();
        settings.appearance.adapt_ink = true;
        let theme = Theme::new(&settings);
        apply(&mut page, theme, Appearance::Print);
        assert_eq!(page.properties.color, Some(Color::from_rgb(0xffffff)));
        assert_eq!(original.properties.color, Some(Color::from_rgb(0)));
        let adjusted = ink_svg(
            r##"<svg><path fill="#000000"/><path fill='#ffffff'/></svg>"##,
            theme.canvas_for_page(&original.properties),
        );
        assert!(!adjusted.contains("fill=\"#000000\""));
        assert!(adjusted.contains("fill='#ffffff'"));
    }
}
