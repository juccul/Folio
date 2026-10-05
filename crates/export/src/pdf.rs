use super::*;
use krilla_svg::SurfaceExt;
use lopdf::{Dictionary, Document as Pdf, Object as PdfObject, Stream, dictionary};
use std::collections::HashMap;

fn inherited(doc: &Pdf, id: lopdf::ObjectId, key: &[u8]) -> Option<PdfObject> {
    let mut current = id;
    for _ in 0..64 {
        let page = doc.get_dictionary(current).ok()?;
        if let Ok(value) = page.get(key) {
            return Some(value.clone());
        }
        current = page.get(b"Parent").ok()?.as_reference().ok()?;
    }
    None
}
fn resources(doc: &Pdf, id: lopdf::ObjectId) -> Dictionary {
    let Some(value) = inherited(doc, id, b"Resources") else {
        return Dictionary::new();
    };
    match value {
        PdfObject::Dictionary(d) => d,
        PdfObject::Reference(id) => doc.get_dictionary(id).cloned().unwrap_or_default(),
        _ => Dictionary::new(),
    }
}

/// Preserve original PDF objects and overlay vector ink, subset-font searchable
/// Unicode text, images and equation outlines. Source files remain unchanged.
pub fn export(doc: &Document, assets: &Path, path: &Path) -> Result<()> {
    let mut annotations = krilla::Document::new();
    for page in &doc.pages {
        let mut layer = page.clone();
        layer.properties.pdf = None;
        let svg = page_svg(&layer, assets, page.properties.pdf.is_none())?;
        let tree = resvg::usvg::Tree::from_str(&svg, &svg_options())?;
        let size =
            krilla::geom::Size::from_wh(tree.size().width() * 0.75, tree.size().height() * 0.75)
                .ok_or_else(|| Error::Invalid("Invalid PDF page size".into()))?;
        let mut page = annotations.start_page_with(krilla::page::PageSettings::new(size));
        let mut surface = page.surface();
        surface.draw_svg(&tree, size, krilla_svg::SvgSettings::default());
        surface.finish();
        page.finish();
    }
    let data = annotations
        .finish()
        .map_err(|e| Error::Invalid(format!("Vector PDF: {e}")))?;
    if doc.pages.iter().all(|p| p.properties.pdf.is_none()) {
        return atomic_write(path, &data);
    }
    let mut output = Pdf::with_version("1.7");
    let mut originals: HashMap<String, std::collections::BTreeMap<u32, lopdf::ObjectId>> =
        HashMap::new();
    let mut templates = HashMap::new();
    let mut used_pages = std::collections::HashSet::new();
    let mut catalog = Dictionary::new();
    let mut catalog_names = Dictionary::new();
    let mut form_dictionary = Dictionary::new();
    let mut names = Vec::new();
    let mut fields = Vec::new();
    for page in &doc.pages {
        let Some(background) = &page.properties.pdf else {
            continue;
        };
        if originals.contains_key(&background.asset) {
            continue;
        }
        let mut source = Pdf::load(asset_path(assets, &background.asset)?)
            .map_err(|e| Error::Invalid(format!("Original PDF: {e}")))?;
        if source.is_encrypted() {
            source
                .decrypt("")
                .map_err(|_| Error::Invalid("Unlock this PDF before export".into()))?;
        }
        source.renumber_objects_with(output.max_id + 1);
        let pages = source.get_pages();
        // Inherited attributes must become local before reparenting page trees.
        for id in pages.values() {
            for key in [
                b"Resources".as_slice(),
                b"MediaBox",
                b"CropBox",
                b"Rotate",
                b"UserUnit",
            ] {
                if let Some(value) = inherited(&source, *id, key) {
                    source
                        .get_dictionary_mut(*id)
                        .map_err(|e| Error::Invalid(e.to_string()))?
                        .set(key, value);
                }
            }
        }
        let source_catalog = source
            .trailer
            .get(b"Root")
            .ok()
            .and_then(|v| v.as_reference().ok())
            .and_then(|id| source.get_dictionary(id).ok())
            .cloned()
            .unwrap_or_default();
        if catalog.is_empty() {
            catalog = source_catalog.clone();
            for (key, target) in [
                (b"Names".as_slice(), &mut catalog_names),
                (b"AcroForm".as_slice(), &mut form_dictionary),
            ] {
                if let Ok(value) = source_catalog.get(key) {
                    *target = match value {
                        PdfObject::Dictionary(d) => d.clone(),
                        PdfObject::Reference(id) => {
                            source.get_dictionary(*id).cloned().unwrap_or_default()
                        }
                        _ => Dictionary::new(),
                    };
                }
            }
        }
        // Retain attachments from every input; flatten their name trees.
        fn collect_names(doc: &Pdf, object: &PdfObject, out: &mut Vec<PdfObject>, depth: usize) {
            if depth > 32 {
                return;
            }
            let dict = match object {
                PdfObject::Reference(id) => doc.get_dictionary(*id).ok(),
                PdfObject::Dictionary(d) => Some(d),
                _ => None,
            };
            if let Some(dict) = dict {
                if let Ok(array) = dict.get(b"Names").and_then(PdfObject::as_array) {
                    out.extend(array.iter().cloned());
                }
                if let Ok(kids) = dict.get(b"Kids").and_then(PdfObject::as_array) {
                    for kid in kids {
                        collect_names(doc, kid, out, depth + 1);
                    }
                }
            }
        }
        if let Ok(object) = source_catalog.get(b"Names") {
            let dict = match object {
                PdfObject::Reference(id) => source.get_dictionary(*id).ok(),
                PdfObject::Dictionary(d) => Some(d),
                _ => None,
            };
            if let Some(dict) = dict
                && let Ok(embedded) = dict.get(b"EmbeddedFiles")
            {
                collect_names(&source, embedded, &mut names, 0);
            }
        }
        if let Ok(form) = source_catalog.get(b"AcroForm") {
            let dict = match form {
                PdfObject::Reference(id) => source.get_dictionary(*id).ok(),
                PdfObject::Dictionary(d) => Some(d),
                _ => None,
            };
            if let Some(dict) = dict
                && let Ok(array) = dict.get(b"Fields").and_then(PdfObject::as_array)
            {
                fields.extend(array.iter().cloned());
            }
        }
        for id in pages.values() {
            templates.insert(
                *id,
                (
                    source
                        .get_dictionary(*id)
                        .map_err(|e| Error::Invalid(e.to_string()))?
                        .clone(),
                    resources(&source, *id),
                    source.get_page_contents(*id),
                ),
            );
        }
        originals.insert(background.asset.clone(), pages);
        output.max_id = source.max_id;
        output.objects.extend(source.objects);
    }
    let mut overlay = Pdf::load_mem(&data).map_err(|e| Error::Invalid(e.to_string()))?;
    overlay.renumber_objects_with(output.max_id + 1);
    let overlay_pages = overlay.get_pages();
    output.max_id = overlay.max_id;
    let pages_id = output.new_object_id();
    let mut page_ids = Vec::new();
    for (i, page) in doc.pages.iter().enumerate() {
        let overlay_id = overlay_pages[&(i as u32 + 1)];
        let mut dictionary = if let Some(background) = &page.properties.pdf {
            let id = *originals[&background.asset]
                .get(&background.page)
                .ok_or_else(|| Error::Invalid("Original PDF page missing".into()))?;
            let (mut dictionary, mut resource, original_contents) = templates[&id].clone();
            let output_id = if used_pages.insert(id) {
                id
            } else {
                let new_id = output.new_object_id();
                if let Ok(annotations) = dictionary.get(b"Annots").and_then(PdfObject::as_array) {
                    let mut copies = Vec::new();
                    for annotation in annotations {
                        if let Ok(original_id) = annotation.as_reference() {
                            if let Ok(original) = output.get_dictionary(original_id) {
                                let mut copy = original.clone();
                                copy.set("P", new_id);
                                let parent =
                                    copy.get(b"Parent").ok().and_then(|v| v.as_reference().ok());
                                let copy_id = output.add_object(copy);
                                copies.push(PdfObject::Reference(copy_id));
                                if let Some(parent) = parent
                                    && let Ok(field) = output.get_dictionary_mut(parent)
                                {
                                    let mut kids = field
                                        .get(b"Kids")
                                        .and_then(PdfObject::as_array)
                                        .cloned()
                                        .unwrap_or_default();
                                    kids.push(PdfObject::Reference(copy_id));
                                    field.set("Kids", kids);
                                }
                            }
                        } else {
                            copies.push(annotation.clone());
                        }
                    }
                    dictionary.set("Annots", copies);
                }
                new_id
            };
            let form = Stream::new(
                dictionary! {"Type"=>"XObject","Subtype"=>"Form","BBox"=>vec![0.into(),0.into(),(page.properties.width*0.75).into(),(page.properties.height*0.75).into()],"Resources"=>resources(&overlay,overlay_id)},
                overlay.get_page_content(overlay_id),
            );
            let form_id = output.add_object(form);

            let mut xobjects = resource
                .get(b"XObject")
                .ok()
                .and_then(|v| match v {
                    PdfObject::Dictionary(d) => Some(d.clone()),
                    PdfObject::Reference(id) => output.get_dictionary(*id).ok().cloned(),
                    _ => None,
                })
                .unwrap_or_default();
            let name = format!("FolioOverlay{i}");
            xobjects.set(name.as_bytes(), form_id);
            resource.set("XObject", xobjects);
            dictionary.set("Resources", resource);
            let crop = inherited(&output, id, b"CropBox")
                .or_else(|| inherited(&output, id, b"MediaBox"))
                .and_then(|v| {
                    let value = if let PdfObject::Reference(id) = v {
                        output.get_object(id).ok()?.clone()
                    } else {
                        v
                    };
                    value.as_array().ok().cloned()
                })
                .ok_or_else(|| Error::Invalid("PDF page has no box".into()))?;
            let n = |i: usize| crop.get(i).and_then(|v| v.as_float().ok()).unwrap_or(0.);
            let (x, y, w, h) = (n(0), n(1), n(2) - n(0), n(3) - n(1));
            let rotate = dictionary
                .get(b"Rotate")
                .ok()
                .and_then(|v| v.as_i64().ok())
                .unwrap_or(0)
                .rem_euclid(360);
            let unit = dictionary
                .get(b"UserUnit")
                .ok()
                .and_then(|v| v.as_float().ok())
                .filter(|v| v.is_finite() && *v > 0.)
                .unwrap_or(1.);
            let inv = 1. / unit;
            let matrix = match rotate {
                90 => format!("0 {inv} {} 0 {} {y}", -inv, x + w),
                180 => format!("{} 0 0 {} {} {}", -inv, -inv, x + w, y + h),
                270 => format!("0 {} {inv} 0 {x} {}", -inv, y + h),
                _ => format!("{inv} 0 0 {inv} {x} {y}"),
            };
            let content = output.add_object(Stream::new(
                Dictionary::new(),
                format!("q {matrix} cm /{name} Do Q\n").into_bytes(),
            ));
            // Isolate the original graphics state before appending annotations.
            let mut contents = vec![PdfObject::Reference(
                output.add_object(Stream::new(Dictionary::new(), b"q\n".to_vec())),
            )];
            contents.extend(original_contents.into_iter().map(PdfObject::Reference));
            contents.push(PdfObject::Reference(
                output.add_object(Stream::new(Dictionary::new(), b"Q\n".to_vec())),
            ));
            contents.push(PdfObject::Reference(content));
            dictionary.set("Contents", contents);
            // Reuse original page identity so links and form widgets stay valid.
            dictionary.set("Parent", pages_id);
            output.set_object(output_id, dictionary);
            page_ids.push(output_id);
            continue;
        } else {
            overlay
                .get_dictionary(overlay_id)
                .map_err(|e| Error::Invalid(e.to_string()))?
                .clone()
        };
        dictionary.set("Parent", pages_id);
        let id = output.add_object(dictionary);
        page_ids.push(id);
    }
    output.objects.extend(overlay.objects);
    output.set_object(pages_id,dictionary! {"Type"=>"Pages","Kids"=>page_ids.iter().map(|id|PdfObject::Reference(*id)).collect::<Vec<_>>(),"Count"=>page_ids.len() as i64});
    catalog.set("Type", "Catalog");
    catalog.set("Pages", pages_id);
    if !names.is_empty() {
        catalog_names.set("EmbeddedFiles", dictionary! {"Names"=>names});
        catalog.set("Names", catalog_names);
    }
    if !fields.is_empty() {
        form_dictionary.set("Fields", fields);
        catalog.set("AcroForm", form_dictionary);
    }
    let root = output.add_object(catalog);
    output.trailer.set("Root", root);
    let mut bytes = Vec::new();
    output
        .save_to(&mut bytes)
        .map_err(|e| Error::Invalid(e.to_string()))?;
    atomic_write(path, &bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn duplicate_source_pages_keep_independent_overlays_links_and_catalog() {
        let root = std::env::temp_dir().join(format!("folio-pdf-fidelity-{}", Id::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let mut source = Pdf::with_version("1.7");
        let pages = source.new_object_id();
        let page = source.new_object_id();
        let content = source.add_object(Stream::new(
            Dictionary::new(),
            b"0 0 1 rg 10 10 20 20 re f\n".to_vec(),
        ));
        let link=source.add_object(dictionary! {"Type"=>"Annot","Subtype"=>"Link","Rect"=>vec![10.into(),10.into(),30.into(),30.into()],"P"=>page,"A"=>dictionary! {"S"=>"URI","URI"=>PdfObject::string_literal("https://example.org")}});
        source.set_object(page,dictionary! {"Type"=>"Page","Parent"=>pages,"Contents"=>content,"Annots"=>vec![PdfObject::Reference(link)]});
        source.set_object(pages,dictionary! {"Type"=>"Pages","Kids"=>vec![PdfObject::Reference(page)],"Count"=>1,"MediaBox"=>vec![0.into(),0.into(),300.into(),400.into()],"Resources"=>Dictionary::new()});
        let attachment = source.add_object(Stream::new(
            dictionary! {"Type"=>"EmbeddedFile"},
            b"retained attachment".to_vec(),
        ));
        let file=source.add_object(dictionary! {"Type"=>"Filespec","F"=>PdfObject::string_literal("readme.txt"),"EF"=>dictionary! {"F"=>attachment}});
        let catalog=source.add_object(dictionary! {"Type"=>"Catalog","Pages"=>pages,"Names"=>dictionary! {"Dests"=>dictionary! {"Names"=>vec![PdfObject::string_literal("chapter"),PdfObject::Array(vec![PdfObject::Reference(page),PdfObject::Name(b"Fit".to_vec())])]},"EmbeddedFiles"=>dictionary! {"Names"=>vec![PdfObject::string_literal("readme.txt"),PdfObject::Reference(file)]}},"AcroForm"=>dictionary! {"Fields"=>Vec::<PdfObject>::new(),"DA"=>PdfObject::string_literal("/Helv 12 Tf 0 g")}});
        source.trailer.set("Root", catalog);
        source.save(root.join("original.pdf")).unwrap();
        let original = std::fs::read(root.join("original.pdf")).unwrap();
        let mut doc = Document::new("duplicates");
        doc.pages.push(Page::new());
        for p in &mut doc.pages {
            p.properties.width = 400.;
            p.properties.height = 400. / 0.75;
            p.properties.pdf = Some(PdfBackground {
                asset: "original.pdf".into(),
                page: 1,
                preview_asset: None,
            });
        }
        export(&doc, &root, &root.join("annotated.pdf")).unwrap();
        assert_eq!(std::fs::read(root.join("original.pdf")).unwrap(), original);
        let output = Pdf::load(root.join("annotated.pdf")).unwrap();
        let pages = output.get_pages();
        assert_eq!(pages.len(), 2);
        assert_ne!(pages[&1], pages[&2]);
        let first = String::from_utf8(output.get_page_content(pages[&1])).unwrap();
        let second = String::from_utf8(output.get_page_content(pages[&2])).unwrap();
        assert!(first.contains("FolioOverlay0"));
        assert!(!second.contains("FolioOverlay0"));
        assert!(second.contains("FolioOverlay1"));
        for id in pages.values() {
            let annotation = output
                .get_dictionary(*id)
                .unwrap()
                .get(b"Annots")
                .unwrap()
                .as_array()
                .unwrap()[0]
                .as_reference()
                .unwrap();
            assert_eq!(
                output
                    .get_dictionary(annotation)
                    .unwrap()
                    .get(b"P")
                    .unwrap()
                    .as_reference()
                    .unwrap(),
                *id
            );
        }
        let catalog = output.catalog().unwrap();
        let names = catalog.get(b"Names").unwrap().as_dict().unwrap();
        assert!(names.has(b"Dests"));
        assert!(names.has(b"EmbeddedFiles"));
        let form = catalog.get(b"AcroForm").unwrap().as_dict().unwrap();
        assert!(form.has(b"DA"));
        std::fs::remove_dir_all(root).unwrap();
    }
}
