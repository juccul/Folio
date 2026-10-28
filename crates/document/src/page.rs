//! Pages: their objects in drawing order, paper and handwriting metadata.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Paper {
    Blank,
    Ruled,
    Grid,
    Dots,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PageProperties {
    #[serde(default)]
    pub bookmark: Option<String>,
    pub width: f32,
    pub height: f32,
    pub paper: Paper,
    pub infinite: bool,
    #[serde(default)]
    pub color: Option<Color>,
    pub pdf: Option<PdfBackground>,
}
impl Default for PageProperties {
    fn default() -> Self {
        Self {
            bookmark: None,
            width: 794.,
            height: 1123.,
            paper: Paper::Ruled,
            infinite: false,
            color: None,
            pdf: None,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PdfBackground {
    pub asset: String,
    pub page: u32,
    pub preview_asset: Option<String>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
/// Retired recognition metadata retained for file and undo compatibility.
pub enum GroupLevel {
    Character,
    Word,
    Line,
    Paragraph,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
/// Opaque legacy data: no recognition, grouping or search uses these records.
pub struct InkGroup {
    pub id: Id,
    pub stroke_ids: Vec<Id>,
    pub bounding_box: Rect,
    pub level: GroupLevel,
    pub recognized_text: Option<String>,
    #[serde(default)]
    pub alternatives: Vec<String>,
    pub confidence: Option<f32>,
    pub language: String,
    pub engine: Option<String>,
    pub revision: u64,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct InkText {
    #[serde(default)]
    pub stale: bool,
    pub text: String,
    pub sources: Vec<Id>,
    pub bounds: Rect,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Page {
    #[serde(default)]
    pub ink_text: Vec<InkText>,
    pub id: Id,
    pub properties: PageProperties,
    pub objects: BTreeMap<Id, Arc<Object>>,
    pub order: Vec<Id>,
    pub groups: Vec<InkGroup>,
    pub revision: u64,
}
impl Page {
    pub fn new() -> Self {
        Self {
            ink_text: vec![],
            id: Id::new_v4(),
            properties: PageProperties::default(),
            objects: BTreeMap::new(),
            order: Vec::new(),
            groups: Vec::new(),
            revision: 0,
        }
    }
    /// Clone editable content with fresh identities and remap page-local dependencies.
    pub fn duplicate(&self) -> Self {
        // Object identities/order are rebuilt below. Avoid cloning the old map
        // and order just to discard them, especially on large imported pages.
        let mut page = Self {
            id: Id::new_v4(),
            revision: 0,
            properties: self.properties.clone(),
            objects: BTreeMap::new(),
            order: Vec::new(),
            groups: Vec::new(),
            ink_text: self.ink_text.clone(),
        };
        let ids: BTreeMap<_, _> = self.order.iter().map(|id| (*id, Id::new_v4())).collect();
        page.objects = self
            .objects
            .values()
            .map(|object| {
                let mut object = object.as_ref().clone();
                object.set_id(ids[&object.id()]);
                match &mut object {
                    Object::Shape(shape) => {
                        shape.source_strokes.retain(|id| ids.contains_key(id));
                        for id in &mut shape.source_strokes {
                            *id = ids[id];
                        }
                    }
                    Object::Equation(equation) => {
                        equation.source_strokes.retain(|id| ids.contains_key(id));
                        for id in &mut equation.source_strokes {
                            *id = ids[id];
                        }
                        if let Some(link) = &mut equation.math_link {
                            link.sources.retain(|id| ids.contains_key(id));
                            for id in &mut link.sources {
                                *id = ids[id];
                            }
                        }
                    }
                    _ => {}
                }
                (object.id(), Arc::new(object))
            })
            .collect();
        page.order = self.order.iter().map(|id| ids[id]).collect();
        for entry in &mut page.ink_text {
            for source in &mut entry.sources {
                *source = ids[source];
            }
        }
        page
    }
    pub fn ordered_objects(&self) -> impl DoubleEndedIterator<Item = &Arc<Object>> {
        self.order.iter().filter_map(|id| self.objects.get(id))
    }
    pub fn hidden_sources(&self) -> std::collections::HashSet<Id> {
        self.objects
            .values()
            .flat_map(|o| match o.as_ref() {
                Object::Shape(s) => s.source_strokes.as_slice(),
                Object::Equation(e) => e.source_strokes.as_slice(),
                _ => &[],
            })
            .copied()
            .collect()
    }
    /// Searchable content in display order without cloning individual strings.
    pub fn text_fragments(&self) -> impl Iterator<Item = &str> {
        let hidden = if self.ink_text.is_empty() {
            std::collections::HashSet::new()
        } else {
            self.hidden_sources()
        };
        self.ordered_objects()
            .map(|object| object.searchable_text())
            .filter(|text| !text.is_empty())
            .chain(
                self.ink_text
                    .iter()
                    .filter(move |entry| {
                        !entry.stale
                            && entry
                                .sources
                                .iter()
                                .all(|id| self.objects.contains_key(id) && !hidden.contains(id))
                    })
                    .map(|entry| entry.text.as_str()),
            )
    }
    pub fn text(&self) -> String {
        let mut text = String::new();
        for (index, fragment) in self.text_fragments().enumerate() {
            if index > 0 {
                text.push('\n');
            }
            text.push_str(fragment);
        }
        text
    }
}
impl Default for Page {
    fn default() -> Self {
        Self::new()
    }
}
