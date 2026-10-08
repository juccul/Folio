//! Embedded Tabler Outline SVGs and a matching Folio marker on a 24×24 grid.
//! Attribution and upstream mappings: third_party/licenses/tabler-icons/NOTICE.md.
use gpui::{prelude::*, rems, *};
use std::borrow::Cow;

/// Assets are compiled into the binary so icons also work offline and in recovery.
pub struct IconAssets;

// One registry keeps icon names, asset paths and embedded bytes in sync.
macro_rules! define_icons {
    ($($variant:ident => $file:literal,)*) => {
        #[derive(Clone, Copy)]
        pub enum Icon { $($variant,)* }

        impl Icon {
            fn path(self) -> &'static str {
                match self {
                    $(Self::$variant => concat!("icons/", $file),)*
                }
            }

            #[cfg(test)]
            const ALL: &'static [Self] = &[$(Self::$variant,)*];
        }

        impl AssetSource for IconAssets {
            fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
                Ok(match path {
                    $(concat!("icons/", $file) => Some(Cow::Borrowed(
                        include_bytes!(concat!("../assets/icons/", $file)).as_slice()
                    )),)*
                    _ => None,
                })
            }

            fn list(&self, path: &str) -> Result<Vec<SharedString>> {
                Ok([$(concat!("icons/", $file),)*]
                    .into_iter()
                    .filter(|asset| asset.starts_with(path))
                    .map(SharedString::from)
                    .collect())
            }
        }
    };
}

define_icons! {
    Library => "Library.svg",
    Book => "Book.svg",
    Folder => "Folder.svg",
    Star => "Star.svg",
    StarFilled => "StarFilled.svg",
    Clock => "Clock.svg",
    Trash => "Trash.svg",
    Search => "Search.svg",
    Settings => "Settings.svg",
    Plus => "Plus.svg",
    Back => "Back.svg",
    Forward => "Forward.svg",
    Down => "Down.svg",
    Close => "Close.svg",
    Minimize => "Minimize.svg",
    Maximize => "Maximize.svg",
    Restore => "Restore.svg",
    More => "More.svg",
    Sidebar => "Sidebar.svg",
    Pen => "Pen.svg",
    Pencil => "Pencil.svg",
    Marker => "Marker.svg",
    Highlighter => "Highlighter.svg",
    Eraser => "Eraser.svg",
    Lasso => "Lasso.svg",
    Rectangle => "Rectangle.svg",
    Shapes => "Shapes.svg",
    Text => "Text.svg",
    Image => "Image.svg",
    Hand => "Hand.svg",
    Undo => "Undo.svg",
    Redo => "Redo.svg",
    Export => "Export.svg",
    Import => "Import.svg",
    Check => "Check.svg",
    Grid => "Grid.svg",
    List => "List.svg",
    Sliders => "Sliders.svg",
    Help => "Help.svg",
}

pub fn icon(kind: Icon, color: u32) -> Svg {
    // GPUI caches an alpha mask at the actual device size, then applies the
    // current theme color. SVG retains rounded strokes at fractional scales.
    svg()
        .path(kind.path())
        .text_color(rgb(color))
        .size(rems(1.375))
        .flex_shrink_0()
}

#[cfg(test)]
mod tests {
    use super::{Icon, IconAssets};
    use gpui::AssetSource;

    #[test]
    fn every_embedded_icon_renders_at_small_and_scaled_sizes() {
        let listed = IconAssets.list("icons/").unwrap();
        assert_eq!(listed.len(), Icon::ALL.len());
        assert!(IconAssets.load("icons/missing.svg").unwrap().is_none());
        for &kind in Icon::ALL {
            let path = kind.path();
            assert!(listed.iter().any(|asset| asset.as_ref() == path));
            let source = IconAssets.load(path).unwrap().expect("embedded icon");
            for size in [16, 22, 28, 35, 44] {
                let source = std::str::from_utf8(&source).unwrap();
                let pixmap = folio_export::raster_svg(source, size as f32 / 24.)
                    .unwrap_or_else(|error| panic!("{path} at {size}px: {error}"));
                assert_eq!(pixmap.width(), size as u32, "{path}");
                assert_eq!(pixmap.height(), size as u32, "{path}");
                assert!(
                    pixmap
                        .pixels()
                        .iter()
                        .filter(|pixel| pixel.alpha() > 32)
                        .count()
                        > size as usize / 2,
                    "{path} is blank at {size}px"
                );
                if matches!(kind, Icon::More) {
                    // Sparse menu dots must each survive at small/fractional sizes.
                    for third in 0..3 {
                        let left = size * third / 3;
                        let right = size * (third + 1) / 3;
                        assert!(
                            (left..right).any(|x| (0..size).any(|y| {
                                pixmap.pixels()[(y * size + x) as usize].alpha() > 32
                            })),
                            "{path} lost menu dot {third} at {size}px"
                        );
                    }
                }
            }
        }
    }
}
