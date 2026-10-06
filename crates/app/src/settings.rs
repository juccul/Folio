use folio_document::*;
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub dark: bool,
    pub reduce_motion: bool,
    pub appearance: crate::appearance::Appearance,
    pub ui_scale: f32,
    pub default_pen: PenStyle,
    pub presets: Vec<PenStyle>,
    pub paper: Paper,
    pub pad_buttons: Vec<String>,
    pub segment_eraser: bool,
    pub recent_colors: Vec<Color>,
    pub scratch_erase: bool,
    pub hold_shapes: bool,
    pub encircle_select: bool,
    pub autosave: bool,
    pub cursor_size: f32,
}

impl Settings {
    /// Older or hand-edited preferences must not create nonfinite GPUI sizes.
    pub fn normalize(&mut self) {
        let defaults = Self::default();
        let bound = |value: f32, fallback: f32, min: f32, max: f32| {
            if value.is_finite() {
                value.clamp(min, max)
            } else {
                fallback
            }
        };
        self.ui_scale = bound(self.ui_scale, defaults.ui_scale, 0.8, 1.6);
        self.cursor_size = bound(self.cursor_size, defaults.cursor_size, 4., 64.);
        let normalize_pen = |pen: &mut PenStyle| {
            pen.width = bound(pen.width, defaults.default_pen.width, 0.2, 80.);
            pen.opacity = bound(pen.opacity, defaults.default_pen.opacity, 0., 1.);
            pen.stabilization = bound(
                pen.stabilization,
                defaults.default_pen.stabilization,
                0.,
                0.9,
            );
            pen.pressure_gamma = bound(
                pen.pressure_gamma,
                defaults.default_pen.pressure_gamma,
                0.2,
                3.,
            );
        };
        normalize_pen(&mut self.default_pen);
        for preset in &mut self.presets {
            normalize_pen(preset);
        }
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            dark: false,
            reduce_motion: false,
            appearance: Default::default(),
            ui_scale: 1.,
            default_pen: PenStyle::default(),
            presets: vec![
                PenStyle::default(),
                PenStyle {
                    tool: InkTool::Fountain,
                    width: 4.,
                    ..Default::default()
                },
                PenStyle {
                    tool: InkTool::Pencil,
                    width: 2.,
                    opacity: 0.8,
                    ..Default::default()
                },
                PenStyle {
                    tool: InkTool::Marker,
                    width: 7.,
                    ..Default::default()
                },
                PenStyle {
                    tool: InkTool::Highlighter,
                    width: 20.,
                    opacity: 0.3,
                    color: Color::from_rgb(0xecc75c),
                    ..Default::default()
                },
            ],
            paper: Paper::Ruled,
            pad_buttons: vec![
                "undo".into(),
                "redo".into(),
                "eraser".into(),
                "pen".into(),
                "previous-page".into(),
                "next-page".into(),
            ],
            segment_eraser: false,
            recent_colors: vec![],
            scratch_erase: false,
            hold_shapes: true,
            encircle_select: false,
            autosave: true,
            cursor_size: 12.,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn malformed_numeric_preferences_are_bounded_without_losing_choices() {
        let mut settings = Settings {
            ui_scale: f32::INFINITY,
            cursor_size: -10.,
            dark: true,
            autosave: false,
            ..Default::default()
        };
        settings.default_pen.width = f32::NAN;
        settings.default_pen.opacity = -1.;
        settings.presets[0].pressure_gamma = f32::INFINITY;
        settings.normalize();
        assert_eq!(settings.ui_scale, 1.);
        assert_eq!(settings.cursor_size, 4.);
        assert!(settings.dark && !settings.autosave);
        assert!(settings.default_pen.valid());
        assert!(settings.presets.iter().all(PenStyle::valid));
    }

    #[test]
    fn legacy_recognition_preferences_are_ignored_and_not_saved_again() {
        let settings: Settings = serde_json::from_value(serde_json::json!({
            "dark": true,
            "auto_recognition": true,
            "ocr_engine": "trocr",
            "trocr_model": "/old/model",
            "math_model": "/old/math",
            "python": "/old/python",
            "spellcheck": true,
            "recognition_language": "eng",
            "strike_delete": true,
            "insertion_gesture": true
        }))
        .unwrap();
        assert!(settings.dark);
        assert!(settings.hold_shapes);
        let saved = serde_json::to_value(settings).unwrap();
        for field in [
            "auto_recognition",
            "ocr_engine",
            "trocr_model",
            "math_model",
            "python",
            "spellcheck",
            "recognition_language",
            "strike_delete",
            "insertion_gesture",
        ] {
            assert!(
                saved.get(field).is_none(),
                "Retired setting {field} survived"
            );
        }
    }
}
