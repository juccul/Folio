//! Page objects: ink strokes, text, images, shapes and equations.
use super::*;

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
// Pages and commands store objects behind Arc. Boxing the largest variant
// would add a second allocation and indirection to each equation object.
#[allow(clippy::large_enum_variant)]
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
