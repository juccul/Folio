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
