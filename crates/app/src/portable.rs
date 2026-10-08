//! Versioned, checksummed local archives. Restores never overwrite a library.
use super::*;
use flate2::{Compression, read::GzDecoder, write::GzEncoder};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::Read,
    path::{Component, Path},
};
const MAX_BYTES: u64 = 2 * 1024 * 1024 * 1024;
const MAX_FILES: usize = 16_384;
#[derive(Serialize, Deserialize)]
struct Manifest {
    version: u32,
    kind: String,
    files: BTreeMap<String, String>,
}
struct Stage(PathBuf);
impl Stage {
    fn new(parent: &Path) -> Result<Self, String> {
        let path = parent.join(format!(".folio-stage-{}", Id::new_v4()));
        fs::create_dir(&path).map_err(|e| e.to_string())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&path, fs::Permissions::from_mode(0o700))
                .map_err(|e| e.to_string())?;
        }
        Ok(Self(path))
    }
}
impl Drop for Stage {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && !Path::new(name).is_absolute()
        && Path::new(name)
            .components()
            .all(|c| matches!(c, Component::Normal(_)))
        && !name.contains('\\')
}
/// File pickers must never publish exports over the active database or assets.
/// Copies at the library root use their usual extension; other locations are external.
pub(super) fn protect_library_destination(
    root: &Path,
    destination: &Path,
    extension: &str,
) -> Result<(), String> {
    let root = fs::canonicalize(root).map_err(|e| e.to_string())?;
    let parent = destination
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let parent = fs::canonicalize(parent).map_err(|e| e.to_string())?;
    let name = destination
        .file_name()
        .ok_or("Choose an export file name")?;
    let resolved = parent.join(name);
    if resolved.starts_with(&root)
        && (parent != root || resolved.extension().and_then(|s| s.to_str()) != Some(extension))
    {
        return Err("Choose a file outside the library's database and asset directories, or use the normal export extension at the library root".into());
    }
    Ok(())
}
fn sync_directories(root: &Path) -> Result<(), String> {
    for entry in fs::read_dir(root).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        if entry.file_type().map_err(|e| e.to_string())?.is_dir() {
            sync_directories(&entry.path())?;
        }
    }
    folio_platform::sync_directory(root).map_err(|e| e.to_string())
}
fn digest(path: &Path) -> Result<String, String> {
    let mut input = File::open(path).map_err(|e| e.to_string())?;
    let mut hash = Sha256::new();
    let mut buffer = [0; 64 * 1024];
    loop {
        let n = input.read(&mut buffer).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        hash.update(&buffer[..n]);
    }
    Ok(format!("{:x}", hash.finalize()))
}
fn regular(path: &Path) -> Result<(), String> {
    if !fs::symlink_metadata(path)
        .map_err(|e| e.to_string())?
        .is_file()
    {
        return Err("Archives accept regular files only".into());
    }
    Ok(())
}
fn collect(
    root: &Path,
    directory: &Path,
    files: &mut BTreeMap<String, PathBuf>,
) -> Result<(), String> {
    if !directory.exists() {
        return Ok(());
    }
    for entry in fs::read_dir(directory).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        let kind = entry.file_type().map_err(|e| e.to_string())?;
        if kind.is_dir() {
            collect(root, &path, files)?;
        } else if kind.is_file() {
            let name = path
                .strip_prefix(root)
                .map_err(|e| e.to_string())?
                .to_str()
                .ok_or("Archive path must be UTF-8")?
                .replace('\\', "/");
            files.insert(name, path);
        } else {
            return Err("A library asset is a symlink or special file".into());
        }
        if files.len() > MAX_FILES {
            return Err("Archive has too many files".into());
        }
    }
    Ok(())
}
fn write_archive(
    kind: &str,
    files: BTreeMap<String, PathBuf>,
    destination: &Path,
) -> Result<(), String> {
    let parent = destination
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let stage = Stage::new(parent)?;
    let temporary = stage.0.join("archive");
    let mut manifest = Manifest {
        version: 1,
        kind: kind.into(),
        files: BTreeMap::new(),
    };
    let mut bytes = 0u64;
    if files.len() > MAX_FILES {
        return Err("Archive has too many files".into());
    }
    for (name, path) in &files {
        if !valid_name(name) || name == "manifest.json" {
            return Err("Invalid archive file name".into());
        }
        regular(path)?;
        let size = fs::metadata(path).map_err(|e| e.to_string())?.len();
        if name == "document.json" && size > 128 * 1024 * 1024 {
            return Err("Notebook JSON exceeds the 128 MiB limit".into());
        }
        bytes = bytes
            .checked_add(size)
            .ok_or("Archive exceeds size limit")?;
        if bytes > MAX_BYTES {
            return Err("Archive exceeds the 2 GiB uncompressed limit".into());
        }
        manifest.files.insert(name.clone(), digest(path)?);
    }
    let file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&temporary)
        .map_err(|e| e.to_string())?;
    let encoder = GzEncoder::new(file, Compression::fast());
    let mut builder = tar::Builder::new(encoder);
    let data = serde_json::to_vec(&manifest).map_err(|e| e.to_string())?;
    if bytes
        .checked_add(data.len() as u64)
        .is_none_or(|size| size > MAX_BYTES)
    {
        return Err("Archive including its manifest exceeds the 2 GiB limit".into());
    }
    let mut header = tar::Header::new_gnu();
    header.set_size(data.len() as u64);
    header.set_mode(0o600);
    header.set_cksum();
    builder
        .append_data(&mut header, "manifest.json", data.as_slice())
        .map_err(|e| e.to_string())?;
    for (name, path) in files {
        builder
            .append_path_with_name(path, name)
            .map_err(|e| e.to_string())?;
    }
    let file = builder
        .into_inner()
        .map_err(|e| e.to_string())?
        .finish()
        .map_err(|e| e.to_string())?;
    drop(file);
    folio_platform::publish_file(&temporary, destination).map_err(|e| e.to_string())?;
    Ok(())
}
fn read_archive(path: &Path, stage: &Path, kind: &str) -> Result<Manifest, String> {
    let mut archive =
        tar::Archive::new(GzDecoder::new(File::open(path).map_err(|e| e.to_string())?));
    let mut seen = HashSet::new();
    let mut total = 0u64;
    for entry in archive.entries().map_err(|e| e.to_string())? {
        let mut entry = entry.map_err(|e| e.to_string())?;
        let name = entry
            .path()
            .map_err(|e| e.to_string())?
            .to_str()
            .ok_or("Archive path must be UTF-8")?
            .to_string();
        if !valid_name(&name)
            || !entry.header().entry_type().is_file()
            || !seen.insert(name.clone())
            || seen.len() > MAX_FILES + 1
        {
            return Err("Invalid, duplicate or non-file archive entry".into());
        }
        let size = entry.header().size().map_err(|e| e.to_string())?;
        total = total
            .checked_add(size)
            .ok_or("Archive exceeds size limit")?;
        if total > MAX_BYTES
            || (name == "manifest.json" && size > 16 * 1024 * 1024)
            || (name == "document.json" && size > 128 * 1024 * 1024)
        {
            return Err("Archive exceeds size limit".into());
        }
        let destination = stage.join(&name);
        fs::create_dir_all(destination.parent().unwrap()).map_err(|e| e.to_string())?;
        let mut output = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(destination)
            .map_err(|e| e.to_string())?;
        let copied = std::io::copy(&mut entry, &mut output).map_err(|e| e.to_string())?;
        if copied != size {
            return Err("Truncated archive entry".into());
        }
        output.sync_all().map_err(|e| e.to_string())?;
    }
    // Tar stops at its end marker. Drain the decoder so gzip CRC/truncation
    // errors in the trailer cannot be hidden by a complete tar payload.
    let mut decoder = archive.into_inner();
    let mut trailing = 0usize;
    let mut buffer = [0; 4096];
    loop {
        let size = decoder.read(&mut buffer).map_err(|e| e.to_string())?;
        if size == 0 {
            break;
        }
        trailing += size;
        if trailing > 1024 * 1024 || buffer[..size].iter().any(|byte| *byte != 0) {
            return Err("Unexpected data after archive end marker".into());
        }
    }
    let manifest: Manifest =
        serde_json::from_slice(&fs::read(stage.join("manifest.json")).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    if manifest.version != 1 || manifest.kind != kind {
        return Err("Unsupported archive version or archive type".into());
    }
    if seen.len() != manifest.files.len() + 1 {
        return Err("Archive manifest does not match its files".into());
    }
    for (name, hash) in &manifest.files {
        if !valid_name(name)
            || name == "manifest.json"
            || !seen.contains(name)
            || digest(&stage.join(name))? != *hash
        {
            return Err("Archive checksum verification failed".into());
        }
    }
    Ok(manifest)
}
fn references(document: &Document) -> HashSet<String> {
    let mut assets = HashSet::new();
    for page in &document.pages {
        if let Some(pdf) = &page.properties.pdf {
            assets.insert(pdf.asset.clone());
            if let Some(preview) = &pdf.preview_asset {
                assets.insert(preview.clone());
            }
        }
        for object in page.objects.values() {
            if let Object::Image(image) = object.as_ref() {
                assets.insert(image.asset.clone());
            }
        }
    }
    assets
}
pub fn export_notebook(
    document: &Document,
    assets: &Path,
    destination: &Path,
) -> Result<(), String> {
    document.validate()?;
    let stage = Stage::new(&std::env::temp_dir())?;
    let mut document = document.clone();
    for page in &mut document.pages {
        if let Some(pdf) = &mut page.properties.pdf
            && pdf
                .preview_asset
                .as_ref()
                .is_some_and(|name| !assets.join(name).is_file())
        {
            pdf.preview_asset = None;
        }
    }
    let data = stage.0.join("document.json");
    fs::write(
        &data,
        serde_json::to_vec(&document).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    let mut files = BTreeMap::from([("document.json".into(), data)]);
    for name in references(&document) {
        let path = folio_export::asset_path(assets, &name).map_err(|e| e.to_string())?;
        files.insert(format!("assets/{name}"), path);
        if let Some(stem) = name.strip_suffix("-unlocked.pdf") {
            let original = format!("{stem}.pdf");
            if assets.join(&original).is_file() {
                files.insert(format!("assets/{original}"), assets.join(original));
            }
        }
    }
    write_archive("notebook", files, destination)
}
pub fn import_notebook(path: &Path, assets: &Path) -> Result<Document, String> {
    let stage = Stage::new(&std::env::temp_dir())?;
    let manifest = read_archive(path, &stage.0, "notebook")?;
    if manifest
        .files
        .keys()
        .any(|name| name != "document.json" && !name.starts_with("assets/"))
    {
        return Err("Unexpected notebook archive file".into());
    }
    let mut document: Document = serde_json::from_slice(
        &fs::read(stage.0.join("document.json")).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    document.validate()?;
    for name in references(&document) {
        if !manifest.files.contains_key(&format!("assets/{name}")) {
            return Err("Notebook archive is missing an asset".into());
        }
    }
    let namespace = Id::new_v4();
    let mut names = HashMap::new();
    for name in manifest
        .files
        .keys()
        .filter_map(|name| name.strip_prefix("assets/"))
    {
        names.insert(
            name.to_owned(),
            format!("{namespace}-{}", name.replace('/', "-")),
        );
    }
    let old_cover = document.metadata.cover_page;
    document.metadata.id = Id::new_v4();
    document.metadata.notebook = None;
    document.metadata.trashed = false;
    document.metadata.favorite = false;
    document.metadata.created_at = now_ms();
    document.metadata.updated_at = now_ms();
    document.metadata.cover_page = None;
    for page in &mut document.pages {
        let old = page.id;
        *page = page.duplicate();
        if Some(old) == old_cover {
            document.metadata.cover_page = Some(page.id);
        }
        if let Some(pdf) = &mut page.properties.pdf {
            pdf.asset = names[&pdf.asset].clone();
            pdf.preview_asset = pdf.preview_asset.as_ref().map(|name| names[name].clone());
        }
        for object in page.objects.values_mut() {
            if let Object::Image(image) = Arc::make_mut(object) {
                image.asset = names[&image.asset].clone();
            }
        }
    }
    document.validate()?;
    fs::create_dir_all(assets).map_err(|e| e.to_string())?;
    for (old, new) in names {
        let from = stage.0.join("assets").join(old);
        let to = assets.join(new);
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(to)
            .map_err(|e| e.to_string())?;
        std::io::copy(&mut File::open(from).map_err(|e| e.to_string())?, &mut file)
            .map_err(|e| e.to_string())?;
        file.sync_all().map_err(|e| e.to_string())?;
    }
    folio_platform::sync_directory(assets).map_err(|e| e.to_string())?;
    Ok(document)
}
pub fn backup(root: &Path, destination: &Path) -> Result<(), String> {
    protect_library_destination(root, destination, "foliobackup")?;
    let stage = Stage::new(&std::env::temp_dir())?;
    let snapshot = stage.0.join("notes.sqlite3");
    Store::open_reader(root.join("notes.sqlite3"))
        .map_err(|e| e.to_string())?
        .backup_to(&snapshot)
        .map_err(|e| e.to_string())?;
    let mut files = BTreeMap::from([("notes.sqlite3".into(), snapshot)]);
    collect(root, &root.join("assets"), &mut files)?;
    collect(root, &root.join("orphaned-assets"), &mut files)?;
    write_archive("library", files, destination)
}
pub fn restore(path: &Path, destination: &Path) -> Result<(), String> {
    if destination.exists() {
        return Err(
            "Restore needs a new library directory; existing libraries are never overwritten"
                .into(),
        );
    }
    let parent = destination.parent().ok_or("Choose a restore destination")?;
    let stage = Stage::new(parent)?;
    let manifest = read_archive(path, &stage.0, "library")?;
    if manifest.files.keys().any(|name| {
        name != "notes.sqlite3"
            && !name.starts_with("assets/")
            && !name.starts_with("orphaned-assets/")
    }) {
        return Err("Unexpected library archive file".into());
    }
    if !manifest.files.contains_key("notes.sqlite3") {
        return Err("Archive has no database".into());
    }
    {
        let store = Store::open(stage.0.join("notes.sqlite3")).map_err(|e| e.to_string())?;
        let integrity = store
            .connection
            .query_row("PRAGMA integrity_check", [], |r| r.get::<_, String>(0))
            .map_err(|e| e.to_string())?;
        if integrity != "ok" {
            return Err("Backup database is damaged".into());
        }
        for note in store.list_notes().map_err(|e| e.to_string())? {
            let document = store
                .load(note.id)
                .map_err(|e| e.to_string())?
                .ok_or("Backup notebook is missing")?;
            for asset in references(&document) {
                folio_export::asset_path(&stage.0.join("assets"), &asset)
                    .map_err(|e| e.to_string())?;
                // PDF previews are regenerable; original PDFs/images are not.
                let preview = document.pages.iter().any(|p| {
                    p.properties
                        .pdf
                        .as_ref()
                        .and_then(|pdf| pdf.preview_asset.as_ref())
                        == Some(&asset)
                });
                if !preview && !stage.0.join("assets").join(&asset).is_file() {
                    return Err("Backup is missing a document asset".into());
                }
            }
        }
    }
    fs::remove_file(stage.0.join("manifest.json")).map_err(|e| e.to_string())?;
    sync_directories(&stage.0)?;
    fs::rename(&stage.0, destination).map_err(|e| e.to_string())?;
    folio_platform::sync_directory(parent).map_err(|e| e.to_string())?;
    Ok(())
}
impl Controller {
    pub fn validate_export_destination(&self, path: &Path, extension: &str) -> Result<(), String> {
        protect_library_destination(&self.data_dir, path, extension)
    }
    pub fn use_library_by_default(&mut self) -> Result<(), String> {
        folio_platform::use_library_by_default(&self.data_dir).map_err(|e| e.to_string())?;
        self.status = format!("Startup library: {}", self.data_dir.display());
        Ok(())
    }
    pub fn backup_library(&mut self, path: PathBuf) -> Result<(), String> {
        self.validate_export_destination(&path, "foliobackup")?;
        self.flush()?;
        self.submit(Job::Backup {
            root: self.data_dir.clone(),
            path,
        });
        Ok(())
    }
    pub fn restore_library(&mut self, path: PathBuf, parent: PathBuf) -> Result<(), String> {
        self.flush()?;
        let destination = parent.join(format!("folio-restored-{}", Id::new_v4()));
        self.submit(Job::Restore { path, destination });
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    #[test]
    fn archive_destinations_preserve_the_live_database_and_assets() {
        let root = Stage::new(&std::env::temp_dir()).unwrap();
        let mut app = Controller::open(root.0.join("library")).unwrap();
        app.add_text("Keep this notebook".into(), Point::new(20., 20.));
        app.flush().unwrap();
        let before = fs::read(&app.database).unwrap();
        let image = app.assets.join("picture.png");
        fs::write(&image, b"original asset").unwrap();
        for path in [
            app.database.clone(),
            image.clone(),
            app.data_dir.join("session.lock"),
        ] {
            assert!(backup(&app.data_dir, &path).is_err());
            app.export(path, ExportKind::Notebook);
            assert!(app.error.take().is_some());
        }
        #[cfg(unix)]
        {
            let alias = root.0.join("alias");
            std::os::unix::fs::symlink(&app.data_dir, &alias).unwrap();
            assert!(backup(&app.data_dir, &alias.join("notes.sqlite3")).is_err());
        }
        assert_eq!(fs::read(&app.database).unwrap(), before);
        assert_eq!(fs::read(image).unwrap(), b"original asset");
        assert!(
            protect_library_destination(
                &app.data_dir,
                &app.data_dir.join("saved.foliobackup"),
                "foliobackup"
            )
            .is_ok()
        );
    }
    #[test]
    fn exporter_rejects_unimportable_json_and_reader_checks_gzip_trailer() {
        let root = Stage::new(&std::env::temp_dir()).unwrap();
        let payload = root.0.join("payload");
        File::create(&payload)
            .unwrap()
            .set_len(128 * 1024 * 1024 + 1)
            .unwrap();
        let archive = root.0.join("archive");
        assert!(
            write_archive(
                "notebook",
                BTreeMap::from([("document.json".into(), payload.clone())]),
                &archive
            )
            .is_err()
        );
        assert!(!archive.exists());
        fs::write(&payload, b"valid").unwrap();
        write_archive(
            "notebook",
            BTreeMap::from([("document.json".into(), payload)]),
            &archive,
        )
        .unwrap();
        let extracted = Stage::new(&root.0).unwrap();
        read_archive(&archive, &extracted.0, "notebook").unwrap();
        let mut bytes = fs::read(&archive).unwrap();
        let crc = bytes.len() - 8;
        bytes[crc] ^= 0x80;
        fs::write(&archive, bytes).unwrap();
        let extracted = Stage::new(&root.0).unwrap();
        assert!(read_archive(&archive, &extracted.0, "notebook").is_err());
    }
    #[test]
    fn notebook_roundtrip_keeps_editable_objects_and_remaps_assets_and_ids() {
        let root = Stage::new(&std::env::temp_dir()).unwrap();
        let assets = root.0.join("assets");
        fs::create_dir(&assets).unwrap();
        fs::write(assets.join("picture.png"), b"image bytes").unwrap();
        let mut document = Document::new("Notes");
        let object = Object::Image(ImageObject {
            id: Id::new_v4(),
            asset: "picture.png".into(),
            rect: Rect::new(10., 20., 30., 40.),
            transform: Transform::default(),
            crop: Some(Rect::new(0.1, 0.1, 0.8, 0.8)),
        });
        let id = object.id();
        document.pages[0].order.push(id);
        document.pages[0].objects.insert(id, Arc::new(object));
        document.metadata.cover_page = Some(document.pages[0].id);
        let bundle = root.0.join("note.folio");
        export_notebook(&document, &assets, &bundle).unwrap();
        let imported = import_notebook(&bundle, &assets).unwrap();
        assert_ne!(document.metadata.id, imported.metadata.id);
        assert_ne!(document.pages[0].id, imported.pages[0].id);
        assert_eq!(imported.metadata.cover_page, Some(imported.pages[0].id));
        let Object::Image(image) = imported.pages[0].objects.values().next().unwrap().as_ref()
        else {
            panic!()
        };
        assert_ne!(image.id, id);
        assert_ne!(image.asset, "picture.png");
        assert_eq!(fs::read(assets.join(&image.asset)).unwrap(), b"image bytes");
        assert_eq!(image.crop, Some(Rect::new(0.1, 0.1, 0.8, 0.8)));
    }
    #[test]
    fn backup_uses_wal_snapshot_and_restores_history_settings_and_assets_separately() {
        let root = Stage::new(&std::env::temp_dir()).unwrap();
        let data = root.0.join("library");
        let mut app = Controller::open(data.clone()).unwrap();
        app.add_text("Before backup".into(), Point::new(20., 20.));
        app.settings.dark = true;
        app.store_settings();
        app.flush().unwrap();
        fs::write(app.assets.join("kept.bin"), b"asset").unwrap();
        let id = app.active;
        let archive = root.0.join("backup.foliobackup");
        backup(&data, &archive).unwrap();
        app.add_text("Later edit".into(), Point::new(20., 70.));
        app.flush().unwrap();
        let restored = root.0.join("restored");
        restore(&archive, &restored).unwrap();
        let mut recovered = Controller::open(restored.clone()).unwrap();
        assert!(recovered.settings.dark);
        assert_eq!(recovered.active, id);
        assert_eq!(recovered.page().text(), "Before backup");
        recovered.undo();
        assert!(recovered.page().text().is_empty());
        assert_eq!(
            fs::read(recovered.assets.join("kept.bin")).unwrap(),
            b"asset"
        );
        assert!(app.page().text().contains("Later edit"));
        assert!(restore(&archive, &data).is_err());
    }
    #[test]
    fn corrupted_wrong_type_and_nonregular_archives_are_rejected_without_publishing() {
        let root = Stage::new(&std::env::temp_dir()).unwrap();
        let file = root.0.join("payload");
        fs::write(&file, b"valid").unwrap();
        let archive = root.0.join("archive");
        write_archive(
            "notebook",
            BTreeMap::from([("document.json".into(), file)]),
            &archive,
        )
        .unwrap();
        let target = root.0.join("restore");
        assert!(restore(&archive, &target).is_err());
        assert!(!target.exists());
        assert!(!valid_name("../escape"));
        assert!(!valid_name("/absolute"));
        let bytes = fs::read(&archive).unwrap();
        fs::write(&archive, &bytes[..bytes.len() / 2]).unwrap();
        assert!(restore(&archive, &target).is_err());
        assert!(!target.exists());
    }
    #[test]
    fn archive_reader_rejects_symlinks_traversal_and_oversized_headers() {
        let root = Stage::new(&std::env::temp_dir()).unwrap();
        for (name, kind, size) in [
            ("assets/link", tar::EntryType::Symlink, 0),
            ("../outside", tar::EntryType::Regular, 0),
            ("large", tar::EntryType::Regular, MAX_BYTES + 1),
        ] {
            let path = root.0.join("bad.tar.gz");
            let mut header = tar::Header::new_gnu();
            header.set_entry_type(kind);
            header.set_size(size);
            header.set_mode(0o600);
            header.as_mut_bytes()[..name.len()].copy_from_slice(name.as_bytes());
            header.set_cksum();
            let mut encoder = GzEncoder::new(File::create(&path).unwrap(), Compression::fast());
            encoder.write_all(header.as_bytes()).unwrap();
            encoder.finish().unwrap();
            let extracted = Stage::new(&root.0).unwrap();
            assert!(read_archive(&path, &extracted.0, "library").is_err());
            assert!(!root.0.join("outside").exists());
        }
    }
}
