use folio_document::*;
use serde::{Deserialize, Serialize};
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum EraserMode {
    #[default]
    Stroke,
    Segment,
    Object,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PageTemplate {
    pub id: Id,
    pub name: String,
    pub asset: String,
    #[serde(default)]
    pub preview: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub templates: Vec<PageTemplate>,
    pub collapsed_folders: Vec<Id>,
    pub recent_documents: Vec<Id>,
    pub dark: bool,
    pub reduce_motion: bool,
    pub appearance: crate::appearance::Appearance,
    pub ui_scale: f32,
    pub default_pen: PenStyle,
    pub presets: Vec<PenStyle>,
    pub paper: Paper,
    pub default_page: Option<PageProperties>,
    pub pad_buttons: Vec<String>,
    #[serde(skip_serializing)]
    pub segment_eraser: bool,
    pub eraser_mode: EraserMode,
    pub eraser_radius: f32,
    pub recent_colors: Vec<Color>,
    pub scratch_erase: bool,
    pub hold_shapes: bool,
    pub encircle_select: bool,
    pub autosave: bool,
    pub ocr_download_allowed: bool,
    pub cursor_size: f32,
}

impl Settings {
    pub fn effective_eraser_mode(&self) -> EraserMode {
        if self.segment_eraser {
            EraserMode::Segment
        } else {
            self.eraser_mode
        }
    }
    /// Older or hand-edited preferences must not create nonfinite GPUI sizes.
    pub fn normalize(&mut self) {
        let mut seen = std::collections::HashSet::new();
        self.templates.retain(|template| {
            !template.name.trim().is_empty()
                && seen.insert(template.id)
                && folio_export::asset_path(std::path::Path::new("."), &template.asset).is_ok()
                && template.preview.as_ref().is_none_or(|name| {
                    folio_export::asset_path(std::path::Path::new("."), name).is_ok()
                })
        });
        let mut seen_recent = std::collections::HashSet::new();
        self.recent_documents.retain(|id| seen_recent.insert(*id));
        self.recent_documents.truncate(20);
        let defaults = Self::default();
        let bound = |value: f32, fallback: f32, min: f32, max: f32| {
            if value.is_finite() {
                value.clamp(min, max)
            } else {
                fallback
            }
        };
        self.ui_scale = bound(self.ui_scale, defaults.ui_scale, 0.8, 1.6);
        self.eraser_radius = bound(self.eraser_radius, defaults.eraser_radius, 2., 80.);
        if self.segment_eraser {
            self.eraser_mode = EraserMode::Segment;
            self.segment_eraser = false;
        }
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
            templates: vec![],
            collapsed_folders: vec![],
            recent_documents: vec![],
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
            default_page: None,
            pad_buttons: vec![
                "undo".into(),
                "redo".into(),
                "eraser".into(),
                "pen".into(),
                "previous-page".into(),
                "next-page".into(),
            ],
            segment_eraser: false,
            eraser_mode: EraserMode::Stroke,
            eraser_radius: 10.,
            recent_colors: vec![],
            scratch_erase: false,
            hold_shapes: true,
            encircle_select: false,
            autosave: true,
            ocr_download_allowed: false,
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
    fn legacy_segment_eraser_migrates_and_new_sizes_are_bounded() {
        let mut settings: Settings =
            serde_json::from_value(serde_json::json!({"segment_eraser":true,"eraser_radius":1000}))
                .unwrap();
        settings.normalize();
        assert_eq!(settings.effective_eraser_mode(), EraserMode::Segment);
        assert_eq!(settings.eraser_radius, 80.);
        assert!(
            serde_json::to_value(&settings)
                .unwrap()
                .get("segment_eraser")
                .is_none()
        );
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
