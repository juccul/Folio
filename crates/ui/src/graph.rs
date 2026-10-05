//! Display-only colors for solver-generated SVGs, including graphs saved by
//! earlier versions. Recolor before rasterizing so antialiasing stays correct.
use gpui::RenderImage;
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Palette {
    pub paper: u32,
    pub grid: u32,
    pub axes: u32,
    pub labels: u32,
    pub curve: u32,
}
impl Palette {
    pub fn svg(self, source: &str) -> String {
        // These attributes are the semantic colors emitted by our graph
        // worker. Change only attribute values, in one pass: a custom theme
        // may itself contain one of the original colors.
        let mut output = String::with_capacity(source.len());
        let mut previous = "";
        for (index, part) in source.split('"').enumerate() {
            if index > 0 {
                output.push('"');
            }
            let color = if index % 2 == 1 && previous.ends_with("fill=") {
                match part {
                    "white" => Some(self.paper),
                    "#333" => Some(self.labels),
                    _ => None,
                }
            } else if index % 2 == 1 && previous.ends_with("stroke=") {
                match part {
                    "#e4e7eb" => Some(self.grid),
                    "#889099" => Some(self.axes),
                    "#2463ae" => Some(self.curve),
                    _ => None,
                }
            } else {
                None
            };
            if let Some(color) = color {
                output.push_str(&format!("#{color:06x}"));
            } else {
                output.push_str(part);
            }
            previous = part;
        }
        output
    }
}

pub(super) fn image(width: u32, height: u32, mut pixels: Vec<u8>) -> Arc<RenderImage> {
    // resvg emits premultiplied RGBA; GPUI expects straight BGRA.
    for pixel in pixels.as_chunks_mut::<4>().0 {
        let a = pixel[3] as u32;
        for channel in pixel.iter_mut().take(3) {
            *channel = (*channel as u32 * 255 + a / 2)
                .checked_div(a)
                .unwrap_or(0)
                .min(255) as u8;
        }
        pixel.swap(0, 2);
    }
    let buffer = image::RgbaImage::from_raw(width, height, pixels).unwrap();
    Arc::new(RenderImage::new(smallvec::smallvec![image::Frame::new(
        buffer
    )]))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn saved_graphs_recolor_before_rasterization_without_changing_geometry() {
        let source = r##"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="60"><rect width="100" height="60" fill="white"/><defs><clipPath id="plot"><rect x="10" y="10" width="80" height="40"/></clipPath></defs><path d="M10 20H90" stroke="#e4e7eb" fill="none"/><path d="M50 10V50" stroke="#889099"/><text x="5" y="58" font-size="8" fill="#333">-10</text><path d="M0 30H100" stroke="#2463ae" stroke-width="2" fill="none" clip-path="url(#plot)"/></svg>"##;
        let palette = Palette {
            paper: 0x182522,
            grid: 0x30423c,
            axes: 0x78948a,
            labels: 0xffffff,
            curve: 0x77bdab,
        };
        let svg = palette.svg(source);
        let pixmap = folio_export::raster_svg(&svg, 1.).unwrap();
        assert_eq!((pixmap.width(), pixmap.height()), (100, 60));
        let pixel = |x, y| pixmap.pixel(x, y).unwrap();
        let paper = pixel(5, 30);
        let curve = pixel(60, 30);
        assert_eq!(
            (paper.red(), paper.green(), paper.blue(), paper.alpha()),
            (0x18, 0x25, 0x22, 255)
        );
        assert_eq!(
            (curve.red(), curve.green(), curve.blue()),
            (0x77, 0xbd, 0xab)
        );
        assert!(svg.contains("clip-path=\"url(#plot)\""));
        assert!(svg.contains("fill=\"none\""));
        assert!(source.contains("fill=\"white\""));
    }
    #[test]
    fn custom_colors_do_not_get_replaced_twice() {
        let palette = Palette {
            paper: 0x333333,
            grid: 0x2463ae,
            axes: 0xe4e7eb,
            labels: 0xffffff,
            curve: 0x889099,
        };
        let svg = palette.svg(r##"<rect fill="white"/><path stroke="#e4e7eb"/><path stroke="#889099"/><path stroke="#2463ae"/><text fill="#333">#2463ae</text>"##);
        assert_eq!(
            svg,
            r##"<rect fill="#333333"/><path stroke="#2463ae"/><path stroke="#e4e7eb"/><path stroke="#889099"/><text fill="#ffffff">#2463ae</text>"##
        );
    }
}
