//! Shared semantic theme for native controls and display-only canvas colors.
use folio_app::{
    Settings,
    appearance::{ThemeColor, ThemeToken},
};
use gpui::{Rgba, rgba};
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
        self.readable(color, 3.)
    }
    fn readable(self, color: u32, minimum: f32) -> u32 {
        if !self.adapt_ink || contrast(color, self.paper) >= minimum {
            return color;
        }
        let target = self.foreground;
        let first = if luminance(self.paper) < 0.4 { 15 } else { 1 };
        for step in first..=20 {
            let mixed = mix(color, target, step as f32 / 20.);
            if contrast(mixed, self.paper) >= 4.5 {
                return mixed;
            }
        }
        target
    }
    pub fn preview_pixels(self, bgra: &[u8]) -> Vec<u8> {
        let mut output = bgra.to_vec();
        if !self.adapt_ink {
            return output;
        }
        let mut colors = std::collections::HashMap::new();
        for pixel in output.as_chunks_mut::<4>().0 {
            if pixel[3] == 0 {
                continue;
            }
            let color = ((pixel[2] as u32) << 16) | ((pixel[1] as u32) << 8) | pixel[0] as u32;
            let adjusted = *colors
                .entry(color)
                .or_insert_with(|| self.readable(color, 4.5));
            pixel[0] = adjusted as u8;
            pixel[1] = (adjusted >> 8) as u8;
            pixel[2] = (adjusted >> 16) as u8;
        }
        output
    }
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
    pub chrome_active: u32,
    pub primary_foreground: u32,
    pub sidebar_accent: u32,
    pub destructive: u32,
    pub ring: u32,
    pub border: Rgba,
    pub input: Rgba,
    pub radius: f32,
    pub canvas: CanvasTheme,
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
            canvas.grid = mix(canvas.paper, canvas.foreground, 0.12);
            canvas.dots = mix(canvas.paper, canvas.foreground, 0.22);
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
            curve: readable.ink(self.accent),
        }
    }
    pub fn new(settings: &Settings) -> Self {
        use ThemeToken::*;
        let resolve = |token| settings.appearance.color(settings.dark, token);
        let color = |token| resolve(token).rgb();
        let paper = if settings.appearance.canvas_follows_theme {
            color(Background)
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
        let grid = mix(paper, foreground, 0.12);
        let dots = mix(paper, foreground, 0.22);
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
            chrome_active: color(Secondary),
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
        }
    }
}
pub fn mix(a: u32, b: u32, amount: f32) -> u32 {
    let channel = |shift: u32| {
        let a = ((a >> shift) & 255) as f32;
        let b = ((b >> shift) & 255) as f32;
        (a + (b - a) * amount).round() as u32
    };
    (channel(16) << 16) | (channel(8) << 8) | channel(0)
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
        let theme = Theme::new(&Settings::default());
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
        let theme = Theme::new(&settings);
        assert_eq!(theme.canvas.paper, 0x0a0a0a);
        assert!(contrast(theme.canvas.ink(0x2b3934), theme.canvas.paper) >= 4.5);
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
        let settings = Settings {
            dark: true,
            ..Default::default()
        };
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
    }
    #[test]
    fn custom_mid_gray_paper_chooses_a_readable_foreground() {
        let mut settings = Settings::default();
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
}
