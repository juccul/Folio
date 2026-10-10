//! Serializable appearance preferences. Colors never mutate document objects.
use folio_document::Color;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ThemeToken {
    Background,
    Foreground,
    Card,
    Popover,
    Primary,
    PrimaryForeground,
    Secondary,
    MutedForeground,
    Border,
    Input,
    Ring,
    Sidebar,
    SidebarPrimary,
    Destructive,
}
impl ThemeToken {
    pub const ALL: [Self; 14] = [
        Self::Background,
        Self::Foreground,
        Self::Card,
        Self::Popover,
        Self::Primary,
        Self::PrimaryForeground,
        Self::Secondary,
        Self::MutedForeground,
        Self::Border,
        Self::Input,
        Self::Ring,
        Self::Sidebar,
        Self::SidebarPrimary,
        Self::Destructive,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::Background => "Background",
            Self::Foreground => "Text",
            Self::Card => "Cards & toolbars",
            Self::Popover => "Menus & dialogs",
            Self::Primary => "Primary",
            Self::PrimaryForeground => "Text on primary",
            Self::Secondary => "Hover & selection",
            Self::MutedForeground => "Muted text",
            Self::Border => "Borders",
            Self::Input => "Input borders",
            Self::Ring => "Focus ring",
            Self::Sidebar => "Sidebar",
            Self::SidebarPrimary => "Sidebar accent",
            Self::Destructive => "Destructive actions",
        }
    }
    pub fn key(self) -> &'static str {
        match self {
            Self::Background => "background",
            Self::Foreground => "foreground",
            Self::Card => "card",
            Self::Popover => "popover",
            Self::Primary => "primary",
            Self::PrimaryForeground => "primary-foreground",
            Self::Secondary => "secondary",
            Self::MutedForeground => "muted-foreground",
            Self::Border => "border",
            Self::Input => "input",
            Self::Ring => "ring",
            Self::Sidebar => "sidebar",
            Self::SidebarPrimary => "sidebar-primary",
            Self::Destructive => "destructive",
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThemeColor(pub u32);
impl ThemeColor {
    pub fn rgb(self) -> u32 {
        self.0 >> 8
    }
    pub fn alpha(self) -> f32 {
        (self.0 & 255) as f32 / 255.
    }
    pub fn opaque(rgb: u32) -> Self {
        Self((rgb << 8) | 255)
    }
    pub fn hex(self) -> String {
        if self.0 & 255 == 255 {
            format!("#{:06x}", self.rgb())
        } else {
            format!("#{:08x}", self.0)
        }
    }
    pub fn parse(text: &str) -> Result<Self, &'static str> {
        let text = text.trim().trim_start_matches('#');
        if !matches!(text.len(), 6 | 8) || !text.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err("Use #RRGGBB or #RRGGBBAA, such as #171717 or #ffffff1a.");
        }
        let value = u32::from_str_radix(text, 16).map_err(|_| "Invalid hex color.")?;
        Ok(if text.len() == 6 {
            Self::opaque(value)
        } else {
            Self(value)
        })
    }
}
/// OKLCH → linear sRGB → encoded sRGB, with channel gamut clipping.
/// Used only to create defaults; resolving a theme has no conversions per frame.
fn oklch(l: f64, c: f64, h: f64, alpha: f64) -> ThemeColor {
    let a = c * h.to_radians().cos();
    let b = c * h.to_radians().sin();
    let ll = (l + 0.3963377774 * a + 0.2158037573 * b).powi(3);
    let mm = (l - 0.1055613458 * a - 0.0638541728 * b).powi(3);
    let ss = (l - 0.0894841775 * a - 1.2914855480 * b).powi(3);
    let encode = |v: f64| {
        let v = v.clamp(0., 1.);
        let v = if v <= 0.0031308 {
            12.92 * v
        } else {
            1.055 * v.powf(1. / 2.4) - 0.055
        };
        (v * 255.).round() as u32
    };
    ThemeColor(
        (encode(4.0767416621 * ll - 3.3077115913 * mm + 0.2309699292 * ss) << 24)
            | (encode(-1.2684380046 * ll + 2.6097574011 * mm - 0.3413193965 * ss) << 16)
            | (encode(-0.0041960863 * ll - 0.7034186147 * mm + 1.7076147010 * ss) << 8)
            | (alpha.clamp(0., 1.) * 255.).round() as u32,
    )
}
fn defaults(dark: bool) -> BTreeMap<ThemeToken, ThemeColor> {
    use ThemeToken::*;
    let gray = |l| oklch(l, 0., 0., 1.);
    if dark {
        [
            (Background, gray(0.145)),
            (Foreground, gray(0.985)),
            (Card, ThemeColor::opaque(0x0d0d0d)),
            (Popover, ThemeColor::opaque(0x0d0d0d)),
            (Primary, gray(0.922)),
            (PrimaryForeground, ThemeColor::opaque(0x0d0d0d)),
            (Secondary, gray(0.269)),
            (MutedForeground, gray(0.708)),
            (Border, ThemeColor(0xffffff0d)),
            (Input, oklch(1., 0., 0., 0.15)),
            (Ring, gray(0.556)),
            (Sidebar, ThemeColor::opaque(0x0d0d0d)),
            (SidebarPrimary, oklch(0.488, 0.243, 264.376, 1.)),
            (Destructive, oklch(0.704, 0.191, 22.216, 1.)),
        ]
        .into()
    } else {
        [
            (Background, gray(1.)),
            (Foreground, gray(0.145)),
            (Card, gray(1.)),
            (Popover, gray(1.)),
            (Primary, gray(0.205)),
            (PrimaryForeground, gray(0.985)),
            (Secondary, gray(0.97)),
            (MutedForeground, gray(0.556)),
            (Border, gray(0.922)),
            (Input, gray(0.922)),
            (Ring, gray(0.708)),
            (Sidebar, gray(0.985)),
            (SidebarPrimary, gray(0.205)),
            (Destructive, oklch(0.577, 0.245, 27.325, 1.)),
        ]
        .into()
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Appearance {
    pub light: BTreeMap<ThemeToken, ThemeColor>,
    pub dark: BTreeMap<ThemeToken, ThemeColor>,
    pub radius: f32,
    pub canvas_follows_theme: bool,
    pub canvas_color: Option<Color>,
    pub adapt_ink: bool,
}
impl Default for Appearance {
    fn default() -> Self {
        Self {
            light: BTreeMap::new(),
            dark: BTreeMap::new(),
            radius: 10.,
            canvas_follows_theme: true,
            canvas_color: None,
            adapt_ink: true,
        }
    }
}
static DEFAULTS: std::sync::LazyLock<[BTreeMap<ThemeToken, ThemeColor>; 2]> =
    std::sync::LazyLock::new(|| [defaults(false), defaults(true)]);
impl Appearance {
    /// Resolve a single theme token without allocating an entire palette.
    pub fn color(&self, dark: bool, token: ThemeToken) -> ThemeColor {
        let overrides = if dark { &self.dark } else { &self.light };
        overrides
            .get(&token)
            .copied()
            .unwrap_or(DEFAULTS[usize::from(dark)][&token])
    }
    pub fn palette(&self, dark: bool) -> BTreeMap<ThemeToken, ThemeColor> {
        let mut palette = DEFAULTS[usize::from(dark)].clone();
        palette.extend(if dark { &self.dark } else { &self.light });
        palette
    }
    pub fn set_color(&mut self, dark: bool, token: ThemeToken, color: ThemeColor) {
        let overrides = if dark {
            &mut self.dark
        } else {
            &mut self.light
        };
        overrides.insert(token, color);
    }
    pub fn radius(&self) -> f32 {
        if self.radius.is_finite() {
            self.radius.clamp(0., 20.)
        } else {
            10.
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn individual_colors_match_full_palettes_and_mode_overrides() {
        let mut appearance = Appearance::default();
        appearance.set_color(
            true,
            ThemeToken::Border,
            ThemeColor::parse("#12345678").unwrap(),
        );
        appearance.set_color(
            false,
            ThemeToken::Foreground,
            ThemeColor::parse("#102030").unwrap(),
        );
        for dark in [false, true] {
            for (token, color) in appearance.palette(dark) {
                assert_eq!(appearance.color(dark, token), color);
            }
        }
    }
    #[test]
    fn defaults_match_requested_oklch_tokens_and_alpha() {
        let appearance = Appearance::default();
        let light = appearance.palette(false);
        let dark = appearance.palette(true);
        assert_eq!(light[&ThemeToken::Background].hex(), "#ffffff");
        assert_eq!(light[&ThemeToken::Foreground].hex(), "#0a0a0a");
        assert_eq!(light[&ThemeToken::Primary].hex(), "#171717");
        assert_eq!(light[&ThemeToken::Secondary].hex(), "#f5f5f5");
        assert_eq!(dark[&ThemeToken::Foreground].hex(), "#fafafa");
        for token in [
            ThemeToken::Card,
            ThemeToken::Popover,
            ThemeToken::PrimaryForeground,
            ThemeToken::Sidebar,
        ] {
            assert_eq!(dark[&token].hex(), "#0d0d0d");
        }
        assert_eq!(dark[&ThemeToken::Border].hex(), "#ffffff0d");
        assert_eq!(dark[&ThemeToken::Input].hex(), "#ffffff26");
    }
    #[test]
    fn customization_is_mode_specific_and_roundtrips() {
        let mut appearance = Appearance::default();
        let original = appearance.palette(false);
        appearance.set_color(
            true,
            ThemeToken::Card,
            ThemeColor::parse("#28232f").unwrap(),
        );
        appearance.radius = 14.;
        appearance.canvas_follows_theme = false;
        let json = serde_json::to_string(&appearance).unwrap();
        let restored: Appearance = serde_json::from_str(&json).unwrap();
        assert_eq!(appearance, restored);
        assert_eq!(restored.palette(false), original);
        assert_eq!(restored.palette(true)[&ThemeToken::Card].hex(), "#28232f");
        assert!(Appearance::default().canvas_follows_theme);
        assert!(ThemeColor::parse("#ffffff<script>").is_err());
        assert!(ThemeColor::parse("red").is_err());
    }
    #[test]
    fn old_settings_migrate_without_losing_preferences() {
        let settings: crate::Settings =
            serde_json::from_str(r#"{"dark":true,"scratch_erase":true,"ui_scale":1.2}"#).unwrap();
        assert!(
            settings.dark && settings.scratch_erase && settings.appearance.canvas_follows_theme
        );
        assert_eq!(settings.ui_scale, 1.2);
        assert_eq!(settings.appearance.radius(), 10.);
    }
    #[test]
    fn appearance_changes_persist_without_restyling_document_objects() {
        let root =
            std::env::temp_dir().join(format!("folio-appearance-{}", folio_document::Id::new_v4()));
        let mut app = crate::Controller::open(root.clone()).unwrap();
        app.add_text("Original text".into(), folio_document::Point::new(40., 40.));
        let original = app.page().objects.clone();
        let note = app.active;
        app.settings.dark = true;
        app.settings.appearance.canvas_follows_theme = false;
        app.settings.appearance.canvas_color = Some(Color::from_rgb(0xfdf5e6));
        app.settings.appearance.set_color(
            true,
            ThemeToken::Primary,
            ThemeColor::parse("#fafafa").unwrap(),
        );
        app.store_settings();
        app.flush().unwrap();
        drop(app);
        let reopened = crate::Controller::open(root.clone()).unwrap();
        assert_eq!(reopened.active, note);
        assert_eq!(reopened.page().objects, original);
        assert!(reopened.settings.dark);
        assert!(!reopened.settings.appearance.canvas_follows_theme);
        assert_eq!(
            reopened.settings.appearance.canvas_color.unwrap().rgb(),
            0xfdf5e6
        );
        drop(reopened);
        std::fs::remove_dir_all(root).unwrap();
    }
}
