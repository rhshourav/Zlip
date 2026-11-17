use serde::{Deserialize, Serialize};
use std::fmt;

/// Stable identifier of a history item or vault entry.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize,
)]
#[serde(transparent)]
pub struct ItemId(pub u64);

impl fmt::Display for ItemId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// What a clipboard item *is*. The set matches the UI's category rail and the
/// pastel palette one-to-one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum Category {
    Text,
    Link,
    Code,
    Image,
    Color,
    File,
    Snippet,
    Sensitive,
}

impl Category {
    pub const ALL: [Self; 8] = [
        Self::Text,
        Self::Link,
        Self::Code,
        Self::Image,
        Self::Color,
        Self::File,
        Self::Snippet,
        Self::Sensitive,
    ];

    /// Stable wire / database name.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Link => "link",
            Self::Code => "code",
            Self::Image => "image",
            Self::Color => "color",
            Self::File => "file",
            Self::Snippet => "snippet",
            Self::Sensitive => "sensitive",
        }
    }

    /// Parse the stable name back. Unknown names return `None`.
    #[must_use]
    pub fn from_name(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|c| c.as_str() == s)
    }

    /// Default pastel from the design system (blueprint §1.3).
    #[must_use]
    pub fn default_hex(self) -> &'static str {
        match self {
            Self::Text => "#C5CAF5",
            Self::Link => "#A9D6F5",
            Self::Code => "#D6C4F5",
            Self::Image => "#FAD0B8",
            Self::Color => "#F8C4D8",
            Self::File => "#F7E3A6",
            Self::Snippet => "#BDEBD0",
            Self::Sensitive => "#F5B5AD",
        }
    }
}

/// A decoded PNG plus its dimensions (so lists never need to decode it).
#[derive(Clone, PartialEq, Eq)]
pub struct ImageData {
    pub png: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

impl fmt::Debug for ImageData {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Never dump pixel bytes into logs.
        write!(
            f,
            "ImageData({}x{}, {} bytes)",
            self.width,
            self.height,
            self.png.len()
        )
    }
}

/// The content of one clipboard capture.
#[derive(Clone, PartialEq, Eq)]
pub enum Payload {
    Text(String),
    /// Absolute paths from a file-drop (`CF_HDROP`).
    Files(Vec<String>),
    Image(ImageData),
}

impl fmt::Debug for Payload {
    // Redacted on purpose: a `{:?}` in a log line must never leak clipboard text.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Text(t) => write!(f, "Text({} bytes)", t.len()),
            Self::Files(v) => write!(f, "Files({})", v.len()),
            Self::Image(i) => i.fmt(f),
        }
    }
}

impl Payload {
    /// Approximate size in bytes, used for retention budgets.
    #[must_use]
    pub fn size_bytes(&self) -> usize {
        match self {
            Self::Text(t) => t.len(),
            Self::Files(v) => v.iter().map(String::len).sum(),
            Self::Image(i) => i.png.len(),
        }
    }

    /// Text form used for search and "paste as text": files become one path per line.
    #[must_use]
    pub fn as_text(&self) -> Option<String> {
        match self {
            Self::Text(t) => Some(t.clone()),
            Self::Files(v) => Some(v.join("\n")),
            Self::Image(_) => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn category_names_round_trip() {
        for c in Category::ALL {
            assert_eq!(Category::from_name(c.as_str()), Some(c));
        }
        assert_eq!(Category::from_name("nope"), None);
    }

    #[test]
    fn debug_output_never_contains_content() {
        let p = Payload::Text("hunter2-super-secret".into());
        assert!(!format!("{p:?}").contains("hunter2"));
        let f = Payload::Files(vec!["C:/secret/plan.docx".into()]);
        assert!(!format!("{f:?}").contains("plan"));
    }

    #[test]
    fn default_palette_is_valid_hex() {
        for c in Category::ALL {
            assert!(crate::parse_color(c.default_hex()).is_some(), "{c:?}");
        }
    }
}
