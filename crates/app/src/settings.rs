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
pub struct PenPreset {
    pub id: Id,
    pub name: String,
    pub style: PenStyle,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct WorkspacePreferences {
    pub list_view: bool,
    pub sort_by_name: bool,
    pub sort_reverse: bool,
    pub pages_open: bool,
    pub library_open: bool,
    pub open_tabs: Vec<Id>,
    pub active_document: Option<Id>,
    pub current_pages: Vec<(Id, Id)>,
}
impl Default for WorkspacePreferences {
    fn default() -> Self {
        Self {
            list_view: false,
            sort_by_name: false,
            sort_reverse: false,
            pages_open: false,
            library_open: true,
            open_tabs: vec![],
            active_document: None,
            current_pages: vec![],
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub templates: Vec<PageTemplate>,
    pub starter_document: Option<Id>,
    pub workspace: WorkspacePreferences,
    pub reopen_documents: bool,
    pub collapsed_folders: Vec<Id>,
    pub recent_documents: Vec<Id>,
    pub dark: bool,
    /// Missing in older profiles: preserve their explicit light/dark choice.
    #[serde(default)]
    pub follow_system_theme: bool,
    pub reduce_motion: bool,
    pub appearance: crate::appearance::Appearance,
    pub ui_scale: f32,
    pub default_pen: PenStyle,
    /// Tools whose unchosen ink color follows the paper. Older saved profiles
    /// omit this list and retain their exact colors, including neutral colors.
    #[serde(default)]
    pub theme_default_ink_tools: Vec<InkTool>,
    #[serde(default, skip_serializing)]
    pub presets: Vec<PenStyle>,
    pub pen_presets: Vec<PenPreset>,
    pub tool_styles: Vec<PenStyle>,
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
    /// Resolve system appearance without replacing an explicit user override.
    pub fn apply_system_theme(&mut self, dark: bool) -> bool {
        if !self.follow_system_theme || self.dark == dark {
            return false;
        }
        self.dark = dark;
        true
    }
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
        let mut tabs = std::collections::HashSet::new();
        self.workspace.open_tabs.retain(|id| tabs.insert(*id));
        self.workspace.open_tabs.truncate(128);
        let mut pages = std::collections::HashSet::new();
        self.workspace
            .current_pages
            .retain(|(id, _)| pages.insert(*id));
        self.workspace.current_pages.truncate(128);
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
        for preset in &mut self.pen_presets {
            normalize_pen(&mut preset.style);
        }
        for style in &mut self.tool_styles {
            normalize_pen(style);
        }
        for style in std::mem::take(&mut self.presets) {
            if !self.pen_presets.iter().any(|p| p.style == style) {
                let name = format!("{:?} {}", style.tool, self.pen_presets.len() + 1);
                self.pen_presets.push(PenPreset {
                    id: Id::new_v4(),
                    name,
                    style,
                });
            }
        }
        let mut cleaned: Vec<PenPreset> = vec![];
        for mut preset in std::mem::take(&mut self.pen_presets) {
            if cleaned.iter().any(|p| p.style == preset.style) {
                continue;
            }
            if cleaned.iter().any(|p| p.id == preset.id) {
                preset.id = Id::new_v4();
            }
            let name = preset
                .name
                .trim()
                .chars()
                .filter(|c| !c.is_control())
                .take(56)
                .collect::<String>();
            let name = if name.is_empty() { "Pen".into() } else { name };
            let mut candidate = name.clone();
            let mut suffix = 2;
            while cleaned
                .iter()
                .any(|p| p.name.to_lowercase() == candidate.to_lowercase())
            {
                candidate = format!("{name} {suffix}");
                suffix += 1;
            }
            preset.name = candidate;
            cleaned.push(preset);
            if cleaned.len() == 64 {
                break;
            }
        }
        self.pen_presets = cleaned;
        let mut profiles: Vec<PenStyle> = vec![];
        for style in std::mem::take(&mut self.tool_styles) {
            profiles.retain(|s| s.tool != style.tool);
            profiles.push(style);
        }
        self.tool_styles = profiles;
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            templates: vec![],
            starter_document: None,
            workspace: WorkspacePreferences::default(),
            reopen_documents: true,
            collapsed_folders: vec![],
            recent_documents: vec![],
            dark: false,
            follow_system_theme: true,
            reduce_motion: false,
            appearance: Default::default(),
            ui_scale: 1.,
            default_pen: PenStyle::default(),
            theme_default_ink_tools: vec![
                InkTool::Ballpoint,
                InkTool::Fountain,
                InkTool::Pencil,
                InkTool::Marker,
            ],
            pen_presets: vec![],
            tool_styles: vec![],
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
    #[test]
    fn system_theme_defaults_and_legacy_manual_choices_round_trip() {
        let fresh = Settings::default();
        assert!(fresh.follow_system_theme);
        for dark in [false, true] {
            let old: Settings = serde_json::from_value(serde_json::json!({"dark": dark})).unwrap();
            assert!(!old.follow_system_theme);
            assert_eq!(old.dark, dark);
            let reopened: Settings =
                serde_json::from_str(&serde_json::to_string(&old).unwrap()).unwrap();
            assert!(!reopened.follow_system_theme);
            assert_eq!(reopened.dark, dark);
        }
        let reopened: Settings =
            serde_json::from_str(&serde_json::to_string(&fresh).unwrap()).unwrap();
        assert!(reopened.follow_system_theme);
    }

    #[test]
    fn system_changes_preserve_palettes_and_respect_manual_override() {
        let mut settings = Settings::default();
        let palette = serde_json::to_value(&settings.appearance).unwrap();
        assert!(settings.apply_system_theme(true));
        assert!(settings.dark);
        assert!(!settings.apply_system_theme(true));
        assert!(settings.apply_system_theme(false));
        assert_eq!(serde_json::to_value(&settings.appearance).unwrap(), palette);
        settings.follow_system_theme = false;
        assert!(!settings.apply_system_theme(true));
        assert!(!settings.dark);
    }
    use super::*;

    #[test]
    fn legacy_presets_migrate_once_without_resurrecting_deleted_defaults() {
        let style = PenStyle::default();
        let mut settings: Settings =
            serde_json::from_value(serde_json::json!({"presets":[style,style]})).unwrap();
        settings.normalize();
        assert_eq!(settings.pen_presets.len(), 1);
        let id = settings.pen_presets[0].id;
        let mut reopened: Settings =
            serde_json::from_str(&serde_json::to_string(&settings).unwrap()).unwrap();
        reopened.normalize();
        assert_eq!(reopened.pen_presets.len(), 1);
        assert_eq!(reopened.pen_presets[0].id, id);
        reopened.pen_presets.clear();
        let mut reopened: Settings =
            serde_json::from_str(&serde_json::to_string(&reopened).unwrap()).unwrap();
        reopened.normalize();
        assert!(reopened.pen_presets.is_empty());
    }
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
        assert!(
            settings
                .pen_presets
                .iter()
                .all(|preset| preset.style.valid())
        );
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
