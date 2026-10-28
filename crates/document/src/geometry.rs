//! Points, rectangles and affine transforms in document coordinates.
use super::*;

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
