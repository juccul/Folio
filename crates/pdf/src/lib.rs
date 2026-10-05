//! Original PDFs with lazy, bounded Poppler previews. Metadata and passwords are
//! handled in Rust; only requested pages are rendered on document workers.
use folio_document::*;
use std::{path::Path, process::Command, time::Duration};
#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("PDF import: {0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Invalid(String),
    #[error("This PDF needs a password")]
    PasswordRequired,
}
fn inherited(document: &lopdf::Document, id: lopdf::ObjectId, key: &[u8]) -> Option<lopdf::Object> {
    let mut id = id;
    for _ in 0..64 {
        let dict = document.get_dictionary(id).ok()?;
        if let Ok(value) = dict.get(key) {
            return Some(value.clone());
        }
        id = dict.get(b"Parent").ok()?.as_reference().ok()?;
    }
    None
}
pub fn import(path: &Path, assets: &Path) -> Result<Vec<Page>, Error> {
    import_with_password(path, assets, None)
}
pub fn import_with_password(
    path: &Path,
    assets: &Path,
    password: Option<&str>,
) -> Result<Vec<Page>, Error> {
    let metadata =
        lopdf::Document::load_metadata(path).map_err(|e| Error::Invalid(e.to_string()))?;
    let encrypted = metadata.encrypted;
    // The password-aware loader decrypts raw objects and object streams while
    // parsing; loading first and decrypting later can omit encrypted page trees.
    let mut document = if encrypted {
        lopdf::Document::load_with_password(path, password.unwrap_or(""))
            .map_err(|_| Error::PasswordRequired)?
    } else {
        lopdf::Document::load(path).map_err(|e| Error::Invalid(e.to_string()))?
    };
    let id = Id::new_v4();
    let original = folio_platform::copy_asset(path, assets, &id.to_string(), "pdf")?;
    let name = if encrypted {
        let unlocked = assets.join(format!("{id}-unlocked.pdf"));
        let temporary = assets.join(format!("{id}-unlocking.tmp"));
        document
            .save(&temporary)
            .map_err(|e| Error::Invalid(e.to_string()))?;
        std::fs::File::open(&temporary)?.sync_all()?;
        std::fs::rename(temporary, &unlocked)?;
        std::fs::File::open(assets)?.sync_all()?;
        unlocked.file_name().unwrap().to_string_lossy().into_owned()
    } else {
        original.file_name().unwrap().to_string_lossy().into_owned()
    };
    let source_pages = document.get_pages();
    if source_pages.is_empty() {
        return Err(Error::Invalid("PDF has no pages".into()));
    }
    let mut pages = Vec::new();
    for (number, page_id) in source_pages {
        let mut value = inherited(&document, page_id, b"CropBox")
            .or_else(|| inherited(&document, page_id, b"MediaBox"))
            .ok_or_else(|| Error::Invalid("PDF page has no size".into()))?;
        if let lopdf::Object::Reference(id) = value {
            value = document
                .get_object(id)
                .map_err(|e| Error::Invalid(e.to_string()))?
                .clone();
        }
        let values = value
            .as_array()
            .map_err(|e| Error::Invalid(e.to_string()))?;
        let n = |i: usize| values.get(i).and_then(|v| v.as_float().ok()).unwrap_or(0.);
        let unit = inherited(&document, page_id, b"UserUnit")
            .and_then(|v| v.as_float().ok())
            .filter(|v| v.is_finite() && *v > 0.)
            .unwrap_or(1.);
        let (mut width, mut height) = ((n(2) - n(0)) * unit, (n(3) - n(1)) * unit);
        let rotation = inherited(&document, page_id, b"Rotate")
            .and_then(|v| v.as_i64().ok())
            .unwrap_or(0)
            .rem_euclid(360);
        if rotation == 90 || rotation == 270 {
            std::mem::swap(&mut width, &mut height);
        }
        if !width.is_finite() || !height.is_finite() || width <= 0. || height <= 0. {
            return Err(Error::Invalid("Invalid PDF page size".into()));
        }
        let mut page = Page::new();
        page.properties = PageProperties {
            width: width * 96. / 72.,
            height: height * 96. / 72.,
            paper: Paper::Blank,
            infinite: false,
            pdf: Some(PdfBackground {
                asset: name.clone(),
                page: number,
                preview_asset: Some(format!("{id}-page-{number}.png")),
            }),
        };
        pages.push(page);
    }
    Ok(pages)
}
pub fn render_preview(background: &PdfBackground, assets: &Path) -> Result<(), Error> {
    let preview = background
        .preview_asset
        .as_deref()
        .ok_or_else(|| Error::Invalid("Missing preview name".into()))?;
    let source = checked_asset(assets, &background.asset)?;
    let target = checked_asset(assets, preview)?;
    if target.is_file() {
        return Ok(());
    }
    let stem = assets.join(format!(".preview-{}", Id::new_v4()));
    let generated = stem.with_extension("png");
    let result = (|| {
        folio_platform::run(
            Command::new("pdftoppm")
                .args([
                    "-f",
                    &background.page.to_string(),
                    "-l",
                    &background.page.to_string(),
                    "-singlefile",
                    "-cropbox",
                    "-scale-to",
                    "2400",
                    "-png",
                ])
                .arg(source)
                .arg(&stem),
            None,
            Duration::from_secs(30),
        )?;
        std::fs::File::open(&generated)?.sync_all()?;
        std::fs::rename(&generated, &target)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(generated);
    }
    result
}
fn checked_asset(assets: &Path, name: &str) -> Result<std::path::PathBuf, Error> {
    if Path::new(name)
        .components()
        .any(|p| !matches!(p, std::path::Component::Normal(_)))
    {
        return Err(Error::Invalid("Invalid PDF asset name".into()));
    }
    Ok(assets.join(name))
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::{Dictionary, Document, Object, Stream, dictionary};
    fn source(count: usize) -> Document {
        let mut doc = Document::with_version("1.7");
        let parent = doc.new_object_id();
        let mut kids = Vec::new();
        for _ in 0..count {
            let content = doc.add_object(Stream::new(
                Dictionary::new(),
                b"0 0 1 rg 20 20 80 80 re f".to_vec(),
            ));
            let page =
                doc.add_object(dictionary! {"Type"=>"Page","Parent"=>parent,"Contents"=>content});
            kids.push(Object::Reference(page));
        }
        doc.set_object(parent,dictionary! {"Type"=>"Pages","Kids"=>kids,"Count"=>count as i64,"MediaBox"=>vec![0.into(),0.into(),300.into(),400.into()],"Resources"=>Dictionary::new()});
        let catalog = doc.add_object(dictionary! {"Type"=>"Catalog","Pages"=>parent});
        doc.trailer.set("Root", catalog);
        doc
    }
    #[test]
    fn crop_and_rotation_have_matching_preview_dimensions_and_page_order() {
        let root = std::env::temp_dir().join(format!("folio-crop-pdf-{}", Id::new_v4()));
        std::fs::create_dir(&root).unwrap();
        let mut doc = source(3);
        let ids = doc.get_pages();
        let p = doc.get_dictionary_mut(ids[&2]).unwrap();
        p.set(
            "CropBox",
            vec![50.into(), 50.into(), 250.into(), 150.into()],
        );
        p.set("Rotate", 90);
        doc.save(root.join("source.pdf")).unwrap();
        let pages = import(&root.join("source.pdf"), &root.join("assets")).unwrap();
        assert_eq!(
            pages
                .iter()
                .map(|p| p.properties.pdf.as_ref().unwrap().page)
                .collect::<Vec<_>>(),
            vec![1, 2, 3]
        );
        let props = &pages[1].properties;
        assert!((props.width - 100. * 96. / 72.).abs() < 0.01);
        assert!((props.height - 200. * 96. / 72.).abs() < 0.01);
        let bg = props.pdf.as_ref().unwrap();
        render_preview(bg, &root.join("assets")).unwrap();
        let png =
            std::fs::read(root.join("assets").join(bg.preview_asset.as_ref().unwrap())).unwrap();
        let width = u32::from_be_bytes(png[16..20].try_into().unwrap());
        let height = u32::from_be_bytes(png[20..24].try_into().unwrap());
        assert!((width as f32 / height as f32 - props.width / props.height).abs() < 0.01);
        assert!(
            pages[0]
                .properties
                .pdf
                .as_ref()
                .unwrap()
                .preview_asset
                .as_ref()
                .is_some_and(|name| !root.join("assets").join(name).exists())
        );
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn large_pdf_import_is_lazy_and_has_no_500_page_limit() {
        let root = std::env::temp_dir().join(format!("folio-lazy-pdf-{}", Id::new_v4()));
        std::fs::create_dir(&root).unwrap();
        source(501).save(root.join("source.pdf")).unwrap();
        let pages = import(&root.join("source.pdf"), &root.join("assets")).unwrap();
        assert_eq!(pages.len(), 501);
        assert_eq!(std::fs::read_dir(root.join("assets")).unwrap().count(), 1);
        assert!(pages.iter().all(|p| {
            p.properties
                .pdf
                .as_ref()
                .unwrap()
                .preview_asset
                .as_ref()
                .is_some_and(|asset| !root.join("assets").join(asset).exists())
        }));
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn encrypted_original_is_retained_and_password_is_not_persisted() {
        let root = std::env::temp_dir().join(format!("folio-locked-pdf-{}", Id::new_v4()));
        std::fs::create_dir(&root).unwrap();
        let mut doc = source(1);
        doc.trailer.set(
            "ID",
            vec![
                Object::string_literal("folio-fixture-id"),
                Object::string_literal("folio-fixture-id"),
            ],
        );
        let encryption = lopdf::EncryptionState::try_from(lopdf::EncryptionVersion::V2 {
            document: &doc,
            owner_password: "owner",
            user_password: "secret",
            key_length: 128,
            permissions: lopdf::Permissions::all(),
        })
        .unwrap();
        doc.encrypt(&encryption).unwrap();
        doc.save(root.join("source.pdf")).unwrap();
        let bytes = std::fs::read(root.join("source.pdf")).unwrap();
        assert!(matches!(
            import(&root.join("source.pdf"), &root.join("assets")),
            Err(Error::PasswordRequired)
        ));
        let pages = import_with_password(
            &root.join("source.pdf"),
            &root.join("assets"),
            Some("secret"),
        )
        .unwrap();
        let working = &pages[0].properties.pdf.as_ref().unwrap().asset;
        assert!(working.ends_with("-unlocked.pdf"));
        assert!(
            !Document::load(root.join("assets").join(working))
                .unwrap()
                .is_encrypted()
        );
        let original = working.replace("-unlocked.pdf", ".pdf");
        assert_eq!(
            std::fs::read(root.join("assets").join(original)).unwrap(),
            bytes
        );
        assert!(!serde_json::to_string(&pages).unwrap().contains("secret"));
        std::fs::remove_dir_all(root).unwrap();
    }
}
