//! Conservative smart gestures. Candidates must overlap real objects before
//! the application may act; ordinary letters cannot erase an empty region.
mod encircle;
mod geometry;
mod scratch;
pub use encircle::{closed_loop, encloses, selection_loop};
pub use scratch::{Scratch, scratch};
#[cfg(test)]
mod tests;
