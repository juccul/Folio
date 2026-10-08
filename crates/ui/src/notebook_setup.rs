use super::{Field, FieldBoundsChanged, NotesView, Submitted, Theme};
use folio_document::{Color, PageProperties, Paper};
use gpui::{prelude::*, *};

pub(super) struct Setup {
    pub properties: PageProperties,
    pub color: Entity<Field>,
    pub custom_color_open: bool,
}

impl NotesView {
    pub(super) fn prepare_notebook_setup(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let properties = self.controller.default_page_properties();
        let initial_color = properties
            .color
            .map(|color| color.hex())
            .unwrap_or_default();
        let color = cx.new(|cx| Field::new(initial_color, false, cx));
        self.notebook_setup = Some(Setup {
            properties,
            color,
            custom_color_open: false,
        });
    }

    pub(super) fn observe_notebook_setup(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(setup) = &self.notebook_setup else {
            return;
        };
        let color = setup.color.clone();
        self.subscriptions.push(cx.observe(&color, |this, _, cx| {
            this.modal_error = None;
            cx.notify();
        }));
        self.subscriptions
            .push(cx.subscribe(&color, |_, _, _: &FieldBoundsChanged, cx| cx.notify()));
        self.subscriptions.push(cx.subscribe_in(
            &color,
            window,
            |this, _, _: &Submitted, window, cx| this.submit_modal(window, cx),
        ));
    }

    pub(super) fn create_configured_notebook(
        &mut self,
        name: String,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let setup = self
            .notebook_setup
            .as_ref()
            .ok_or("Document setup is unavailable")?;
        let mut properties = setup.properties.clone();
        properties.color = paper_color(&setup.color.read(cx).content)?;
        let name = if name.trim().is_empty() {
            "Untitled document".into()
        } else {
            name
        };
        self.controller
            .create_note_with_properties(name, properties)?;
        Ok(())
    }

    pub(super) fn notebook_setup_options(&mut self, cx: &mut Context<Self>) -> Div {
        let Some(setup) = &self.notebook_setup else {
            return div();
        };
        let mut properties = setup.properties.clone();
        let color = setup.color.clone();
        let color_value = color.read(cx).content.clone();
        let custom_color_open = setup.custom_color_open;
        if let Ok(value) = paper_color(&color_value) {
            properties.color = value;
        }
        let theme = Theme::new(&self.controller.settings);
        color.update(cx, |field, _| field.theme = theme);
        let label =
            |text: &'static str| div().text_sm().font_weight(FontWeight::MEDIUM).child(text);
        let mut layout = div().flex().gap_2();
        for (id, title, subtitle, infinite) in [
            ("canvas-pages", "Pages", "Separate sheets", false),
            (
                "canvas-infinite",
                "Infinite canvas",
                "Room in every direction",
                true,
            ),
        ] {
            let content = div()
                .flex()
                .flex_col()
                .items_start()
                .gap_1()
                .child(div().font_weight(FontWeight::MEDIUM).child(title))
                .child(div().text_xs().text_color(rgb(theme.muted)).child(subtitle));
            layout = layout.child(
                self.control(
                    id,
                    title,
                    content.into_any_element(),
                    properties.infinite == infinite,
                    cx,
                    move |this, _, _| {
                        if let Some(setup) = &mut this.notebook_setup {
                            setup.properties.infinite = infinite;
                        }
                    },
                )
                .flex_1()
                .justify_start()
                .p_3()
                .border_color(theme.border),
            );
        }
        let mut patterns = div().flex().gap_2().flex_wrap();
        for (id, title, paper) in [
            ("paper-blank", "Blank", Paper::Blank),
            ("paper-lines", "Lines", Paper::Ruled),
            ("paper-grid", "Grid", Paper::Grid),
            ("paper-dots", "Dots", Paper::Dots),
        ] {
            patterns = patterns.child(
                self.button(
                    id,
                    title,
                    properties.paper == paper,
                    cx,
                    move |this, _, _| {
                        if let Some(setup) = &mut this.notebook_setup {
                            setup.properties.paper = paper;
                        }
                    },
                )
                .flex_1()
                .border_color(theme.border),
            );
        }
        let mut options = div()
            .flex()
            .flex_col()
            .gap_3()
            .child(label("Canvas"))
            .child(layout)
            .child(label("Paper pattern"))
            .child(patterns);
        if !properties.infinite {
            let landscape = properties.width > properties.height;
            let dimensions = (
                properties.width.min(properties.height),
                properties.width.max(properties.height),
            );
            let mut sizes = div().flex().flex_wrap().gap_2();
            for (id, title, width, height) in [
                (
                    "size-a4",
                    "A4 · 210 × 297 mm",
                    210. / 25.4 * 96.,
                    297. / 25.4 * 96.,
                ),
                (
                    "size-a5",
                    "A5 · 148 × 210 mm",
                    148. / 25.4 * 96.,
                    210. / 25.4 * 96.,
                ),
                ("size-letter", "Letter · 8.5 × 11 in", 816., 1056.),
            ] {
                sizes = sizes.child(
                    self.button(
                        id,
                        title,
                        (dimensions.0 - width).abs() < 1. && (dimensions.1 - height).abs() < 1.,
                        cx,
                        move |this, _, _| {
                            if let Some(setup) = &mut this.notebook_setup {
                                (setup.properties.width, setup.properties.height) = if landscape {
                                    (height, width)
                                } else {
                                    (width, height)
                                };
                            }
                        },
                    )
                    .border_color(theme.border),
                );
            }
            let mut orientations = div().flex().gap_2();
            for (id, title, wide) in [
                ("orientation-portrait", "Portrait", false),
                ("orientation-landscape", "Landscape", true),
            ] {
                orientations = orientations.child(
                    self.button(id, title, landscape == wide, cx, move |this, _, _| {
                        if let Some(setup) = &mut this.notebook_setup {
                            let short = setup.properties.width.min(setup.properties.height);
                            let long = setup.properties.width.max(setup.properties.height);
                            (setup.properties.width, setup.properties.height) =
                                if wide { (long, short) } else { (short, long) };
                        }
                    })
                    .border_color(theme.border),
                );
            }
            options = options
                .child(label("Page size"))
                .child(sizes)
                .child(orientations);
        }
        let mut colors = div().flex().items_center().gap_2().flex_wrap().child(
            self.button(
                "paper-theme",
                "Theme",
                color_value.trim().is_empty(),
                cx,
                |this, _, cx| {
                    if let Some(setup) = &this.notebook_setup {
                        setup
                            .color
                            .update(cx, |field, cx| field.set_content(String::new(), cx));
                    }
                },
            )
            .border_color(theme.border),
        );
        for (id, title, value) in [
            ("paper-white", "White", 0xffffff),
            ("paper-cream", "Cream", 0xfff7e6),
            ("paper-sage", "Sage", 0xeaf2e8),
            ("paper-blue", "Blue", 0xeaf1fb),
            ("paper-charcoal", "Charcoal", 0x202124),
            ("paper-black", "Black", 0x0e0e0e),
        ] {
            let selected =
                properties.color == Some(Color::from_rgb(value)) && !color_value.trim().is_empty();
            let swatch = div()
                .size(px(22.))
                .rounded_full()
                .bg(rgb(value))
                .border_1()
                .border_color(theme.border);
            colors = colors.child(
                self.control(
                    id,
                    title,
                    swatch.into_any_element(),
                    selected,
                    cx,
                    move |this, _, cx| {
                        if let Some(setup) = &this.notebook_setup {
                            setup.color.update(cx, |field, cx| {
                                field.set_content(Color::from_rgb(value).hex(), cx)
                            });
                        }
                    },
                )
                .size(px(38.))
                .p_1()
                .border_color(if selected {
                    rgb(theme.ring)
                } else {
                    theme.border
                }),
            );
        }
        colors = colors.child(self.button(
            "paper-custom-picker",
            "Custom…",
            custom_color_open,
            cx,
            |this, _, _| {
                if let Some(setup) = &mut this.notebook_setup {
                    setup.custom_color_open = !setup.custom_color_open;
                }
            },
        ));
        options = options
            .child(label("Paper color"))
            .child(colors)
            .child(
                div()
                    .text_xs()
                    .text_color(rgb(theme.muted))
                    .child("Custom color · leave the hex value empty to follow the theme"),
            )
            .child(color.clone());
        if custom_color_open {
            options = options.child(self.color_selector(color, false, cx));
        }
        let preview_theme = theme.canvas_for_page(&properties);
        let preview = paper_preview(&properties, preview_theme);
        div()
            .flex()
            .gap_5()
            .items_start()
            .child(options.flex_1().min_w_0())
            .child(
                div()
                    .w(px(154.))
                    .flex_shrink_0()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .items_center()
                    .child(label("Preview"))
                    .child(preview)
                    .child(div().text_xs().text_color(rgb(theme.muted)).child(
                        if properties.infinite {
                            "An endless writing surface"
                        } else {
                            "New pages use this paper"
                        },
                    )),
            )
    }
}

fn paper_color(content: &str) -> Result<Option<Color>, String> {
    let content = content.trim();
    if content.is_empty() {
        return Ok(None);
    }
    let hex = content.strip_prefix('#').unwrap_or(content);
    if hex.len() != 6 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("Use a six-digit paper color, such as #fff7e6.".into());
    }
    u32::from_str_radix(hex, 16)
        .map(|value| Some(Color::from_rgb(value)))
        .map_err(|_| "Use a six-digit paper color, such as #fff7e6.".into())
}

fn paper_preview(properties: &PageProperties, theme: super::theme::CanvasTheme) -> Div {
    let width = 150.;
    let height = if properties.infinite {
        172.
    } else {
        (width * properties.height / properties.width).clamp(100., 214.)
    };
    let paper = properties.paper;
    div()
        .w(px(width))
        .h(px(height))
        .overflow_hidden()
        .rounded_sm()
        .border_1()
        .border_color(rgb(theme.grid))
        .bg(rgb(theme.paper))
        .child(
            canvas(
                |_, _, _| (),
                move |bounds, _, window, _| {
                    window.with_content_mask(Some(ContentMask { bounds }), |window| {
                        let x = f32::from(bounds.origin.x);
                        let y = f32::from(bounds.origin.y);
                        let w = f32::from(bounds.size.width);
                        let h = f32::from(bounds.size.height);
                        let mut lines = PathBuilder::stroke(px(0.7));
                        match paper {
                            Paper::Blank => return,
                            Paper::Ruled | Paper::Grid => {
                                let mut at = 16.;
                                while at < h {
                                    lines.move_to(point(px(x), px(y + at)));
                                    lines.line_to(point(px(x + w), px(y + at)));
                                    at += 16.;
                                }
                                if paper == Paper::Grid {
                                    let mut at = 16.;
                                    while at < w {
                                        lines.move_to(point(px(x + at), px(y)));
                                        lines.line_to(point(px(x + at), px(y + h)));
                                        at += 16.;
                                    }
                                }
                            }
                            Paper::Dots => {
                                let mut dy = 8.;
                                while dy < h {
                                    let mut dx = 8.;
                                    while dx < w {
                                        window.paint_quad(fill(
                                            Bounds::new(
                                                point(px(x + dx), px(y + dy)),
                                                size(px(1.5), px(1.5)),
                                            ),
                                            rgb(theme.dots),
                                        ));
                                        dx += 16.;
                                    }
                                    dy += 16.;
                                }
                                return;
                            }
                        }
                        if let Ok(path) = lines.build() {
                            window.paint_path(path, rgb(theme.grid));
                        }
                    });
                },
            )
            .size_full(),
        )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[::core::prelude::v1::test]
    fn paper_colors_accept_theme_and_rgb_but_reject_invalid_input() {
        assert_eq!(paper_color(" "), Ok(None));
        assert_eq!(
            paper_color(" #fff7e6 "),
            Ok(Some(Color::from_rgb(0xfff7e6)))
        );
        for value in ["blue", "#fff", "#fff7e6ff", "#12xx12"] {
            assert!(paper_color(value).is_err());
        }
    }
}
