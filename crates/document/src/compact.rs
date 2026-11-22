//! Bit-exact lossless compressed storage for immutable ink stroke sample buffers.
//!
//! Raw sensor samples (`StrokePoint`) and smoothed display paths (`PathPoint`)
//! dominate memory in ink-heavy notebooks. This module stores them as compact
//! interleaved delta-coded byte slices while preserving every IEEE-754 bit
//! (including `-0.0` and `NaN` payloads) and standard JSON array serialization.
use crate::{PathPoint, Point, Rect, StrokePoint};
use serde::{
    Deserialize, Deserializer, Serialize, Serializer,
    de::{SeqAccess, Visitor},
    ser::SerializeSeq,
};
use std::fmt;

const FLAG_CONST_PRESSURE: u8 = 1 << 0;
const FLAG_ZERO_TILT_X: u8 = 1 << 1;
const FLAG_CONST_TILT_X: u8 = 1 << 2;
const FLAG_ZERO_TILT_Y: u8 = 1 << 3;
const FLAG_CONST_TILT_Y: u8 = 1 << 4;
const FLAG_ZERO_BUTTONS: u8 = 1 << 5;
const FLAG_CONST_BUTTONS: u8 = 1 << 6;
const FLAG_LINEAR_TIME: u8 = 1 << 7;

const PATH_FLAG_CONST_RADIUS: u8 = 1 << 0;
const PATH_FLAG_HAS_NAN: u8 = 1 << 1;

/// Order-preserving bijection between all `f32` bit patterns and `u32`.
#[inline]
fn f32_to_ordered(v: f32) -> u32 {
    let b = v.to_bits();
    if b & 0x8000_0000 != 0 {
        !b
    } else {
        b ^ 0x8000_0000
    }
}

#[inline]
fn f32_from_ordered(o: u32) -> f32 {
    let b = if o & 0x8000_0000 != 0 {
        o ^ 0x8000_0000
    } else {
        !o
    };
    f32::from_bits(b)
}

#[inline]
fn pred_f32(index: usize, prev1: f32, prev2: f32) -> f32 {
    if index >= 2 {
        let p = 2.0_f32 * prev1 - prev2;
        if p.is_finite() { p } else { prev1 }
    } else {
        prev1
    }
}

#[inline]
fn zigzag32(v: u32) -> u32 {
    let s = v as i32;
    ((s << 1) ^ (s >> 31)) as u32
}

#[inline]
fn unzigzag32(v: u32) -> u32 {
    ((v >> 1) ^ (0_u32.wrapping_sub(v & 1))) as i32 as u32
}

#[inline]
fn zigzag64(v: u64) -> u64 {
    let s = v as i64;
    ((s << 1) ^ (s >> 63)) as u64
}

#[inline]
fn unzigzag64(v: u64) -> u64 {
    ((v >> 1) ^ (0_u64.wrapping_sub(v & 1))) as i64 as u64
}

#[inline]
fn write_u32_le(out: &mut Vec<u8>, v: u32) {
    out.extend_from_slice(&v.to_le_bytes());
}

#[inline]
fn write_uleb32(out: &mut Vec<u8>, mut v: u32) {
    while v >= 0x80 {
        out.push((v as u8 & 0x7f) | 0x80);
        v >>= 7;
    }
    out.push(v as u8);
}

#[inline]
fn write_uleb64(out: &mut Vec<u8>, mut v: u64) {
    while v >= 0x80 {
        out.push((v as u8 & 0x7f) | 0x80);
        v >>= 7;
    }
    out.push(v as u8);
}

/// Bounds-checked byte reader shared by both streaming iterators and blob validation.
struct ByteReader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> ByteReader<'a> {
    #[inline]
    fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    #[inline]
    fn is_eof(&self) -> bool {
        self.pos == self.data.len()
    }

    #[inline]
    fn read_u8(&mut self) -> Option<u8> {
        let &byte = self.data.get(self.pos)?;
        self.pos += 1;
        Some(byte)
    }

    #[inline]
    fn read_u32_le(&mut self) -> Option<u32> {
        let slice = self.data.get(self.pos..self.pos + 4)?;
        self.pos += 4;
        Some(u32::from_le_bytes([slice[0], slice[1], slice[2], slice[3]]))
    }

    #[inline]
    fn read_uleb32(&mut self) -> Option<u32> {
        let mut result = 0_u32;
        let mut shift = 0_u32;
        loop {
            let byte = self.read_u8()?;
            if shift >= 32 || (shift == 28 && byte > 0x0f) {
                return None;
            }
            if shift > 0 && byte == 0 {
                return None;
            }
            result |= ((byte & 0x7f) as u32) << shift;
            if byte & 0x80 == 0 {
                return Some(result);
            }
            shift += 7;
        }
    }

    #[inline]
    fn read_uleb64(&mut self) -> Option<u64> {
        let mut result = 0_u64;
        let mut shift = 0_u32;
        loop {
            let byte = self.read_u8()?;
            if shift >= 64 || (shift == 63 && byte > 0x01) {
                return None;
            }
            if shift > 0 && byte == 0 {
                return None;
            }
            result |= ((byte & 0x7f) as u64) << shift;
            if byte & 0x80 == 0 {
                return Some(result);
            }
            shift += 7;
        }
    }
}

/// Lossless, bit-exact compressed buffer of [`StrokePoint`] sensor samples.
#[derive(Clone, Default)]
pub struct RawSamples {
    len: u32,
    has_nan: bool,
    bytes: Box<[u8]>,
}

impl RawSamples {
    pub fn from_slice(points: &[StrokePoint]) -> Self {
        let len = u32::try_from(points.len()).expect("stroke exceeds u32::MAX raw points");
        if points.is_empty() {
            return Self::default();
        }
        let first = points[0];
        let mut flags = 0_u8;
        if points
            .iter()
            .all(|p| p.pressure.to_bits() == first.pressure.to_bits())
        {
            flags |= FLAG_CONST_PRESSURE;
        }
        if points.iter().all(|p| p.tilt_x.to_bits() == 0) {
            flags |= FLAG_ZERO_TILT_X;
        } else if points
            .iter()
            .all(|p| p.tilt_x.to_bits() == first.tilt_x.to_bits())
        {
            flags |= FLAG_CONST_TILT_X;
        }
        if points.iter().all(|p| p.tilt_y.to_bits() == 0) {
            flags |= FLAG_ZERO_TILT_Y;
        } else if points
            .iter()
            .all(|p| p.tilt_y.to_bits() == first.tilt_y.to_bits())
        {
            flags |= FLAG_CONST_TILT_Y;
        }
        if points.iter().all(|p| p.buttons == 0) {
            flags |= FLAG_ZERO_BUTTONS;
        } else if points.iter().all(|p| p.buttons == first.buttons) {
            flags |= FLAG_CONST_BUTTONS;
        }
        let linear_dt = if points.len() <= 1 {
            Some(0_u64)
        } else {
            let dt = points[1].timestamp.wrapping_sub(first.timestamp);
            points
                .iter()
                .enumerate()
                .all(|(i, p)| {
                    p.timestamp == first.timestamp.wrapping_add((i as u64).wrapping_mul(dt))
                })
                .then_some(dt)
        };
        if linear_dt.is_some() {
            flags |= FLAG_LINEAR_TIME;
        }

        let mut out = Vec::with_capacity(24 + points.len() * 8);
        out.push(flags);
        write_u32_le(&mut out, first.x.to_bits());
        write_u32_le(&mut out, first.y.to_bits());
        write_u32_le(&mut out, first.pressure.to_bits());
        if flags & FLAG_ZERO_TILT_X == 0 {
            write_u32_le(&mut out, first.tilt_x.to_bits());
        }
        if flags & FLAG_ZERO_TILT_Y == 0 {
            write_u32_le(&mut out, first.tilt_y.to_bits());
        }
        if flags & FLAG_ZERO_BUTTONS == 0 {
            write_uleb32(&mut out, first.buttons);
        }
        write_uleb64(&mut out, first.timestamp);
        if let Some(dt) = linear_dt
            && points.len() > 1
        {
            write_uleb64(&mut out, zigzag64(dt));
        }

        let mut prev_x = first.x;
        let mut prev2_x = first.x;
        let mut prev_y = first.y;
        let mut prev2_y = first.y;
        let mut prev_p = first.pressure;
        let mut prev2_p = first.pressure;
        let mut prev_tx = f32_to_ordered(first.tilt_x);
        let mut prev_ty = f32_to_ordered(first.tilt_y);
        let mut prev_btn = first.buttons;
        let mut prev_t = first.timestamp;
        let mut prev_dt = 0_u64;

        for (i, point) in points.iter().enumerate().skip(1) {
            let px = pred_f32(i, prev_x, prev2_x);
            write_uleb32(
                &mut out,
                zigzag32(f32_to_ordered(point.x).wrapping_sub(f32_to_ordered(px))),
            );
            prev2_x = prev_x;
            prev_x = point.x;

            let py = pred_f32(i, prev_y, prev2_y);
            write_uleb32(
                &mut out,
                zigzag32(f32_to_ordered(point.y).wrapping_sub(f32_to_ordered(py))),
            );
            prev2_y = prev_y;
            prev_y = point.y;

            if flags & FLAG_CONST_PRESSURE == 0 {
                let pp = pred_f32(i, prev_p, prev2_p);
                write_uleb32(
                    &mut out,
                    zigzag32(f32_to_ordered(point.pressure).wrapping_sub(f32_to_ordered(pp))),
                );
                prev2_p = prev_p;
                prev_p = point.pressure;
            }
            if flags & (FLAG_ZERO_TILT_X | FLAG_CONST_TILT_X) == 0 {
                let cur = f32_to_ordered(point.tilt_x);
                write_uleb32(&mut out, zigzag32(cur.wrapping_sub(prev_tx)));
                prev_tx = cur;
            }
            if flags & (FLAG_ZERO_TILT_Y | FLAG_CONST_TILT_Y) == 0 {
                let cur = f32_to_ordered(point.tilt_y);
                write_uleb32(&mut out, zigzag32(cur.wrapping_sub(prev_ty)));
                prev_ty = cur;
            }
            if flags & (FLAG_ZERO_BUTTONS | FLAG_CONST_BUTTONS) == 0 {
                write_uleb32(&mut out, zigzag32(point.buttons.wrapping_sub(prev_btn)));
                prev_btn = point.buttons;
            }
            if flags & FLAG_LINEAR_TIME == 0 {
                let dt = point.timestamp.wrapping_sub(prev_t);
                let d2 = if i == 1 { dt } else { dt.wrapping_sub(prev_dt) };
                write_uleb64(&mut out, zigzag64(d2));
                prev_dt = dt;
                prev_t = point.timestamp;
            }
        }
        let has_nan = points.iter().any(|p| {
            p.x.is_nan()
                || p.y.is_nan()
                || p.pressure.is_nan()
                || p.tilt_x.is_nan()
                || p.tilt_y.is_nan()
        });
        Self {
            len,
            has_nan,
            bytes: out.into_boxed_slice(),
        }
    }

    /// Construct directly from a previously encoded canonical byte slice after
    /// validating its exact length and structure via the shared streaming decoder.
    pub fn from_encoded(len: u32, bytes: &[u8]) -> Option<Self> {
        let count = len as usize;
        if count == 0 {
            return bytes.is_empty().then(Self::default);
        }
        let mut iter = RawIter::from_parts(count, bytes)?;
        let mut decoded = 0;
        let mut has_nan = false;
        while let Some(point) = iter.next() {
            has_nan |= point.x.is_nan()
                || point.y.is_nan()
                || point.pressure.is_nan()
                || point.tilt_x.is_nan()
                || point.tilt_y.is_nan();
            decoded += 1;
        }
        if decoded != count || !iter.reader.is_eof() {
            return None;
        }
        Some(Self {
            len,
            has_nan,
            bytes: bytes.into(),
        })
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.len as usize
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Canonical bit-exact byte slice suitable for hashing and deduplication.
    #[inline]
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Heap bytes retained by the compressed buffer.
    #[inline]
    pub fn byte_len(&self) -> usize {
        self.bytes.len()
    }

    pub fn iter(&self) -> RawIter<'_> {
        RawIter::from_parts(self.len(), &self.bytes).unwrap_or_else(RawIter::empty)
    }

    pub fn first(&self) -> Option<StrokePoint> {
        self.iter().next()
    }

    pub fn get(&self, index: usize) -> Option<StrokePoint> {
        self.iter().nth(index)
    }

    pub fn to_vec(&self) -> Vec<StrokePoint> {
        let mut out = Vec::with_capacity(self.len());
        out.extend(self.iter());
        out
    }
}

pub struct RawIter<'a> {
    reader: ByteReader<'a>,
    index: usize,
    len: usize,
    flags: u8,
    prev_x: f32,
    prev2_x: f32,
    prev_y: f32,
    prev2_y: f32,
    prev_p: f32,
    prev2_p: f32,
    prev_tx: u32,
    prev_ty: u32,
    prev_btn: u32,
    prev_t: u64,
    prev_dt: u64,
    first: Option<StrokePoint>,
}

impl<'a> RawIter<'a> {
    fn empty() -> Self {
        Self {
            reader: ByteReader::new(&[]),
            index: 0,
            len: 0,
            flags: 0,
            prev_x: 0.,
            prev2_x: 0.,
            prev_y: 0.,
            prev2_y: 0.,
            prev_p: 0.,
            prev2_p: 0.,
            prev_tx: 0,
            prev_ty: 0,
            prev_btn: 0,
            prev_t: 0,
            prev_dt: 0,
            first: None,
        }
    }

    fn from_parts(len: usize, data: &'a [u8]) -> Option<Self> {
        if len == 0 {
            return data.is_empty().then(Self::empty);
        }
        let mut reader = ByteReader::new(data);
        let flags = reader.read_u8()?;
        let x = f32::from_bits(reader.read_u32_le()?);
        let y = f32::from_bits(reader.read_u32_le()?);
        let pressure = f32::from_bits(reader.read_u32_le()?);
        let tilt_x = if flags & FLAG_ZERO_TILT_X != 0 {
            0.0_f32
        } else {
            f32::from_bits(reader.read_u32_le()?)
        };
        let tilt_y = if flags & FLAG_ZERO_TILT_Y != 0 {
            0.0_f32
        } else {
            f32::from_bits(reader.read_u32_le()?)
        };
        let buttons = if flags & FLAG_ZERO_BUTTONS != 0 {
            0
        } else {
            reader.read_uleb32()?
        };
        let timestamp = reader.read_uleb64()?;
        let linear_dt = if flags & FLAG_LINEAR_TIME != 0 && len > 1 {
            unzigzag64(reader.read_uleb64()?)
        } else {
            0
        };
        let first = StrokePoint {
            timestamp,
            x,
            y,
            pressure,
            tilt_x,
            tilt_y,
            buttons,
        };
        Some(Self {
            reader,
            index: 0,
            len,
            flags,
            prev_x: x,
            prev2_x: x,
            prev_y: y,
            prev2_y: y,
            prev_p: pressure,
            prev2_p: pressure,
            prev_tx: f32_to_ordered(tilt_x),
            prev_ty: f32_to_ordered(tilt_y),
            prev_btn: buttons,
            prev_t: timestamp,
            prev_dt: linear_dt,
            first: Some(first),
        })
    }
}

impl Iterator for RawIter<'_> {
    type Item = StrokePoint;

    fn next(&mut self) -> Option<Self::Item> {
        if self.index >= self.len {
            return None;
        }
        let i = self.index;
        self.index += 1;
        if i == 0 {
            return self.first.take();
        }
        let px = pred_f32(i, self.prev_x, self.prev2_x);
        let dx = unzigzag32(self.reader.read_uleb32()?);
        let x = f32_from_ordered(f32_to_ordered(px).wrapping_add(dx));
        self.prev2_x = self.prev_x;
        self.prev_x = x;

        let py = pred_f32(i, self.prev_y, self.prev2_y);
        let dy = unzigzag32(self.reader.read_uleb32()?);
        let y = f32_from_ordered(f32_to_ordered(py).wrapping_add(dy));
        self.prev2_y = self.prev_y;
        self.prev_y = y;

        let pressure = if self.flags & FLAG_CONST_PRESSURE != 0 {
            self.prev_p
        } else {
            let pp = pred_f32(i, self.prev_p, self.prev2_p);
            let dp = unzigzag32(self.reader.read_uleb32()?);
            let p = f32_from_ordered(f32_to_ordered(pp).wrapping_add(dp));
            self.prev2_p = self.prev_p;
            self.prev_p = p;
            p
        };

        let tilt_x = if self.flags & (FLAG_ZERO_TILT_X | FLAG_CONST_TILT_X) != 0 {
            f32_from_ordered(self.prev_tx)
        } else {
            let d = unzigzag32(self.reader.read_uleb32()?);
            self.prev_tx = self.prev_tx.wrapping_add(d);
            f32_from_ordered(self.prev_tx)
        };

        let tilt_y = if self.flags & (FLAG_ZERO_TILT_Y | FLAG_CONST_TILT_Y) != 0 {
            f32_from_ordered(self.prev_ty)
        } else {
            let d = unzigzag32(self.reader.read_uleb32()?);
            self.prev_ty = self.prev_ty.wrapping_add(d);
            f32_from_ordered(self.prev_ty)
        };

        let buttons = if self.flags & (FLAG_ZERO_BUTTONS | FLAG_CONST_BUTTONS) != 0 {
            self.prev_btn
        } else {
            let d = unzigzag32(self.reader.read_uleb32()?);
            self.prev_btn = self.prev_btn.wrapping_add(d);
            self.prev_btn
        };

        let timestamp = if self.flags & FLAG_LINEAR_TIME != 0 {
            self.prev_t = self.prev_t.wrapping_add(self.prev_dt);
            self.prev_t
        } else {
            let d2 = unzigzag64(self.reader.read_uleb64()?);
            let dt = if i == 1 {
                d2
            } else {
                self.prev_dt.wrapping_add(d2)
            };
            self.prev_dt = dt;
            self.prev_t = self.prev_t.wrapping_add(dt);
            self.prev_t
        };

        Some(StrokePoint {
            timestamp,
            x,
            y,
            pressure,
            tilt_x,
            tilt_y,
            buttons,
        })
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = self.len.saturating_sub(self.index);
        (remaining, Some(remaining))
    }
}

impl ExactSizeIterator for RawIter<'_> {}

impl PartialEq for RawSamples {
    fn eq(&self, other: &Self) -> bool {
        if self.len != other.len {
            return false;
        }
        if self.bytes == other.bytes {
            return !self.has_nan;
        }
        self.iter().zip(other.iter()).all(|(a, b)| a == b)
    }
}

impl PartialEq<[StrokePoint]> for RawSamples {
    fn eq(&self, other: &[StrokePoint]) -> bool {
        self.len() == other.len() && self.iter().zip(other.iter()).all(|(a, b)| a == *b)
    }
}

impl PartialEq<Vec<StrokePoint>> for RawSamples {
    fn eq(&self, other: &Vec<StrokePoint>) -> bool {
        self == other.as_slice()
    }
}

impl fmt::Debug for RawSamples {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list().entries(self.iter()).finish()
    }
}

impl From<Vec<StrokePoint>> for RawSamples {
    fn from(points: Vec<StrokePoint>) -> Self {
        Self::from_slice(&points)
    }
}

impl From<&[StrokePoint]> for RawSamples {
    fn from(points: &[StrokePoint]) -> Self {
        Self::from_slice(points)
    }
}

impl Serialize for RawSamples {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut seq = serializer.serialize_seq(Some(self.len()))?;
        for point in self.iter() {
            seq.serialize_element(&point)?;
        }
        seq.end()
    }
}

impl<'de> Deserialize<'de> for RawSamples {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct RawVisitor;
        impl<'de> Visitor<'de> for RawVisitor {
            type Value = RawSamples;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a sequence of StrokePoint")
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
                let mut points = Vec::with_capacity(seq.size_hint().unwrap_or(0).min(4096));
                while let Some(point) = seq.next_element::<StrokePoint>()? {
                    points.push(point);
                }
                Ok(RawSamples::from_slice(&points))
            }
        }
        deserializer.deserialize_seq(RawVisitor)
    }
}

/// Lossless, bit-exact compressed buffer of [`PathPoint`] display samples with
/// precomputed local bounding box and maximum brush radius.
#[derive(Clone, Default)]
pub struct PathSamples {
    len: u32,
    max_radius: f32,
    bounds: Rect,
    bytes: Box<[u8]>,
}

impl PathSamples {
    pub fn from_slice(points: &[PathPoint]) -> Self {
        let len = u32::try_from(points.len()).expect("stroke exceeds u32::MAX path points");
        if points.is_empty() {
            return Self::default();
        }
        let mut max_radius = 0.0_f32;
        let bounds = Rect::from_points(points.iter().map(|p| {
            max_radius = max_radius.max(p.radius);
            p.position
        }));

        let first = points[0];
        let mut flags = 0_u8;
        if points
            .iter()
            .all(|p| p.radius.to_bits() == first.radius.to_bits())
        {
            flags |= PATH_FLAG_CONST_RADIUS;
        }
        if points
            .iter()
            .any(|p| p.position.x.is_nan() || p.position.y.is_nan() || p.radius.is_nan())
        {
            flags |= PATH_FLAG_HAS_NAN;
        }
        let mut out = Vec::with_capacity(16 + points.len() * 6);
        out.push(flags);
        write_u32_le(&mut out, first.position.x.to_bits());
        write_u32_le(&mut out, first.position.y.to_bits());
        write_u32_le(&mut out, first.radius.to_bits());

        let mut prev_x = first.position.x;
        let mut prev2_x = first.position.x;
        let mut prev_y = first.position.y;
        let mut prev2_y = first.position.y;
        let mut prev_r = first.radius;
        let mut prev2_r = first.radius;

        for (i, point) in points.iter().enumerate().skip(1) {
            let px = pred_f32(i, prev_x, prev2_x);
            write_uleb32(
                &mut out,
                zigzag32(f32_to_ordered(point.position.x).wrapping_sub(f32_to_ordered(px))),
            );
            prev2_x = prev_x;
            prev_x = point.position.x;

            let py = pred_f32(i, prev_y, prev2_y);
            write_uleb32(
                &mut out,
                zigzag32(f32_to_ordered(point.position.y).wrapping_sub(f32_to_ordered(py))),
            );
            prev2_y = prev_y;
            prev_y = point.position.y;

            if flags & PATH_FLAG_CONST_RADIUS == 0 {
                let pr = pred_f32(i, prev_r, prev2_r);
                write_uleb32(
                    &mut out,
                    zigzag32(f32_to_ordered(point.radius).wrapping_sub(f32_to_ordered(pr))),
                );
                prev2_r = prev_r;
                prev_r = point.radius;
            }
        }
        Self {
            len,
            max_radius,
            bounds,
            bytes: out.into_boxed_slice(),
        }
    }

    /// Construct directly from a previously encoded canonical byte slice after
    /// validating its exact length and recomputing local bounds and max radius.
    pub fn from_encoded(len: u32, bytes: &[u8]) -> Option<Self> {
        let count = len as usize;
        if count == 0 {
            return bytes.is_empty().then(Self::default);
        }
        let flags = *bytes.first()?;
        if flags & !(PATH_FLAG_CONST_RADIUS | PATH_FLAG_HAS_NAN) != 0 {
            return None;
        }
        let mut iter = PathIter::from_parts(count, bytes)?;
        let first = iter.next()?;
        let mut min_x = first.position.x;
        let mut max_x = first.position.x;
        let mut min_y = first.position.y;
        let mut max_y = first.position.y;
        let mut max_radius = 0.0_f32.max(first.radius);
        let mut has_nan = first.position.x.is_nan()
            || first.position.y.is_nan()
            || first.radius.is_nan();
        let mut decoded = 1;
        while let Some(point) = iter.next() {
            min_x = min_x.min(point.position.x);
            max_x = max_x.max(point.position.x);
            min_y = min_y.min(point.position.y);
            max_y = max_y.max(point.position.y);
            max_radius = max_radius.max(point.radius);
            has_nan |= point.position.x.is_nan()
                || point.position.y.is_nan()
                || point.radius.is_nan();
            decoded += 1;
        }
        if decoded != count
            || !iter.reader.is_eof()
            || has_nan != (flags & PATH_FLAG_HAS_NAN != 0)
        {
            return None;
        }
        Some(Self {
            len,
            max_radius,
            bounds: Rect {
                min: Point::new(min_x, min_y),
                max: Point::new(max_x, max_y),
            },
            bytes: bytes.into(),
        })
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.len as usize
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    #[inline]
    pub fn local_bounds(&self) -> Rect {
        self.bounds
    }

    #[inline]
    pub fn max_radius(&self) -> f32 {
        self.max_radius
    }

    /// Canonical bit-exact byte slice suitable for hashing and deduplication.
    #[inline]
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Heap bytes retained by the compressed buffer.
    #[inline]
    pub fn byte_len(&self) -> usize {
        self.bytes.len()
    }

    pub fn iter(&self) -> PathIter<'_> {
        PathIter::from_parts(self.len(), &self.bytes).unwrap_or_else(PathIter::empty)
    }

    pub fn first(&self) -> Option<PathPoint> {
        self.iter().next()
    }

    pub fn get(&self, index: usize) -> Option<PathPoint> {
        self.iter().nth(index)
    }

    /// Decode all points into a caller-owned reusable buffer without reallocating.
    pub fn decode_into(&self, out: &mut Vec<PathPoint>) {
        out.clear();
        out.reserve(self.len());
        out.extend(self.iter());
    }

    pub fn to_vec(&self) -> Vec<PathPoint> {
        let mut out = Vec::with_capacity(self.len());
        out.extend(self.iter());
        out
    }
}

pub struct PathIter<'a> {
    reader: ByteReader<'a>,
    index: usize,
    len: usize,
    flags: u8,
    prev_x: f32,
    prev2_x: f32,
    prev_y: f32,
    prev2_y: f32,
    prev_r: f32,
    prev2_r: f32,
    first: Option<PathPoint>,
}

impl<'a> PathIter<'a> {
    fn empty() -> Self {
        Self {
            reader: ByteReader::new(&[]),
            index: 0,
            len: 0,
            flags: 0,
            prev_x: 0.,
            prev2_x: 0.,
            prev_y: 0.,
            prev2_y: 0.,
            prev_r: 0.,
            prev2_r: 0.,
            first: None,
        }
    }

    fn from_parts(len: usize, data: &'a [u8]) -> Option<Self> {
        if len == 0 {
            return data.is_empty().then(Self::empty);
        }
        let mut reader = ByteReader::new(data);
        let flags = reader.read_u8()?;
        let x = f32::from_bits(reader.read_u32_le()?);
        let y = f32::from_bits(reader.read_u32_le()?);
        let radius = f32::from_bits(reader.read_u32_le()?);
        let first = PathPoint {
            position: Point::new(x, y),
            radius,
        };
        Some(Self {
            reader,
            index: 0,
            len,
            flags,
            prev_x: x,
            prev2_x: x,
            prev_y: y,
            prev2_y: y,
            prev_r: radius,
            prev2_r: radius,
            first: Some(first),
        })
    }
}

impl Iterator for PathIter<'_> {
    type Item = PathPoint;

    fn next(&mut self) -> Option<Self::Item> {
        if self.index >= self.len {
            return None;
        }
        let i = self.index;
        self.index += 1;
        if i == 0 {
            return self.first.take();
        }
        let px = pred_f32(i, self.prev_x, self.prev2_x);
        let dx = unzigzag32(self.reader.read_uleb32()?);
        let x = f32_from_ordered(f32_to_ordered(px).wrapping_add(dx));
        self.prev2_x = self.prev_x;
        self.prev_x = x;

        let py = pred_f32(i, self.prev_y, self.prev2_y);
        let dy = unzigzag32(self.reader.read_uleb32()?);
        let y = f32_from_ordered(f32_to_ordered(py).wrapping_add(dy));
        self.prev2_y = self.prev_y;
        self.prev_y = y;

        let radius = if self.flags & PATH_FLAG_CONST_RADIUS != 0 {
            self.prev_r
        } else {
            let pr = pred_f32(i, self.prev_r, self.prev2_r);
            let dr = unzigzag32(self.reader.read_uleb32()?);
            let r = f32_from_ordered(f32_to_ordered(pr).wrapping_add(dr));
            self.prev2_r = self.prev_r;
            self.prev_r = r;
            r
        };

        Some(PathPoint {
            position: Point::new(x, y),
            radius,
        })
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = self.len.saturating_sub(self.index);
        (remaining, Some(remaining))
    }
}

impl ExactSizeIterator for PathIter<'_> {}

impl PartialEq for PathSamples {
    fn eq(&self, other: &Self) -> bool {
        if self.len != other.len {
            return false;
        }
        if self.bytes == other.bytes {
            return self.bytes.first().is_none_or(|f| f & PATH_FLAG_HAS_NAN == 0);
        }
        self.iter().zip(other.iter()).all(|(a, b)| a == b)
    }
}

impl PartialEq<[PathPoint]> for PathSamples {
    fn eq(&self, other: &[PathPoint]) -> bool {
        self.len() == other.len() && self.iter().zip(other.iter()).all(|(a, b)| a == *b)
    }
}

impl PartialEq<Vec<PathPoint>> for PathSamples {
    fn eq(&self, other: &Vec<PathPoint>) -> bool {
        self == other.as_slice()
    }
}

impl fmt::Debug for PathSamples {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list().entries(self.iter()).finish()
    }
}

impl From<Vec<PathPoint>> for PathSamples {
    fn from(points: Vec<PathPoint>) -> Self {
        Self::from_slice(&points)
    }
}

impl From<&[PathPoint]> for PathSamples {
    fn from(points: &[PathPoint]) -> Self {
        Self::from_slice(points)
    }
}

impl Serialize for PathSamples {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut seq = serializer.serialize_seq(Some(self.len()))?;
        for point in self.iter() {
            seq.serialize_element(&point)?;
        }
        seq.end()
    }
}

impl<'de> Deserialize<'de> for PathSamples {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct PathVisitor;
        impl<'de> Visitor<'de> for PathVisitor {
            type Value = PathSamples;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a sequence of PathPoint")
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
                let mut points = Vec::with_capacity(seq.size_hint().unwrap_or(0).min(4096));
                while let Some(point) = seq.next_element::<PathPoint>()? {
                    points.push(point);
                }
                Ok(PathSamples::from_slice(&points))
            }
        }
        deserializer.deserialize_seq(PathVisitor)
    }
}
