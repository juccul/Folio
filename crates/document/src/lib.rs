//! Versioned vector documents. Raw sensor samples are immutable; editing changes
//! an affine transform or style, never the original handwriting.
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};
pub use uuid::Uuid;
pub type Id = Uuid;
pub const FORMAT_VERSION: u32 = 4;
pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}
impl Point {
    pub fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
    pub fn distance(self, other: Self) -> f32 {
        (self.x - other.x).hypot(self.y - other.y)
    }
    pub fn lerp(self, other: Self, t: f32) -> Self {
        Self::new(
            self.x + (other.x - self.x) * t,
            self.y + (other.y - self.y) * t,
        )
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Rect {
    pub min: Point,
    pub max: Point,
}
impl Rect {
    pub fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self {
            min: Point::new(x, y),
            max: Point::new(x + w, y + h),
        }
    }
    pub fn from_points(points: impl IntoIterator<Item = Point>) -> Self {
        let mut p = points.into_iter();
        let Some(first) = p.next() else {
            return Self::default();
        };
        let mut r = Self {
            min: first,
            max: first,
        };
        for p in p {
            r.min.x = r.min.x.min(p.x);
            r.min.y = r.min.y.min(p.y);
            r.max.x = r.max.x.max(p.x);
            r.max.y = r.max.y.max(p.y)
        }
        r
    }
    pub fn width(self) -> f32 {
        self.max.x - self.min.x
    }
    pub fn height(self) -> f32 {
        self.max.y - self.min.y
    }
    pub fn center(self) -> Point {
        self.min.lerp(self.max, 0.5)
    }
    pub fn expand(self, n: f32) -> Self {
        Self::new(
            self.min.x - n,
            self.min.y - n,
            self.width() + 2. * n,
            self.height() + 2. * n,
        )
    }
    pub fn contains(self, p: Point) -> bool {
        p.x >= self.min.x && p.x <= self.max.x && p.y >= self.min.y && p.y <= self.max.y
    }
    pub fn intersects(self, b: Self) -> bool {
        self.min.x <= b.max.x
            && self.max.x >= b.min.x
            && self.min.y <= b.max.y
            && self.max.y >= b.min.y
    }
    pub fn union(self, b: Self) -> Self {
        Self::from_points([self.min, self.max, b.min, b.max])
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Transform {
    pub a: f32,
    pub b: f32,
    pub c: f32,
    pub d: f32,
    pub tx: f32,
    pub ty: f32,
}
impl Default for Transform {
    fn default() -> Self {
        Self {
            a: 1.,
            b: 0.,
            c: 0.,
            d: 1.,
            tx: 0.,
            ty: 0.,
        }
    }
}
impl Transform {
    pub fn apply(self, p: Point) -> Point {
        Point::new(
            self.a * p.x + self.c * p.y + self.tx,
            self.b * p.x + self.d * p.y + self.ty,
        )
    }
    /// Composition: apply rhs, then self.
    pub fn compose(self, rhs: Self) -> Self {
        Self {
            a: self.a * rhs.a + self.c * rhs.b,
            b: self.b * rhs.a + self.d * rhs.b,
            c: self.a * rhs.c + self.c * rhs.d,
            d: self.b * rhs.c + self.d * rhs.d,
            tx: self.a * rhs.tx + self.c * rhs.ty + self.tx,
            ty: self.b * rhs.tx + self.d * rhs.ty + self.ty,
        }
    }
    pub fn translate(x: f32, y: f32) -> Self {
        Self {
            tx: x,
            ty: y,
            ..Self::default()
        }
    }
    pub fn around(center: Point, scale: f32, angle: f32) -> Self {
        let (s, c) = angle.sin_cos();
        Self::translate(center.x, center.y)
            .compose(Self {
                a: scale * c,
                b: scale * s,
                c: -scale * s,
                d: scale * c,
                ..Self::default()
            })
            .compose(Self::translate(-center.x, -center.y))
    }
    pub fn inverse(self) -> Option<Self> {
        if ![self.a, self.b, self.c, self.d, self.tx, self.ty]
            .iter()
            .all(|value| value.is_finite())
        {
            return None;
        }
        let det = self.a as f64 * self.d as f64 - self.b as f64 * self.c as f64;
        if det.abs() < 1e-6 {
            return None;
        }
        let r = Self {
            a: (self.d as f64 / det) as f32,
            b: (-self.b as f64 / det) as f32,
            c: (-self.c as f64 / det) as f32,
            d: (self.a as f64 / det) as f32,
            tx: 0.,
            ty: 0.,
        };
        let p = r.apply(Point::new(-self.tx, -self.ty));
        let inverse = Self {
            tx: p.x,
            ty: p.y,
            ..r
        };
        [
            inverse.a, inverse.b, inverse.c, inverse.d, inverse.tx, inverse.ty,
        ]
        .iter()
        .all(|value| value.is_finite())
        .then_some(inverse)
    }
    pub fn scale(self) -> f32 {
        (self.a * self.d - self.b * self.c).abs().sqrt()
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}
impl Color {
    pub const INK: Self = Self {
        r: 43,
        g: 57,
        b: 52,
    };
    pub fn hex(self) -> String {
        format!("#{:02x}{:02x}{:02x}", self.r, self.g, self.b)
    }
    pub fn rgb(self) -> u32 {
        (self.r as u32) << 16 | (self.g as u32) << 8 | self.b as u32
    }
    pub fn from_rgb(c: u32) -> Self {
        Self {
            r: (c >> 16) as u8,
            g: (c >> 8) as u8,
            b: c as u8,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum InkTool {
    Ballpoint,
    Fountain,
    Pencil,
    Marker,
    Highlighter,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PenStyle {
    pub tool: InkTool,
    pub color: Color,
    pub width: f32,
    pub opacity: f32,
    pub stabilization: f32,
    pub pressure_gamma: f32,
}
impl PenStyle {
    pub fn valid(&self) -> bool {
        self.width.is_finite()
            && self.width > 0.
            && self.opacity.is_finite()
            && (0.0..=1.0).contains(&self.opacity)
            && self.stabilization.is_finite()
            && (0.0..=1.0).contains(&self.stabilization)
            && self.pressure_gamma.is_finite()
            && self.pressure_gamma > 0.
    }
}
impl Default for PenStyle {
    fn default() -> Self {
        Self {
            tool: InkTool::Ballpoint,
            color: Color::INK,
            width: 3.,
            opacity: 1.,
            stabilization: 0.25,
            pressure_gamma: 0.8,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct StrokePoint {
    pub x: f32,
    pub y: f32,
    pub pressure: f32,
    pub tilt_x: f32,
    pub tilt_y: f32,
    pub timestamp: u64,
    pub buttons: u32,
}
impl StrokePoint {
    pub fn position(self) -> Point {
        Point::new(self.x, self.y)
    }
    pub fn new(p: Point, pressure: f32, timestamp: u64) -> Self {
        Self {
            x: p.x,
            y: p.y,
            pressure,
            tilt_x: 0.,
            tilt_y: 0.,
            timestamp,
            buttons: 0,
        }
    }
    pub fn valid(self) -> bool {
        self.x.is_finite()
            && self.y.is_finite()
            && self.pressure.is_finite()
            && (0.0..=1.0).contains(&self.pressure)
            && self.tilt_x.is_finite()
            && self.tilt_y.is_finite()
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct PathPoint {
    pub position: Point,
    pub radius: f32,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct InkStroke {
    pub id: Id,
    pub raw: Arc<Vec<StrokePoint>>,
    pub path: Arc<Vec<PathPoint>>,
    pub style: PenStyle,
    pub transform: Transform,
    pub created_at: u64,
    /// A segment cut preserves the complete sensor data and ordinary path.
    #[serde(default)]
    pub fragment_path: Option<Vec<PathPoint>>,
    pub refined_path: Option<Vec<PathPoint>>,
    pub refinement_enabled: bool,
}
impl InkStroke {
    pub fn bounds(&self) -> Rect {
        let mut radius = 0.0_f32;
        let bounds = Rect::from_points(self.display_path().iter().map(|point| {
            radius = radius.max(point.radius);
            self.transform.apply(point.position)
        }));
        expand_brush_bounds(bounds, radius, self.transform)
    }
    pub fn display_path(&self) -> &[PathPoint] {
        if self.refinement_enabled {
            self.refined_path
                .as_deref()
                .unwrap_or_else(|| self.fragment_path.as_deref().unwrap_or(&self.path))
        } else {
            self.fragment_path.as_deref().unwrap_or(&self.path)
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TextBlock {
    pub id: Id,
    pub text: String,
    pub rect: Rect,
    pub transform: Transform,
    pub font_family: String,
    pub font_size: f32,
    pub color: Color,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub alignment: Alignment,
    pub list: ListStyle,
}
/// Word-wrap UTF-8 text without changing whitespace or paragraph boundaries.
/// Font measurement comes from the consumer's local shaping engine.
pub fn text_wrap_ranges(
    text: &str,
    width: f32,
    measure: impl Fn(&str) -> f32,
) -> Vec<std::ops::Range<usize>> {
    let mut result = Vec::new();
    let mut base = 0;
    for paragraph in text.split('\n') {
        let mut start = 0;
        let mut in_word = false;
        let mut word_start = 0;
        for (end, c) in paragraph
            .char_indices()
            .chain(std::iter::once((paragraph.len(), ' ')))
        {
            if !c.is_whitespace() && !in_word {
                word_start = end;
                in_word = true;
            }
            if c.is_whitespace() && in_word {
                if word_start > start && measure(&paragraph[start..end]) > width {
                    result.push(base + start..base + word_start);
                    start = word_start;
                }
                in_word = false;
            }
        }
        result.push(base + start..base + paragraph.len());
        base += paragraph.len() + 1;
    }
    result
}

impl TextBlock {
    /// Resize the frame while keeping glyphs orthogonal and the font size fixed.
    pub fn reflow(&mut self, resize: Transform) {
        let t = resize.compose(self.transform);
        let sx = t.a.hypot(t.b).max(0.001);
        let sy = t.c.hypot(t.d).max(0.001);
        let origin = t.apply(self.rect.min);
        let angle = self.transform.b.atan2(self.transform.a);
        let (sin, cos) = angle.sin_cos();
        self.rect = Rect::new(
            0.,
            0.,
            (self.rect.width() * sx).max(16.),
            (self.rect.height() * sy).max(16.),
        );
        self.transform = Transform {
            a: cos,
            b: sin,
            c: -sin,
            d: cos,
            tx: origin.x,
            ty: origin.y,
        };
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Alignment {
    Left,
    Center,
    Right,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ListStyle {
    None,
    Bullet,
    Numbered,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ImageObject {
    pub id: Id,
    pub asset: String,
    pub rect: Rect,
    pub transform: Transform,
    pub crop: Option<Rect>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ShapeKind {
    Line,
    Arrow,
    Rectangle,
    Square,
    Circle,
    Ellipse,
    Triangle,
    Diamond,
    Arc,
    Polyline,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Shape {
    pub id: Id,
    pub kind: ShapeKind,
    pub vertices: Vec<Point>,
    pub style: PenStyle,
    pub transform: Transform,
    pub source_strokes: Vec<Id>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MathLink {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
    pub expression: String,
    pub operation: String,
    pub variable: String,
    pub domain: String,
    pub angle: String,
    pub method: String,
    pub x_min: f64,
    pub x_max: f64,
    pub live: bool,
    pub sources: Vec<Id>,
    #[serde(default)]
    pub ink_region: Option<Rect>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Equation {
    pub id: Id,
    pub latex: String,
    pub rendered_svg: Option<String>,
    pub rect: Rect,
    pub source_strokes: Vec<Id>,
    pub transform: Transform,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub math_link: Option<MathLink>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Object {
    Stroke(InkStroke),
    Text(TextBlock),
    Image(ImageObject),
    Shape(Shape),
    Equation(Equation),
}
impl Object {
    pub fn id(&self) -> Id {
        match self {
            Self::Stroke(o) => o.id,
            Self::Text(o) => o.id,
            Self::Image(o) => o.id,
            Self::Shape(o) => o.id,
            Self::Equation(o) => o.id,
        }
    }
    pub fn set_id(&mut self, id: Id) {
        match self {
            Self::Stroke(o) => o.id = id,
            Self::Text(o) => o.id = id,
            Self::Image(o) => o.id = id,
            Self::Shape(o) => o.id = id,
            Self::Equation(o) => o.id = id,
        }
    }
    pub fn transform(&self) -> Transform {
        match self {
            Self::Stroke(o) => o.transform,
            Self::Text(o) => o.transform,
            Self::Image(o) => o.transform,
            Self::Shape(o) => o.transform,
            Self::Equation(o) => o.transform,
        }
    }
    pub fn set_transform(&mut self, t: Transform) {
        match self {
            Self::Stroke(o) => o.transform = t,
            Self::Text(o) => o.transform = t,
            Self::Image(o) => o.transform = t,
            Self::Shape(o) => o.transform = t,
            Self::Equation(o) => o.transform = t,
        }
    }
    pub fn bounds(&self) -> Rect {
        let t = self.transform();
        match self {
            Self::Stroke(s) => s.bounds(),
            Self::Shape(s) => brush_bounds(s.vertices.iter().copied(), s.style.width, t),
            Self::Text(o) => transformed_rect(o.rect, t),
            Self::Image(o) => transformed_rect(o.rect, t),
            Self::Equation(o) => transformed_rect(o.rect, t),
        }
    }
    pub fn style_mut(&mut self) -> Option<&mut PenStyle> {
        match self {
            Self::Stroke(s) => Some(&mut s.style),
            Self::Shape(s) => Some(&mut s.style),
            _ => None,
        }
    }
    pub fn searchable_text(&self) -> &str {
        match self {
            Self::Text(t) => &t.text,
            Self::Equation(e) => &e.latex,
            _ => "",
        }
    }
}
fn brush_bounds(
    points: impl IntoIterator<Item = Point>,
    radius: f32,
    transform: Transform,
) -> Rect {
    let bounds = Rect::from_points(points.into_iter().map(|point| transform.apply(point)));
    expand_brush_bounds(bounds, radius, transform)
}
fn expand_brush_bounds(bounds: Rect, radius: f32, transform: Transform) -> Rect {
    // A circular brush becomes an ellipse under nonuniform scaling/shearing.
    // The row norms give its extents along each axis; determinant scale does not.
    let x = radius * transform.a.hypot(transform.c);
    let y = radius * transform.b.hypot(transform.d);
    Rect {
        min: Point::new(bounds.min.x - x, bounds.min.y - y),
        max: Point::new(bounds.max.x + x, bounds.max.y + y),
    }
}
fn transformed_rect(r: Rect, t: Transform) -> Rect {
    Rect::from_points(
        [
            r.min,
            Point::new(r.max.x, r.min.y),
            r.max,
            Point::new(r.min.x, r.max.y),
        ]
        .map(|p| t.apply(p)),
    )
}
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
        let mut page = self.clone();
        page.id = Id::new_v4();
        page.revision = 0;
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
        page.groups.clear();
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
    pub fn text(&self) -> String {
        let mut text = self
            .ordered_objects()
            .map(|o| o.searchable_text().to_owned())
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>();
        if !self.ink_text.is_empty() {
            let hidden = self.hidden_sources();
            text.extend(
                self.ink_text
                    .iter()
                    .filter(|entry| {
                        !entry.stale
                            && entry
                                .sources
                                .iter()
                                .all(|id| self.objects.contains_key(id) && !hidden.contains(id))
                    })
                    .map(|entry| entry.text.clone()),
            );
        }
        text.join("\n")
    }
}
impl Default for Page {
    fn default() -> Self {
        Self::new()
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct NoteMetadata {
    #[serde(default)]
    pub cover_page: Option<Id>,
    pub id: Id,
    pub title: String,
    pub notebook: Option<Id>,
    pub tags: Vec<String>,
    pub favorite: bool,
    pub trashed: bool,
    pub created_at: u64,
    pub updated_at: u64,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Document {
    pub version: u32,
    pub metadata: NoteMetadata,
    pub pages: Vec<Page>,
}
impl Document {
    pub fn new(title: impl Into<String>) -> Self {
        let now = now_ms();
        Self {
            version: FORMAT_VERSION,
            metadata: NoteMetadata {
                cover_page: None,
                id: Id::new_v4(),
                title: title.into(),
                notebook: None,
                tags: Vec::new(),
                favorite: false,
                trashed: false,
                created_at: now,
                updated_at: now,
            },
            pages: vec![Page::new()],
        }
    }
    pub fn page(&self, id: Id) -> Option<&Page> {
        self.pages.iter().find(|p| p.id == id)
    }
    pub fn page_mut(&mut self, id: Id) -> Option<&mut Page> {
        self.pages.iter_mut().find(|p| p.id == id)
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.version == 0 || self.version > FORMAT_VERSION {
            return Err(format!("Unsupported document version {}", self.version));
        }
        if self.pages.is_empty() {
            return Err("A document needs at least one page".into());
        }
        let mut ids = std::collections::HashSet::new();
        for page in &self.pages {
            if !ids.insert(page.id) {
                return Err("Duplicate page ID".into());
            }
            for entry in &page.ink_text {
                if entry.text.trim().is_empty()
                    || entry.text.len() > 65536
                    || (!entry.stale && entry.sources.is_empty())
                    || entry.sources.len() > 4096
                    || entry
                        .sources
                        .iter()
                        .collect::<std::collections::HashSet<_>>()
                        .len()
                        != entry.sources.len()
                    || !entry.sources.iter().all(|id| {
                        page.objects
                            .get(id)
                            .is_some_and(|o| matches!(o.as_ref(), Object::Stroke(_)))
                    })
                    || ![
                        entry.bounds.min.x,
                        entry.bounds.min.y,
                        entry.bounds.max.x,
                        entry.bounds.max.y,
                    ]
                    .iter()
                    .all(|v| v.is_finite())
                    || entry.bounds.width() <= 0.
                    || entry.bounds.height() <= 0.
                {
                    return Err("Invalid handwriting search annotation".into());
                }
            }
            let p = &page.properties;
            if !p.width.is_finite() || !p.height.is_finite() || p.width <= 0. || p.height <= 0. {
                return Err("Invalid page size".into());
            }
            if page.order.len() != page.objects.len()
                || page
                    .order
                    .iter()
                    .collect::<std::collections::HashSet<_>>()
                    .len()
                    != page.order.len()
            {
                return Err("Invalid object order".into());
            }
            for id in &page.order {
                let Some(o) = page.objects.get(id) else {
                    return Err("Missing object".into());
                };
                if !ids.insert(*id) {
                    return Err("Duplicate object ID across pages".into());
                }
                if *id != o.id() {
                    return Err("Mismatched object ID".into());
                }
                let rect = match o.as_ref() {
                    Object::Text(text) => {
                        if !text.font_size.is_finite() || text.font_size <= 0. {
                            return Err("Invalid text font size".into());
                        }
                        Some(text.rect)
                    }
                    Object::Image(image) => Some(image.rect),
                    Object::Equation(equation) => Some(equation.rect),
                    Object::Stroke(stroke) => {
                        if !stroke.style.valid() {
                            return Err("Invalid stroke style".into());
                        }
                        None
                    }
                    Object::Shape(shape) => {
                        if !shape.style.valid()
                            || shape.vertices.len() < 2
                            || shape
                                .vertices
                                .iter()
                                .any(|point| !point.x.is_finite() || !point.y.is_finite())
                        {
                            return Err("Invalid shape geometry or style".into());
                        }
                        None
                    }
                };
                if rect.is_some_and(|rect| {
                    ![
                        rect.min.x,
                        rect.min.y,
                        rect.max.x,
                        rect.max.y,
                        rect.width(),
                        rect.height(),
                    ]
                    .iter()
                    .all(|value| value.is_finite())
                        || rect.width() <= 0.
                        || rect.height() <= 0.
                }) {
                    return Err("Invalid object rectangle".into());
                }
                let bounds = o.bounds();
                if ![
                    bounds.min.x,
                    bounds.min.y,
                    bounds.max.x,
                    bounds.max.y,
                    bounds.width(),
                    bounds.height(),
                ]
                .iter()
                .all(|value| value.is_finite())
                {
                    return Err("Object geometry overflows its transform".into());
                }
                if let Object::Equation(e) = o.as_ref()
                    && let Some(link) = &e.math_link
                {
                    if link.expression.len() > 8192
                        || link.variable.len() > 32
                        || link.sources.len() > 4096
                        || !link.x_min.is_finite()
                        || !link.x_max.is_finite()
                        || link.x_min >= link.x_max
                        || !matches!(
                            link.operation.as_str(),
                            "auto"
                                | "evaluate"
                                | "simplify"
                                | "factor"
                                | "solve"
                                | "differentiate"
                                | "integrate"
                                | "graph"
                                | "assign"
                        )
                        || !matches!(link.domain.as_str(), "real" | "complex")
                        || !matches!(link.angle.as_str(), "radians" | "degrees")
                    {
                        return Err("Invalid linked mathematical calculation".into());
                    }
                    if let Some(region) = link.ink_region
                        && (![region.min.x, region.min.y, region.max.x, region.max.y]
                            .iter()
                            .all(|v| v.is_finite())
                            || region.width() <= 0.
                            || region.height() <= 0.)
                    {
                        return Err("Invalid live mathematics region".into());
                    }
                }
                if let Object::Image(image) = o.as_ref()
                    && let Some(crop) = image.crop
                    && (!crop.min.x.is_finite()
                        || !crop.min.y.is_finite()
                        || !crop.max.x.is_finite()
                        || !crop.max.y.is_finite()
                        || crop.min.x < 0.
                        || crop.min.y < 0.
                        || crop.max.x > 1.
                        || crop.max.y > 1.
                        || crop.width() <= 0.
                        || crop.height() <= 0.)
                {
                    return Err("Invalid image crop".into());
                }
                let t = o.transform();
                if ![t.a, t.b, t.c, t.d, t.tx, t.ty]
                    .iter()
                    .all(|v| v.is_finite())
                    || t.inverse().is_none()
                {
                    return Err("Invalid transform".into());
                }
                if let Object::Stroke(s) = o.as_ref()
                    && (s.raw.is_empty()
                        || s.raw.iter().any(|p| !p.valid())
                        || s.path.is_empty()
                        || s.path
                            .iter()
                            .chain(s.fragment_path.iter().flatten())
                            .chain(s.refined_path.iter().flatten())
                            .any(|p| {
                                !p.position.x.is_finite()
                                    || !p.position.y.is_finite()
                                    || !p.radius.is_finite()
                                    || p.radius <= 0.
                            }))
                {
                    return Err("Invalid stroke".into());
                }
            }
        }
        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Notebook {
    pub id: Id,
    pub name: String,
    pub parent: Option<Id>,
}

/// Reversible, object-sized commands. History never copies a whole document.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Change {
    InkText {
        page: Id,
        before: Vec<InkText>,
        after: Vec<InkText>,
    },
    Groups {
        page: Id,
        before: Vec<InkGroup>,
        after: Vec<InkGroup>,
    },
    Object {
        page: Id,
        id: Id,
        before: Option<Arc<Object>>,
        after: Option<Arc<Object>>,
        index: usize,
    },
    Page {
        index: usize,
        before: Option<Page>,
        after: Option<Page>,
    },
    Properties {
        page: Id,
        before: PageProperties,
        after: PageProperties,
    },
    Metadata {
        before: NoteMetadata,
        after: NoteMetadata,
    },
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Command {
    pub label: String,
    pub changes: Vec<Change>,
}
impl Command {
    pub fn apply(&self, doc: &mut Document, forward: bool) {
        doc.version = FORMAT_VERSION;
        let mut inserted = Vec::new();
        let mut apply = |c: &Change, doc: &mut Document| match c {
            Change::InkText {
                page,
                before,
                after,
            } => {
                if let Some(page) = doc.page_mut(*page) {
                    page.ink_text = if forward {
                        after.clone()
                    } else {
                        before.clone()
                    };
                    page.revision += 1;
                }
            }
            Change::Groups {
                page,
                before,
                after,
            } => {
                if let Some(p) = doc.page_mut(*page) {
                    p.groups = if forward {
                        after.clone()
                    } else {
                        before.clone()
                    };
                    p.revision += 1;
                }
            }
            Change::Object {
                page,
                id,
                before,
                after,
                index,
            } => {
                if let Some(p) = doc.page_mut(*page) {
                    let value = if forward { after } else { before };
                    if let Some(o) = value {
                        if !p.objects.contains_key(id) {
                            p.order.push(*id);
                            inserted.push((*page, *id, *index));
                        }
                        p.objects.insert(*id, o.clone());
                    } else {
                        p.objects.remove(id);
                        p.order.retain(|v| v != id);
                    }
                    p.revision += 1;
                    p.groups.clear();
                }
            }
            Change::Page {
                index,
                before,
                after,
            } => {
                let old = if forward { before } else { after };
                let new = if forward { after } else { before };
                if let Some(old) = old {
                    doc.pages.retain(|p| p.id != old.id)
                }
                if let Some(new) = new {
                    doc.pages.insert((*index).min(doc.pages.len()), new.clone())
                }
            }
            Change::Properties {
                page,
                before,
                after,
            } => {
                if let Some(p) = doc.page_mut(*page) {
                    p.properties = if forward {
                        after.clone()
                    } else {
                        before.clone()
                    };
                    p.revision += 1;
                }
            }
            Change::Metadata { before, after } => {
                doc.metadata = if forward {
                    after.clone()
                } else {
                    before.clone()
                }
            }
        };
        if forward {
            for c in &self.changes {
                apply(c, doc)
            }
        } else {
            for c in self.changes.iter().rev() {
                apply(c, doc)
            }
        }
        // Restore original z-order after a batch deletion. Inserting immediately
        // while undoing in reverse shifts later objects into the wrong positions.
        inserted.sort_by_key(|(page, _, index)| (*page, *index));
        // Ordinary moves and metadata edits do not alter z-order. Appending
        // strokes already leaves them in the correct position; avoid scanning
        // unrelated pages or existing ink for either case.
        let mut start = 0;
        while start < inserted.len() {
            let page_id = inserted[start].0;
            let end = start
                + inserted[start..]
                    .iter()
                    .take_while(|(id, _, _)| *id == page_id)
                    .count();
            let objects = &inserted[start..end];
            if let Some(page) = doc.page_mut(page_id) {
                let suffix = page.order.len().saturating_sub(objects.len());
                let already_ordered = objects.iter().enumerate().all(|(offset, (_, id, index))| {
                    *index == suffix + offset && page.order.get(*index) == Some(id)
                });
                if !already_ordered {
                    let ids = objects
                        .iter()
                        .map(|(_, id, _)| *id)
                        .collect::<std::collections::HashSet<_>>();
                    page.order.retain(|id| !ids.contains(id));
                    for (_, id, index) in objects {
                        page.order.insert((*index).min(page.order.len()), *id);
                    }
                }
            }
            start = end;
        }
        doc.metadata.updated_at = now_ms();
    }
}
#[derive(Default, Clone, Debug, Serialize, Deserialize)]
pub struct History {
    undo: Vec<Command>,
    redo: Vec<Command>,
}
impl History {
    pub fn entries(&self) -> impl Iterator<Item = (&Command, bool)> {
        self.undo
            .iter()
            .map(|c| (c, true))
            .chain(self.redo.iter().rev().map(|c| (c, false)))
    }
    pub fn from_commands(undo: Vec<Command>, redo: Vec<Command>) -> Self {
        Self { undo, redo }
    }

    pub fn execute(&mut self, cmd: Command, doc: &mut Document) {
        if cmd.changes.is_empty() {
            return;
        }
        cmd.apply(doc, true);
        self.undo.push(cmd);
        self.redo.clear();
        if self.undo.len() > 512 {
            self.undo.remove(0);
        }
    }
    pub fn undo(&mut self, doc: &mut Document) -> Option<Command> {
        let cmd = self.undo.pop()?;
        cmd.apply(doc, false);
        self.redo.push(cmd.clone());
        Some(cmd)
    }
    pub fn redo(&mut self, doc: &mut Document) -> Option<Command> {
        let cmd = self.redo.pop()?;
        cmd.apply(doc, true);
        self.undo.push(cmd.clone());
        Some(cmd)
    }
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn legacy_document_is_readable_and_editing_upgrades_its_format() {
        let mut d = Document::new("legacy");
        d.version = 1;
        d.validate().unwrap();
        Command {
            label: "Upgrade".into(),
            changes: vec![],
        }
        .apply(&mut d, true);
        assert_eq!(d.version, FORMAT_VERSION);
    }
    #[test]
    fn serialization_and_validation() {
        let d = Document::new("研究 notes");
        let json = serde_json::to_string(&d).unwrap();
        let loaded: Document = serde_json::from_str(&json).unwrap();
        assert_eq!(loaded, d);
        loaded.validate().unwrap();
    }
    #[test]
    fn affine_roundtrip() {
        let p = Point::new(3., 7.);
        let t =
            Transform::around(Point::new(10., 20.), 2., 0.8).compose(Transform::translate(-4., 9.));
        assert!(t.inverse().unwrap().apply(t.apply(p)).distance(p) < 0.0001);
    }
    #[test]
    fn undo_page_and_metadata() {
        let mut d = Document::new("before");
        let original = d.clone();
        let mut h = History::default();
        let mut m = d.metadata.clone();
        m.title = "after".into();
        h.execute(
            Command {
                label: "change".into(),
                changes: vec![
                    Change::Metadata {
                        before: d.metadata.clone(),
                        after: m,
                    },
                    Change::Page {
                        index: 1,
                        before: None,
                        after: Some(Page::new()),
                    },
                ],
            },
            &mut d,
        );
        assert_eq!(d.pages.len(), 2);
        h.undo(&mut d).unwrap();
        assert_eq!(d.pages, original.pages);
        assert_eq!(d.metadata.title, "before");
        h.redo(&mut d).unwrap();
        assert_eq!(d.metadata.title, "after");
    }
    #[test]
    fn refuses_future_format() {
        let mut d = Document::new("x");
        d.version += 1;
        assert!(d.validate().is_err());
    }
}

#[cfg(test)]
mod edge_case_tests {
    use super::*;
    fn text() -> Arc<Object> {
        Arc::new(Object::Text(TextBlock {
            id: Id::new_v4(),
            text: "Example".into(),
            rect: Rect::new(0., 0., 100., 40.),
            transform: Transform::default(),
            font_family: "sans-serif".into(),
            font_size: 20.,
            color: Color::INK,
            bold: false,
            italic: false,
            underline: false,
            alignment: Alignment::Left,
            list: ListStyle::None,
        }))
    }
    #[test]
    fn duplicate_object_ids_across_pages_are_rejected() {
        let mut doc = Document::new("Collision");
        doc.pages.push(Page::new());
        let object = text();
        for page in &mut doc.pages {
            page.order.push(object.id());
            page.objects.insert(object.id(), object.clone());
        }
        assert!(doc.validate().is_err());
    }
    #[test]
    fn malformed_geometry_and_styles_are_rejected_before_rendering() {
        for variant in 0..5 {
            let mut doc = Document::new("Malformed");
            let mut object = text().as_ref().clone();
            if let Object::Text(text) = &mut object {
                match variant {
                    0 => text.font_size = f32::INFINITY,
                    1 => text.font_size = 0.,
                    2 => text.rect.max.x = f32::NAN,
                    3 => text.rect.max.y = text.rect.min.y - 1.,
                    _ => text.transform.a = f32::NAN,
                }
            }
            let id = object.id();
            doc.pages[0].order.push(id);
            doc.pages[0].objects.insert(id, Arc::new(object));
            assert!(
                doc.validate().is_err(),
                "Accepted malformed variant {variant}"
            );
        }
        let mut doc = Document::new("Shape");
        let shape = Object::Shape(Shape {
            id: Id::new_v4(),
            kind: ShapeKind::Line,
            vertices: vec![Point::new(0., 0.), Point::new(f32::INFINITY, 10.)],
            style: PenStyle::default(),
            transform: Transform::default(),
            source_strokes: vec![],
        });
        doc.pages[0].order.push(shape.id());
        doc.pages[0].objects.insert(shape.id(), Arc::new(shape));
        assert!(doc.validate().is_err());
    }
    #[test]
    fn anisotropic_ink_bounds_contain_the_transformed_brush() {
        let stroke = InkStroke {
            id: Id::new_v4(),
            raw: Arc::new(vec![StrokePoint::new(Point::new(0., 0.), 0.5, 0)]),
            path: Arc::new(vec![PathPoint {
                position: Point::new(0., 0.),
                radius: 4.,
            }]),
            style: PenStyle::default(),
            transform: Transform {
                a: 10.,
                d: 0.1,
                c: 3.,
                ..Default::default()
            },
            created_at: 0,
            fragment_path: None,
            refined_path: None,
            refinement_enabled: false,
        };
        let object = Object::Stroke(stroke.clone());
        let bounds = object.bounds();
        for i in 0..360 {
            let angle = (i as f32).to_radians();
            let p = stroke
                .transform
                .apply(Point::new(4. * angle.cos(), 4. * angle.sin()));
            assert!(
                bounds.contains(p),
                "Bounds {bounds:?} exclude transformed brush {p:?}"
            );
        }
    }
    #[test]
    fn nonfinite_or_singular_transforms_have_no_inverse() {
        for transform in [
            Transform {
                a: f32::NAN,
                ..Default::default()
            },
            Transform {
                tx: f32::INFINITY,
                ..Default::default()
            },
            Transform {
                a: 0.,
                ..Default::default()
            },
        ] {
            assert!(transform.inverse().is_none());
        }
    }
}
