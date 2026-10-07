use super::*;
impl Controller {
    pub fn save_page_template(&mut self, name: String) -> Result<(), String> {
        self.finish();
        let name = name.trim().to_string();
        if name.is_empty() || name.chars().count() > 120 {
            return Err("Template names need 1–120 characters".into());
        }
        let mut document = Document::new(name.clone());
        document.pages = vec![self.page().clone()];
        self.workers.submit(Job::SaveTemplate {
            document,
            assets: self.assets.clone(),
            name,
        })?;
        self.busy += 1;
        Ok(())
    }
    pub fn add_template_page(&mut self, id: Id) -> Result<(), String> {
        self.finish();
        let template = self
            .settings
            .templates
            .iter()
            .find(|t| t.id == id)
            .ok_or("Template no longer exists")?;
        let path =
            folio_export::asset_path(&self.assets, &template.asset).map_err(|e| e.to_string())?;
        self.workers.submit(Job::TemplatePage {
            note: self.active,
            path,
            assets: self.assets.clone(),
        })?;
        self.busy += 1;
        *self.pending_imports.entry(self.active).or_default() += 1;
        Ok(())
    }
    pub fn rename_template(&mut self, id: Id, name: String) -> Result<(), String> {
        if name.trim().is_empty() || name.chars().count() > 120 {
            return Err("Template names need 1–120 characters".into());
        }
        let template = self
            .settings
            .templates
            .iter_mut()
            .find(|t| t.id == id)
            .ok_or("Template no longer exists")?;
        template.name = name.trim().into();
        self.store_settings();
        Ok(())
    }
    pub fn remove_template(&mut self, id: Id) {
        self.settings.templates.retain(|t| t.id != id);
        self.store_settings();
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pending_template_insert_pins_its_note_and_releases_pin_on_failure() {
        let root = std::env::temp_dir().join(format!("folio-template-pin-{}", Id::new_v4()));
        let mut app = Controller::open(root.clone()).unwrap();
        app.add_text("Template content".into(), Point::new(20., 20.));
        app.save_page_template("Reusable".into()).unwrap();
        let settle = |app: &mut Controller| {
            let start = Instant::now();
            while app.has_background_work() {
                app.tick();
                assert!(start.elapsed() < Duration::from_secs(15));
                std::thread::sleep(Duration::from_millis(5));
            }
        };
        settle(&mut app);
        let target = app.active;
        let template = app.settings.templates[0].clone();
        app.add_template_page(template.id).unwrap();
        assert!(app.pending_imports.contains_key(&target));
        for _ in 0..12 {
            app.create_note();
        }
        app.flush().unwrap();
        // Make the persistence receipts current without accepting worker results.
        while let Ok(receipt) = app.persistence.receipts.try_recv() {
            assert!(receipt.result.is_ok());
            app.saved = app.saved.max(receipt.sequence);
        }
        app.trim_caches();
        assert!(
            app.sessions.contains_key(&target),
            "Pending template target was evicted"
        );
        settle(&mut app);
        assert!(!app.pending_imports.contains_key(&target));
        let stored = Store::open_reader(&app.database)
            .unwrap()
            .load(target)
            .unwrap()
            .unwrap();
        assert_eq!(stored.pages.len(), 2);
        app.switch_note(target);
        settle(&mut app);
        std::fs::remove_file(app.assets.join(template.asset)).unwrap();
        app.add_template_page(template.id).unwrap();
        settle(&mut app);
        assert!(app.error.is_some());
        assert!(!app.pending_imports.contains_key(&target));
        drop(app);
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn templates_preserve_layout_are_independent_and_survive_backup_and_undo() {
        let root = std::env::temp_dir().join(format!("folio-templates-{}", Id::new_v4()));
        let mut app = Controller::open(root.clone()).unwrap();
        app.add_text("Lecture heading".into(), Point::new(30., 30.));
        app.bookmark_page("Lecture".into());
        app.page_size(700., 900., false);
        let original = app.page().clone();
        app.save_page_template("Lecture layout".into()).unwrap();
        let settle = |app: &mut Controller| {
            let start = Instant::now();
            while app.has_background_work() {
                app.tick();
                assert!(start.elapsed() < Duration::from_secs(15));
                std::thread::sleep(Duration::from_millis(5));
            }
            assert!(app.error.is_none(), "{:?}", app.error);
        };
        settle(&mut app);
        let id = app.settings.templates[0].id;
        app.add_template_page(id).unwrap();
        settle(&mut app);
        assert_eq!(app.page().text(), original.text());
        assert_eq!(app.page().properties, original.properties);
        assert_ne!(app.page().id, original.id);
        app.add_text("Only this page".into(), Point::new(30., 100.));
        assert_eq!(app.session().document.pages[0].text(), "Lecture heading");
        app.undo();
        app.undo();
        assert_eq!(app.session().document.pages.len(), 1);
        app.redo();
        assert_eq!(app.session().document.pages.len(), 2);
        app.rename_template(id, "Reusable lecture".into()).unwrap();
        app.flush().unwrap();
        let kept = app.settings.templates[0].asset.clone();
        folio_storage::recovery::quarantine_orphans(&root).unwrap();
        assert!(app.assets.join(kept).exists());
        let archive = root
            .parent()
            .unwrap()
            .join(format!("template-backup-{}.foliobackup", Id::new_v4()));
        portable::backup(&root, &archive).unwrap();
        let restored = root
            .parent()
            .unwrap()
            .join(format!("template-restored-{}", Id::new_v4()));
        portable::restore(&archive, &restored).unwrap();
        let mut recovered = Controller::open(restored.clone()).unwrap();
        assert_eq!(recovered.settings.templates[0].name, "Reusable lecture");
        recovered.add_template_page(id).unwrap();
        settle(&mut recovered);
        assert_eq!(recovered.page().text(), "Lecture heading");
        drop(recovered);
        drop(app);
        std::fs::remove_dir_all(root).unwrap();
        std::fs::remove_dir_all(restored).unwrap();
        std::fs::remove_file(archive).unwrap();
    }
}
