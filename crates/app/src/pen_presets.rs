use super::*;
impl Controller {
    pub fn set_ink_tool(&mut self, tool: InkTool) {
        if self.style.tool == tool {
            return;
        }
        let mut style = self
            .settings
            .tool_styles
            .iter()
            .find(|s| s.tool == tool)
            .cloned()
            .unwrap_or_else(|| settings::default_ink_style(tool));
        if self.settings.theme_default_ink_tools.contains(&tool) {
            style.color = self.theme_default_ink_color();
        }
        self.set_style(style);
        self.set_tool(Tool::Pen);
    }
    pub fn save_preset(&mut self) -> Id {
        let name = format!(
            "{:?} {}",
            self.style.tool,
            self.settings.pen_presets.len() + 1
        );
        self.save_named_preset(name).unwrap_or_else(|error| {
            self.status = error;
            Id::nil()
        })
    }
    pub fn save_named_preset(&mut self, name: String) -> Result<Id, String> {
        if let Some(preset) = self
            .settings
            .pen_presets
            .iter()
            .find(|p| p.style == self.style)
        {
            self.status = format!("Already saved as {}", preset.name);
            return Ok(preset.id);
        }
        let name = self.preset_name(&name, None)?;
        if self.settings.pen_presets.len() >= 64 {
            return Err("Remove an unused preset before saving another (64 maximum).".into());
        }
        let id = Id::new_v4();
        self.settings.pen_presets.push(crate::PenPreset {
            id,
            name: name.clone(),
            style: self.style.clone(),
        });
        self.store_settings();
        self.status = format!("Saved preset {name}");
        Ok(id)
    }
    fn preset_name(&self, name: &str, excluding: Option<Id>) -> Result<String, String> {
        let name = name.trim();
        if name.is_empty() || name.chars().count() > 64 || name.chars().any(char::is_control) {
            return Err("Use a preset name of 1–64 characters.".into());
        }
        if self
            .settings
            .pen_presets
            .iter()
            .any(|p| Some(p.id) != excluding && p.name.to_lowercase() == name.to_lowercase())
        {
            return Err("A preset already uses that name.".into());
        }
        Ok(name.into())
    }
    pub fn rename_preset(&mut self, id: Id, name: String) -> Result<(), String> {
        let name = self.preset_name(&name, Some(id))?;
        let preset = self
            .settings
            .pen_presets
            .iter_mut()
            .find(|p| p.id == id)
            .ok_or("Preset no longer exists")?;
        preset.name = name;
        self.store_settings();
        Ok(())
    }
    pub fn update_preset(&mut self, id: Id) -> Result<(), String> {
        if self
            .settings
            .pen_presets
            .iter()
            .any(|p| p.id != id && p.style == self.style)
        {
            return Err("These settings already belong to another preset. Use that preset or remove it first.".into());
        }
        let preset = self
            .settings
            .pen_presets
            .iter_mut()
            .find(|p| p.id == id)
            .ok_or("Preset no longer exists")?;
        preset.style = self.style.clone();
        let name = preset.name.clone();
        self.store_settings();
        self.status = format!("Updated preset {name}");
        Ok(())
    }
    pub fn delete_preset(&mut self, id: Id) {
        self.settings.pen_presets.retain(|p| p.id != id);
        self.store_settings();
        self.status = "Preset removed; your current pen settings are retained.".into();
    }
    pub fn apply_preset(&mut self, id: Id) {
        if let Some(style) = self
            .settings
            .pen_presets
            .iter()
            .find(|p| p.id == id)
            .map(|p| p.style.clone())
        {
            self.settings
                .theme_default_ink_tools
                .retain(|tool| *tool != style.tool);
            self.set_style(style);
            self.set_tool(Tool::Pen);
            self.store_settings();
        }
    }
}
