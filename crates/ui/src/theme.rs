//! Shared semantic theme for native controls and display-only canvas colors.
use folio_app::{
    Settings,
    appearance::{ThemeColor, ThemeToken},
};
use gpui::{
    BoxShadow, Hsla, Rgba, Styled, linear_color_stop, linear_gradient, point, px, rgb, rgba,
};
use std::borrow::Cow;
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CanvasTheme {
    pub paper: u32,
    pub grid: u32,
    pub dots: u32,
    pub foreground: u32,
    pub adapt_ink: bool,
}
impl CanvasTheme {
    pub fn ink(self, color: u32) -> u32 {
        self.readable(color, self.ink_contrast())
    }
    /// Dim ink on dark paper reads worse than the same ratio on light paper,
    /// so dark pages ask for well above the AAA ratio.
    fn ink_contrast(self) -> f32 {
        if luminance(self.paper) < 0.18 {
            8.5
        } else {
            4.5
        }
    }
    fn readable(self, color: u32, minimum: f32) -> u32 {
        if !self.adapt_ink || contrast(color, self.paper) >= minimum {
            return color;
        }
        readable_ink(color, self.paper, minimum)
    }
    pub fn preview_pixels(self, bgra: &[u8]) -> Cow<'_, [u8]> {
        let mut output = Cow::Borrowed(bgra);
        if !self.adapt_ink {
            return output;
        }
        let mut colors = std::collections::HashMap::new();
        for (index, pixel) in bgra.as_chunks::<4>().0.iter().enumerate() {
            if pixel[3] == 0 {
                continue;
            }
            let color = ((pixel[2] as u32) << 16) | ((pixel[1] as u32) << 8) | pixel[0] as u32;
            let adjusted = *colors
                .entry(color)
                .or_insert_with(|| self.readable(color, self.ink_contrast()));
            if adjusted != color {
                // Most light-paper previews already have readable colors.
                // Allocate only when an actual color change is necessary.
                let pixel = &mut output.to_mut()[index * 4..index * 4 + 4];
                pixel[0] = adjusted as u8;
                pixel[1] = (adjusted >> 8) as u8;
                pixel[2] = (adjusted >> 16) as u8;
            }
        }
        output
    }
}
/// Change only lightness, preserving hue and saturation. Search both directions
/// and choose the closest RGB result that meets the requested contrast.
fn readable_ink(color: u32, paper: u32, minimum: f32) -> u32 {
    let source: Hsla = rgb(color).into();
    let at_lightness = |lightness| {
        let value = Hsla {
            l: lightness,
            ..source
        }
        .to_rgb();
        let channel = |value: f32| (value.clamp(0., 1.) * 255.).round() as u32;
        (channel(value.r) << 16) | (channel(value.g) << 8) | channel(value.b)
    };
    let distance = |candidate: u32| {
        [16, 8, 0]
            .into_iter()
            .map(|shift| {
                let delta = ((candidate >> shift) & 255) as i32 - ((color >> shift) & 255) as i32;
                delta * delta
            })
            .sum::<i32>()
    };
    [0., 1.]
        .into_iter()
        .filter_map(|end| {
            let mut candidate = at_lightness(end);
            if contrast(candidate, paper) < minimum {
                return None;
            }
            let (mut low, mut high) = (0., 1.);
            for _ in 0..18 {
                let amount = (low + high) / 2.;
                let next = at_lightness(source.l + (end - source.l) * amount);
                if contrast(next, paper) >= minimum {
                    high = amount;
                    candidate = next;
                } else {
                    low = amount;
                }
            }
            Some(candidate)
        })
        .min_by_key(|candidate| distance(*candidate))
        // Use the better endpoint if the caller asks for a contrast ratio that
        // is unattainable on the selected paper.
        .unwrap_or_else(|| {
            if contrast(0xffffff, paper) > contrast(0, paper) {
                0xffffff
            } else {
                0
            }
        })
}
#[derive(Clone, Copy)]
pub struct Theme {
    pub bg: u32,
    pub surface: u32,
    pub popover: u32,
    pub sidebar: u32,
    pub ink: u32,
    pub muted: u32,
    pub accent: u32,
    pub selected: u32,
    pub chrome: u32,
    pub primary_foreground: u32,
    pub sidebar_accent: u32,
    pub destructive: u32,
    pub ring: u32,
    pub border: Rgba,
    pub input: Rgba,
    pub radius: f32,
    pub canvas: CanvasTheme,
    pub dark: bool,
    /// Opacity of glass surfaces, 0.3–1.
    pub glass_opacity: f32,
    /// Blur radius behind glass surfaces, in logical pixels; 0 disables it.
    pub glass_blur: f32,
}
fn native(color: ThemeColor) -> Rgba {
    rgba(color.0)
}
impl Theme {
    pub fn canvas_for_page(self, properties: &folio_document::PageProperties) -> CanvasTheme {
        let mut canvas = self.page_canvas(properties.pdf.is_some());
        if properties.pdf.is_none()
            && let Some(color) = properties.color
        {
            canvas.paper = color.rgb();
            canvas.foreground =
                if contrast(0xffffff, canvas.paper) > contrast(0x000000, canvas.paper) {
                    0xffffff
                } else {
                    0x000000
                };
            canvas.grid = mix(canvas.paper, canvas.foreground, 0.14);
            canvas.dots = mix(canvas.paper, canvas.foreground, 0.24);
        }
        canvas
    }
    pub fn graph_for_page(
        self,
        properties: &folio_document::PageProperties,
    ) -> super::graph::Palette {
        self.graph_for_canvas(self.canvas_for_page(properties))
    }
    pub fn page_canvas(self, pdf: bool) -> CanvasTheme {
        if pdf {
            CanvasTheme {
                paper: 0xffffff,
                grid: 0xe5e5e5,
                dots: 0xd4d4d4,
                foreground: 0x171717,
                adapt_ink: self.canvas.adapt_ink,
            }
        } else {
            self.canvas
        }
    }
    #[cfg(test)]
    pub fn graph(self, pdf: bool) -> super::graph::Palette {
        self.graph_for_canvas(self.page_canvas(pdf))
    }
    fn graph_for_canvas(self, page: CanvasTheme) -> super::graph::Palette {
        // Generated graphs always need readable labels and curves, including
        // when the user preserves original handwriting colors.
        let readable = CanvasTheme {
            adapt_ink: true,
            ..page
        };
        super::graph::Palette {
            paper: page.paper,
            grid: page.grid,
            axes: mix(page.paper, page.foreground, 0.5),
            labels: page.foreground,
            // Generated curves keep the accent wherever it is readable.
            curve: readable.readable(self.accent, 4.5),
        }
    }
    pub fn new(settings: &Settings) -> Self {
        use ThemeToken::*;
        let resolve = |token| settings.appearance.color(settings.dark, token);
        let color = |token| resolve(token).rgb();
        let paper = if settings.appearance.canvas_follows_theme {
            // Dark paper sits just above the workspace black, so pages and
            // previews keep a visible edge and ink keeps its depth.
            if settings.dark {
                mix(color(Background), 0xffffff, 0.06)
            } else {
                color(Background)
            }
        } else {
            settings
                .appearance
                .canvas_color
                .map(|c| c.rgb())
                .unwrap_or(0xffffff)
        };
        let foreground = if contrast(color(Foreground), paper) >= 4.5 {
            color(Foreground)
        } else if contrast(0xffffff, paper) >= contrast(0x000000, paper) {
            0xffffff
        } else {
            0x000000
        };
        let grid = mix(paper, foreground, 0.14);
        let dots = mix(paper, foreground, 0.24);
        Self {
            bg: color(Background),
            surface: color(Card),
            popover: color(Popover),
            sidebar: color(Sidebar),
            ink: color(Foreground),
            muted: color(MutedForeground),
            accent: color(Primary),
            selected: color(Secondary),
            chrome: color(Card),
            primary_foreground: color(PrimaryForeground),
            sidebar_accent: color(SidebarPrimary),
            destructive: color(Destructive),
            ring: color(Ring),
            border: native(resolve(Border)),
            input: native(resolve(Input)),
            radius: settings.appearance.radius(),
            canvas: CanvasTheme {
                paper,
                grid,
                dots,
                foreground,
                adapt_ink: settings.appearance.adapt_ink,
            },
            dark: settings.dark,
            glass_opacity: settings.appearance.glass_opacity(),
            glass_blur: settings.appearance.glass_blur(),
        }
    }
    /// Corner radius of cards and floating menus that contain controls, so
    /// the controls' corners stay concentric with theirs.
    pub fn corner_md(self) -> f32 {
        if self.radius > 0. { self.radius + 2. } else { 0. }
    }
    /// Corner radius of dialogs.
    pub fn corner_lg(self) -> f32 {
        if self.radius > 0. { self.radius + 4. } else { 0. }
    }
    /// Corner radius of small marks: swatches, hints and previews.
    pub fn corner_sm(self) -> f32 {
        self.radius * 0.5
    }
    /// The highlight of a hovered or active control on glass, `amount` from
    /// clear (0) to fully highlighted (1). A translucent wash of the text
    /// color, so highlights never read as solid blocks over glass.
    pub fn tint(self, amount: f32) -> Rgba {
        let strength = if self.dark { 0x24 } else { 0x16 };
        rgba((self.ink << 8) | (amount.clamp(0., 1.) * strength as f32).round() as u32)
    }
    /// A raised area within a glass surface, such as a card or input.
    pub fn raised(self) -> Rgba {
        if self.dark {
            rgba(0xffffff0d)
        } else {
            rgba(0xffffff8c)
        }
    }
    /// `color` at the glass opacity.
    pub fn translucent(self, color: u32) -> Rgba {
        self.at_opacity(color, self.glass_opacity)
    }
    fn at_opacity(self, color: u32, opacity: f32) -> Rgba {
        rgba((color << 8) | (opacity.clamp(0., 1.) * 255.).round() as u32)
    }
    /// A card within a glass dialog: nearly solid and slightly lifted, so
    /// grouped settings stay easy to read over the blurred backdrop.
    pub fn card(self) -> Rgba {
        let lift = if self.dark { 0.05 } else { 0. };
        self.at_opacity(mix(self.surface, self.ink, lift), 0.94)
    }
    /// Selection outlines, handles and lasso: a blue that stays distinct
    /// from ink and paper in both modes.
    pub fn selection(self) -> u32 {
        if self.dark { 0x60a5fa } else { 0x2563eb }
    }
    /// The sidebar accent, lightened or darkened as needed to read as an
    /// icon on the sidebar.
    pub fn sidebar_icon(self) -> u32 {
        if contrast(self.sidebar_accent, self.sidebar) >= 4.5 {
            self.sidebar_accent
        } else {
            readable_ink(self.sidebar_accent, self.sidebar, 4.5)
        }
    }
    /// Secondary text and idle icons in dense lists: between muted and ink.
    pub fn secondary(self) -> u32 {
        mix(self.muted, self.ink, 0.25)
    }
    /// The frame of a page preview: stronger than glass so a blank page
    /// stays visible against the surface in both modes.
    pub fn cover_edge(self) -> Rgba {
        if self.dark {
            rgba(0xffffff33)
        } else {
            rgba(0x00000024)
        }
    }
    /// The hairline edge of glass: a faint light rim in dark mode and a faint
    /// shade in light mode, so glass stays distinct over similar content.
    pub fn glass_edge(self) -> Rgba {
        if self.dark {
            rgba(0xffffff1f)
        } else {
            rgba(0x0000001a)
        }
    }
}

/// Frosted-glass surfaces: a translucent fill over a blurred backdrop.
pub trait Glass: Styled + Sized {
    /// A docked glass surface, such as a bar or sidebar.
    fn glass_fill(self, theme: Theme, base: u32) -> Self {
        self.glass_fill_at(theme, base, theme.glass_opacity, theme.glass_blur)
    }
    /// A glass fill at an explicit opacity and blur radius.
    fn glass_fill_at(self, theme: Theme, base: u32, opacity: f32, blur: f32) -> Self {
        let surface = self.bg(theme.at_opacity(base, opacity));
        if blur > 0. {
            surface.backdrop_blur(px(blur))
        } else {
            surface
        }
    }
    /// A modal dialog over a scrim, such as Settings or Open a document:
    /// denser and more strongly blurred than menus, while still following the
    /// glass settings. Every dialog shares this so their surfaces match.
    fn glass_dialog(self, theme: Theme, base: u32) -> Self {
        let opacity = theme.glass_opacity + (1. - theme.glass_opacity) * 0.35;
        self.glass_fill_at(theme, base, opacity, theme.glass_blur * 1.75)
            .glass_frame(theme)
    }
    /// A glass bar docked to a window edge: most opaque along that edge,
    /// fading to the glass opacity away from it. `from_top` selects the edge.
    fn glass_fill_from_edge(self, theme: Theme, base: u32, from_top: bool) -> Self {
        let edge = theme.glass_opacity + (1. - theme.glass_opacity) * 0.75;
        let surface = self.bg(linear_gradient(
            if from_top { 180. } else { 0. },
            linear_color_stop(theme.at_opacity(base, edge), 0.),
            linear_color_stop(theme.translucent(base), 1.),
        ));
        if theme.glass_blur > 0. {
            surface.backdrop_blur(px(theme.glass_blur))
        } else {
            surface
        }
    }
    /// The dimmed, softly blurred backdrop behind a dialog.
    fn scrim(self, theme: Theme) -> Self {
        let dim = self.bg(rgba(if theme.dark { 0x00000066 } else { 0x0000003d }));
        if theme.glass_blur > 0. {
            dim.backdrop_blur(px(theme.glass_blur * 0.4))
        } else {
            dim
        }
    }
    /// A floating glass surface, such as a menu or dialog, with a hairline
    /// edge and a soft shadow.
    fn glass(self, theme: Theme, base: u32) -> Self {
        // In dark mode floating surfaces sit a step above the workspace.
        let base = if theme.dark {
            mix(base, 0xffffff, 0.05)
        } else {
            base
        };
        self.glass_fill(theme, base).glass_frame(theme)
    }
    /// The hairline edge and soft shadow of floating glass.
    fn glass_frame(self, theme: Theme) -> Self {
        // Black at this alpha.
        let shadow: u32 = if theme.dark { 0x66 } else { 0x2e };
        self.border_1()
            .border_color(theme.glass_edge())
            .shadow(vec![
                BoxShadow {
                    color: rgba(shadow).into(),
                    offset: point(px(0.), px(12.)),
                    blur_radius: px(32.),
                    spread_radius: px(-4.),
                },
                BoxShadow {
                    color: rgba(shadow / 2).into(),
                    offset: point(px(0.), px(2.)),
                    blur_radius: px(6.),
                    spread_radius: px(0.),
                },
            ])
    }
}
impl<T: Styled> Glass for T {}
pub fn mix(a: u32, b: u32, amount: f32) -> u32 {
    let channel = |shift: u32| {
        let a = ((a >> shift) & 255) as f32;
        let b = ((b >> shift) & 255) as f32;
        (a + (b - a) * amount).round() as u32
    };
    (channel(16) << 16) | (channel(8) << 8) | channel(0)
}
/// Pen cursor ring and halo for the background under the pointer. The ring is
/// the inverted background, or black/white where inversion stays mid-gray; the
/// halo inverts the ring so it remains outlined over ink, patterns and images.
pub fn cursor_colors(background: u32) -> (u32, u32) {
    let mut ring = !background & 0xffffff;
    if contrast(ring, background) < 3. {
        ring = if contrast(0xffffff, background) >= contrast(0x000000, background) {
            0xffffff
        } else {
            0x000000
        };
    }
    (ring, !ring & 0xffffff)
}
fn luminance(color: u32) -> f32 {
    let channel = |shift: u32| {
        let x = ((color >> shift) & 255) as f32 / 255.;
        if x <= 0.04045 {
            x / 12.92
        } else {
            ((x + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * channel(16) + 0.7152 * channel(8) + 0.0722 * channel(0)
}
fn contrast(a: u32, b: u32) -> f32 {
    let a = luminance(a);
    let b = luminance(b);
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn document_paper_colors_apply_to_canvas_and_graphs_without_affecting_other_notes() {
        let mut settings = Settings::default();
        settings.appearance.adapt_ink = true;
        let theme = Theme::new(&settings);
        let mut properties = folio_document::PageProperties::default();
        for paper in [0xffffff, 0xfff7e6, 0x202124, 0x0e0e0e] {
            properties.color = Some(folio_document::Color::from_rgb(paper));
            let canvas = theme.canvas_for_page(&properties);
            let graph = theme.graph_for_page(&properties);
            assert_eq!(canvas.paper, paper);
            assert_eq!(graph.paper, paper);
            assert!(contrast(canvas.ink(0x2b3934), paper) >= 3.);
            assert!(contrast(graph.labels, paper) >= 4.5);
        }
        assert_eq!(cursor_colors(0xffffff), (0x000000, 0xffffff));
        assert_eq!(cursor_colors(0x1a3c8c).0, 0xe5c373);
        for paper in [0xffffff, 0x000000, 0x808080, 0x7f7f7f, 0x1a3c8c, 0xe03030, 0x30c060] {
            let (ring, halo) = cursor_colors(paper);
            assert!(contrast(ring, paper) >= 3., "{paper:06x}");
            assert!(contrast(ring, halo) >= 3., "{paper:06x}");
        }
        assert_eq!(
            theme.canvas_for_page(&folio_document::PageProperties::default()),
            theme.canvas
        );
        properties.pdf = Some(folio_document::PdfBackground {
            asset: "test.pdf".into(),
            page: 1,
            preview_asset: None,
        });
        assert_eq!(theme.canvas_for_page(&properties).paper, 0xffffff);
    }
    #[test]
    fn graphs_follow_custom_paper_and_accent_with_readable_colors() {
        let mut settings = Settings::default();
        settings.appearance.canvas_follows_theme = false;
        settings.appearance.canvas_color = Some(folio_document::Color::from_rgb(0x182522));
        settings.appearance.adapt_ink = false;
        settings
            .appearance
            .set_color(false, ThemeToken::Primary, ThemeColor::opaque(0x77bdab));
        let theme = Theme::new(&settings);
        let graph = theme.graph(false);
        assert_eq!(graph.paper, 0x182522);
        assert_eq!(graph.grid, theme.canvas.grid);
        assert_eq!(graph.curve, 0x77bdab);
        assert!(contrast(graph.labels, graph.paper) >= 4.5);
        assert!(contrast(graph.curve, graph.paper) >= 3.);
        settings.dark = true;
        let pdf_graph = Theme::new(&settings).graph(true);
        assert_eq!(pdf_graph.paper, 0xffffff);
        assert!(contrast(pdf_graph.labels, pdf_graph.paper) >= 4.5);
        assert!(contrast(pdf_graph.curve, pdf_graph.paper) >= 3.);
    }
    #[test]
    fn themed_and_fixed_paper_adapt_ink_without_changing_source_colors() {
        let mut settings = Settings {
            dark: true,
            ..Default::default()
        };
        settings.appearance.adapt_ink = true;
        let theme = Theme::new(&settings);
        assert_eq!(theme.canvas.paper, 0x191919);
        assert!(contrast(theme.canvas.ink(0x2b3934), theme.canvas.paper) >= 3.);
        assert_eq!(theme.canvas.ink(0xffffff), 0xffffff);
        settings.appearance.canvas_follows_theme = false;
        let fixed = Theme::new(&settings);
        assert_eq!(fixed.canvas.paper, 0xffffff);
        assert_eq!(fixed.canvas.ink(0x2b3934), 0x2b3934);
        settings.appearance.adapt_ink = false;
        settings.appearance.canvas_follows_theme = true;
        assert_eq!(Theme::new(&settings).canvas.ink(0x2b3934), 0x2b3934);
    }
    #[test]
    fn text_preview_alpha_and_colored_image_source_are_preserved() {
        let mut settings = Settings {
            dark: true,
            ..Default::default()
        };
        settings.appearance.adapt_ink = true;
        let input = [0, 0, 0, 255, 43, 57, 52, 128, 0, 0, 0, 0];
        let output = Theme::new(&settings).canvas.preview_pixels(&input);
        assert_eq!([output[3], output[7], output[11]], [255, 128, 0]);
        assert!(
            contrast(
                ((output[2] as u32) << 16) | ((output[1] as u32) << 8) | output[0] as u32,
                0x0a0a0a
            ) >= 4.5
        );
        assert_eq!(&output[8..], &input[8..]);
        assert_eq!(input[0], 0);
        assert!(matches!(output, Cow::Owned(_)));
        // Transparent pixels need no adjustment, and already-readable text
        // must not require another full-size allocation.
        let readable = [255, 255, 255, 255, 0, 0, 0, 0];
        let theme = Theme::new(&settings).canvas;
        let unchanged = theme.preview_pixels(&readable);
        assert!(matches!(unchanged, Cow::Borrowed(_)));
        assert_eq!(unchanged.as_ptr(), readable.as_ptr());
        settings.appearance.adapt_ink = false;
        let original = Theme::new(&settings).canvas.preview_pixels(&input);
        assert!(matches!(original, Cow::Borrowed(_)));
        assert_eq!(original.as_ptr(), input.as_ptr());
    }
    #[test]
    fn custom_mid_gray_paper_chooses_a_readable_foreground() {
        let mut settings = Settings::default();
        settings.appearance.adapt_ink = true;
        settings.appearance.canvas_follows_theme = false;
        settings.appearance.canvas_color = Some(folio_document::Color::from_rgb(0x808080));
        let paper = Theme::new(&settings).canvas;
        for color in [0xffffff, 0x808080, 0x0a0a0a] {
            assert!(contrast(paper.ink(color), paper.paper) >= 3.);
        }
        let text = paper.preview_pixels(&[255, 255, 255, 255]);
        let color = ((text[2] as u32) << 16) | ((text[1] as u32) << 8) | text[0] as u32;
        assert!(contrast(color, paper.paper) >= 4.5);
    }
    #[test]
    fn optional_ink_adjustment_preserves_color_and_only_changes_low_contrast_ink() {
        for paper in [0x0a0a0a, 0xffffff, 0x808080, 0xfff7e6] {
            let palette = CanvasTheme {
                paper,
                grid: 0,
                dots: 0,
                foreground: 0,
                adapt_ink: true,
            };
            for color in [
                0x2b3934, 0x340000, 0x003400, 0x000034, 0x55917e, 0xc6605c, 0x3265a8,
            ] {
                let adjusted = palette.ink(color);
                assert!(
                    contrast(adjusted, paper) >= palette.ink_contrast(),
                    "{color:06x} on {paper:06x}"
                );
                if contrast(color, paper) >= palette.ink_contrast() {
                    assert_eq!(adjusted, color);
                }
                let before: Hsla = rgb(color).into();
                let after: Hsla = rgb(adjusted).into();
                let hue_delta = (before.h - after.h).abs();
                assert!(hue_delta.min(1. - hue_delta) < 0.015);
                assert!((before.s - after.s).abs() < 0.025);
                assert_eq!(
                    CanvasTheme {
                        adapt_ink: false,
                        ..palette
                    }
                    .ink(color),
                    color
                );
            }
        }
    }
}
