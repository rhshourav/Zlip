#![forbid(unsafe_code)]
//! Zlip core: everything about clipboard *content* that has no OS dependency.
//!
//! Because nothing here touches Win32, CI proves this crate on Linux, and the
//! same logic can later back a macOS or Linux build.

pub mod categorize;
pub mod geometry;
pub mod image;
pub mod item;
pub mod placeholder;
pub mod queue;
pub mod search;
pub mod transform;

pub use categorize::{Classification, Rgba, classify, parse_color};
pub use item::{Category, ImageData, ItemId, Payload};
pub use placeholder::{Context as PlaceholderContext, Expanded, expand_placeholders};
pub use queue::{PasteQueue, QueueMode};
pub use search::{Candidate, Searcher};
pub use transform::{Transform, TransformError, apply_transforms};
