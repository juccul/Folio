//! Compact binary SQLite encoding for ink strokes (`FLS1`), with transparent
//! JSON fallback for legacy rows and non-stroke objects.
use crate::{Error, Result};
use folio_document::*;
use rusqlite::types::{ToSql, ToSqlOutput, Value, ValueRef};
use std::sync::Arc;

const MAGIC: &[u8; 4] = b"FLS1";
const CMD_MAGIC: &[u8; 4] = b"FLC1";
const FLAG_REFINEMENT_ENABLED: u8 = 1 << 0;
const FLAG_HAS_FRAGMENT: u8 = 1 << 1;
const FLAG_HAS_REFINED: u8 = 1 << 2;

pub(crate) enum EncodedObject {
    Blob(Vec<u8>),
    Json(String),
}

impl ToSql for EncodedObject {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        match self {
            Self::Blob(bytes) => Ok(ToSqlOutput::Borrowed(ValueRef::Blob(bytes))),
            Self::Json(text) => Ok(ToSqlOutput::Borrowed(ValueRef::Text(text.as_bytes()))),
        }
    }
}

pub(crate) fn encode_object(object: &Object) -> Result<EncodedObject> {
    let Object::Stroke(stroke) = object else {
        return Ok(EncodedObject::Json(serde_json::to_string(object)?));
    };
    let raw_bytes = stroke.raw.as_bytes();
    let path_bytes = stroke.path.as_bytes();
    let fragment = stroke
        .fragment_path
        .as_deref()
        .map(PathSamples::from_slice);
    let refined = stroke.refined_path.as_deref().map(PathSamples::from_slice);

    let extra = fragment.as_ref().map_or(0, |f| 8 + f.byte_len())
        + refined.as_ref().map_or(0, |r| 8 + r.byte_len());
    let mut out = Vec::with_capacity(89 + raw_bytes.len() + path_bytes.len() + extra);
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(stroke.id.as_bytes());
    out.push(match stroke.style.tool {
        InkTool::Ballpoint => 0,
        InkTool::Fountain => 1,
        InkTool::Pencil => 2,
        InkTool::Marker => 3,
        InkTool::Highlighter => 4,
    });
    out.push(stroke.style.color.r);
    out.push(stroke.style.color.g);
    out.push(stroke.style.color.b);
    for value in [
        stroke.style.width,
        stroke.style.opacity,
        stroke.style.stabilization,
        stroke.style.pressure_gamma,
        stroke.transform.a,
        stroke.transform.b,
        stroke.transform.c,
        stroke.transform.d,
        stroke.transform.tx,
        stroke.transform.ty,
    ] {
        out.extend_from_slice(&value.to_bits().to_le_bytes());
    }
    out.extend_from_slice(&stroke.created_at.to_le_bytes());
    let mut flags = 0_u8;
    if stroke.refinement_enabled {
        flags |= FLAG_REFINEMENT_ENABLED;
    }
    if fragment.is_some() {
        flags |= FLAG_HAS_FRAGMENT;
    }
    if refined.is_some() {
        flags |= FLAG_HAS_REFINED;
    }
    out.push(flags);

    write_bytes_chunk(&mut out, stroke.raw.len(), raw_bytes)?;
    write_bytes_chunk(&mut out, stroke.path.len(), path_bytes)?;
    if let Some(frag) = &fragment {
        write_bytes_chunk(&mut out, frag.len(), frag.as_bytes())?;
    }
    if let Some(refd) = &refined {
        write_bytes_chunk(&mut out, refd.len(), refd.as_bytes())?;
    }
    Ok(EncodedObject::Blob(out))
}

fn write_bytes_chunk(out: &mut Vec<u8>, count: usize, bytes: &[u8]) -> Result<()> {
    let count =
        u32::try_from(count).map_err(|_| Error::Invalid("Stroke sample count overflow".into()))?;
    let len = u32::try_from(bytes.len())
        .map_err(|_| Error::Invalid("Stroke encoded byte length overflow".into()))?;
    out.extend_from_slice(&count.to_le_bytes());
    out.extend_from_slice(&len.to_le_bytes());
    out.extend_from_slice(bytes);
    Ok(())
}

pub(crate) fn decode_object_ref(value: ValueRef<'_>) -> Result<Object> {
    match value {
        ValueRef::Text(bytes) => Ok(serde_json::from_slice(bytes)?),
        ValueRef::Blob(bytes) => decode_stroke_blob(bytes),
        other => Err(Error::Invalid(format!(
            "Unsupported object record type: {:?}",
            other.data_type()
        ))),
    }
}

pub(crate) fn decode_object_value(value: Value) -> Result<Object> {
    match value {
        Value::Text(text) => Ok(serde_json::from_str(&text)?),
        Value::Blob(bytes) => decode_stroke_blob(&bytes),
        other => Err(Error::Invalid(format!(
            "Unsupported object record type: {:?}",
            other.data_type()
        ))),
    }
}

fn read_exact<'a>(data: &'a [u8], pos: &mut usize, len: usize) -> Result<&'a [u8]> {
    let end = pos
        .checked_add(len)
        .ok_or_else(|| Error::Invalid("Corrupt binary stroke record".into()))?;
    let slice = data
        .get(*pos..end)
        .ok_or_else(|| Error::Invalid("Truncated binary stroke record".into()))?;
    *pos = end;
    Ok(slice)
}

fn read_u32(data: &[u8], pos: &mut usize) -> Result<u32> {
    let b = read_exact(data, pos, 4)?;
    Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

fn read_f32(data: &[u8], pos: &mut usize) -> Result<f32> {
    Ok(f32::from_bits(read_u32(data, pos)?))
}

fn read_u64(data: &[u8], pos: &mut usize) -> Result<u64> {
    let b = read_exact(data, pos, 8)?;
    Ok(u64::from_le_bytes([
        b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7],
    ]))
}

fn decode_stroke_blob(data: &[u8]) -> Result<Object> {
    let mut pos = 0;
    let magic = read_exact(data, &mut pos, 4)?;
    if magic != MAGIC {
        return Err(Error::Invalid("Unknown binary object header".into()));
    }
    let id_bytes: [u8; 16] = read_exact(data, &mut pos, 16)?
        .try_into()
        .map_err(|_| Error::Invalid("Invalid stroke ID".into()))?;
    let id = Id::from_bytes(id_bytes);
    let header = read_exact(data, &mut pos, 4)?;
    let tool = match header[0] {
        0 => InkTool::Ballpoint,
        1 => InkTool::Fountain,
        2 => InkTool::Pencil,
        3 => InkTool::Marker,
        4 => InkTool::Highlighter,
        _ => return Err(Error::Invalid("Invalid stroke tool tag".into())),
    };
    let color = Color {
        r: header[1],
        g: header[2],
        b: header[3],
    };
    let style = PenStyle {
        tool,
        color,
        width: read_f32(data, &mut pos)?,
        opacity: read_f32(data, &mut pos)?,
        stabilization: read_f32(data, &mut pos)?,
        pressure_gamma: read_f32(data, &mut pos)?,
    };
    let transform = Transform {
        a: read_f32(data, &mut pos)?,
        b: read_f32(data, &mut pos)?,
        c: read_f32(data, &mut pos)?,
        d: read_f32(data, &mut pos)?,
        tx: read_f32(data, &mut pos)?,
        ty: read_f32(data, &mut pos)?,
    };
    let created_at = read_u64(data, &mut pos)?;
    let flags = read_exact(data, &mut pos, 1)?[0];
    if flags & !(FLAG_REFINEMENT_ENABLED | FLAG_HAS_FRAGMENT | FLAG_HAS_REFINED) != 0 {
        return Err(Error::Invalid("Unknown stroke binary flags".into()));
    }
    let raw_len = read_u32(data, &mut pos)?;
    let raw_byte_len = read_u32(data, &mut pos)? as usize;
    let raw_bytes = read_exact(data, &mut pos, raw_byte_len)?;
    let raw = RawSamples::from_encoded(raw_len, raw_bytes)
        .ok_or_else(|| Error::Invalid("Corrupt binary raw stroke samples".into()))?;

    let path_len = read_u32(data, &mut pos)?;
    let path_byte_len = read_u32(data, &mut pos)? as usize;
    let path_bytes = read_exact(data, &mut pos, path_byte_len)?;
    let path = PathSamples::from_encoded(path_len, path_bytes)
        .ok_or_else(|| Error::Invalid("Corrupt binary stroke path samples".into()))?;

    let fragment_path = if flags & FLAG_HAS_FRAGMENT != 0 {
        let len = read_u32(data, &mut pos)?;
        let byte_len = read_u32(data, &mut pos)? as usize;
        let bytes = read_exact(data, &mut pos, byte_len)?;
        Some(
            PathSamples::from_encoded(len, bytes)
                .ok_or_else(|| Error::Invalid("Corrupt binary fragment path".into()))?
                .to_vec(),
        )
    } else {
        None
    };

    let refined_path = if flags & FLAG_HAS_REFINED != 0 {
        let len = read_u32(data, &mut pos)?;
        let byte_len = read_u32(data, &mut pos)? as usize;
        let bytes = read_exact(data, &mut pos, byte_len)?;
        Some(
            PathSamples::from_encoded(len, bytes)
                .ok_or_else(|| Error::Invalid("Corrupt binary refined path".into()))?
                .to_vec(),
        )
    } else {
        None
    };

    if pos != data.len() {
        return Err(Error::Invalid("Trailing bytes in binary stroke record".into()));
    }

    Ok(Object::Stroke(InkStroke {
        id,
        raw: Arc::new(raw),
        path: Arc::new(path),
        style,
        transform,
        created_at,
        fragment_path,
        refined_path,
        refinement_enabled: flags & FLAG_REFINEMENT_ENABLED != 0,
    }))
}

pub(crate) fn encode_command(command: &Command) -> Result<EncodedObject> {
    let stroke_only = command.changes.iter().all(|change| {
        matches!(
            change,
            Change::Object { before, after, .. }
                if before.as_deref().is_none_or(|o| matches!(o, Object::Stroke(_)))
                    && after.as_deref().is_none_or(|o| matches!(o, Object::Stroke(_)))
        )
    });
    if !stroke_only {
        return Ok(EncodedObject::Json(serde_json::to_string(command)?));
    }
    let mut out = Vec::with_capacity(16 + command.label.len() + command.changes.len() * 64);
    out.extend_from_slice(CMD_MAGIC);
    let label_bytes = command.label.as_bytes();
    let label_len = u32::try_from(label_bytes.len())
        .map_err(|_| Error::Invalid("Binary history label length overflow".into()))?;
    let change_count = u32::try_from(command.changes.len())
        .map_err(|_| Error::Invalid("Binary history change count overflow".into()))?;
    out.extend_from_slice(&label_len.to_le_bytes());
    out.extend_from_slice(label_bytes);
    out.extend_from_slice(&change_count.to_le_bytes());
    for change in &command.changes {
        let Change::Object {
            page,
            id,
            before,
            after,
            index,
        } = change
        else {
            unreachable!();
        };
        out.extend_from_slice(page.as_bytes());
        out.extend_from_slice(id.as_bytes());
        out.extend_from_slice(&(*index as u64).to_le_bytes());
        let mut present = 0_u8;
        if before.is_some() {
            present |= 1;
        }
        if after.is_some() {
            present |= 2;
        }
        out.push(present);
        for object in [before.as_deref(), after.as_deref()].into_iter().flatten() {
            let EncodedObject::Blob(blob) = encode_object(object)? else {
                unreachable!();
            };
            let blob_len = u32::try_from(blob.len())
                .map_err(|_| Error::Invalid("Binary history object length overflow".into()))?;
            out.extend_from_slice(&blob_len.to_le_bytes());
            out.extend_from_slice(&blob);
        }
    }
    Ok(EncodedObject::Blob(out))
}

pub(crate) fn decode_command_ref(value: ValueRef<'_>) -> Result<Command> {
    match value {
        ValueRef::Text(bytes) => Ok(serde_json::from_slice(bytes)?),
        ValueRef::Blob(bytes) => decode_command_blob(bytes),
        other => Err(Error::Invalid(format!(
            "Unsupported history record type: {:?}",
            other.data_type()
        ))),
    }
}

pub(crate) fn decode_command_value(value: Value) -> Result<Command> {
    match value {
        Value::Text(text) => Ok(serde_json::from_str(&text)?),
        Value::Blob(bytes) => decode_command_blob(&bytes),
        other => Err(Error::Invalid(format!(
            "Unsupported history record type: {:?}",
            other.data_type()
        ))),
    }
}

fn decode_command_blob(data: &[u8]) -> Result<Command> {
    let mut pos = 0;
    let magic = read_exact(data, &mut pos, 4)?;
    if magic != CMD_MAGIC {
        return Err(Error::Invalid("Unknown binary history header".into()));
    }
    let label_len = read_u32(data, &mut pos)? as usize;
    let label_bytes = read_exact(data, &mut pos, label_len)?;
    let label = std::str::from_utf8(label_bytes)
        .map_err(|_| Error::Invalid("Invalid UTF-8 in binary history label".into()))?
        .to_owned();
    let count = read_u32(data, &mut pos)? as usize;
    let mut changes = Vec::with_capacity(count.min(4096));
    for _ in 0..count {
        let page = Id::from_bytes(
            read_exact(data, &mut pos, 16)?
                .try_into()
                .map_err(|_| Error::Invalid("Invalid history page ID".into()))?,
        );
        let id = Id::from_bytes(
            read_exact(data, &mut pos, 16)?
                .try_into()
                .map_err(|_| Error::Invalid("Invalid history object ID".into()))?,
        );
        let index = usize::try_from(read_u64(data, &mut pos)?)
            .map_err(|_| Error::Invalid("Binary history index overflow".into()))?;
        let present = read_exact(data, &mut pos, 1)?[0];
        if present & !3 != 0 {
            return Err(Error::Invalid("Invalid binary history presence mask".into()));
        }
        let before = if present & 1 != 0 {
            let len = read_u32(data, &mut pos)? as usize;
            let blob = read_exact(data, &mut pos, len)?;
            Some(Arc::new(decode_stroke_blob(blob)?))
        } else {
            None
        };
        let after = if present & 2 != 0 {
            let len = read_u32(data, &mut pos)? as usize;
            let blob = read_exact(data, &mut pos, len)?;
            Some(Arc::new(decode_stroke_blob(blob)?))
        } else {
            None
        };
        changes.push(Change::Object {
            page,
            id,
            before,
            after,
            index,
        });
    }
    if pos != data.len() {
        return Err(Error::Invalid(
            "Trailing bytes in binary history record".into(),
        ));
    }
    Ok(Command { label, changes })
}
