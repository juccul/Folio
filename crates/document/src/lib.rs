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

mod command;
mod geometry;
mod objects;
mod page;
#[cfg(test)]
mod tests;
mod validate;
pub use command::*;
pub use geometry::*;
pub use objects::*;
pub use page::*;

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
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Notebook {
    pub id: Id,
    pub name: String,
    pub parent: Option<Id>,
}
