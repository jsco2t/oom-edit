//! Front matter parsing and representation.
//!
//! Parses YAML (`---` blocks) and TOML (`+++` blocks) front matter via
//! `gray_matter`. The parsed data is stored in our own `Value` tree —
//! NOT serde_yaml's or toml's — to avoid API lock-in (architecture §4).
//!
//! Parse failures degrade gracefully: the document still opens, editing
//! is unaffected, and the structural API returns the error (FR-5.6).

use std::collections::BTreeMap;
use std::ops::Range;

use crate::error::FmError;

// ── Value ──────────────────────────────────────────────────────────────────

/// An owned front-matter value tree.
///
/// Minimal owned tree (Str / Num / Bool / Seq / Map) — NOT serde_yaml's
/// `Value`, to avoid API lock-in (architecture §4).
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// A string value.
    Str(String),
    /// A numeric value (integer or float).
    Num(Num),
    /// A boolean value.
    Bool(bool),
    /// An ordered sequence of values.
    Seq(Vec<Value>),
    /// An ordered map of string keys to values.
    Map(BTreeMap<String, Value>),
}

impl Default for Value {
    fn default() -> Self {
        Self::Map(BTreeMap::new())
    }
}

impl Value {
    /// Create a new string value.
    pub fn str(s: String) -> Self {
        Self::Str(s)
    }

    /// Create a new integer value.
    pub fn num(i: i64) -> Self {
        Self::Num(Num::Int(i))
    }

    /// Create a new float value.
    pub fn num_float(f: f64) -> Self {
        Self::Num(Num::Float(f))
    }

    /// Create a new boolean value.
    pub fn bool(b: bool) -> Self {
        Self::Bool(b)
    }

    /// Create a new empty sequence.
    pub fn seq() -> Self {
        Self::Seq(Vec::new())
    }

    /// Create a new empty map.
    pub fn map() -> Self {
        Self::Map(BTreeMap::new())
    }

    /// Check if this value is a map.
    pub fn is_map(&self) -> bool {
        matches!(self, Self::Map(_))
    }

    /// If this value is a map, return a reference to the inner BTreeMap.
    pub fn as_map(&self) -> Option<&BTreeMap<String, Value>> {
        match self {
            Self::Map(m) => Some(m),
            _ => None,
        }
    }

    /// Get a value by key from this map.
    pub fn get(&self, key: &str) -> Option<&Value> {
        match self {
            Self::Map(m) => m.get(key),
            _ => None,
        }
    }
}

/// A numeric value (integer or float).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Num {
    /// An integer value.
    Int(i64),
    /// A floating-point value.
    Float(f64),
}

impl Num {
    /// Try to get this value as an i64. Returns `None` if it's a float.
    pub fn as_int(&self) -> Option<i64> {
        match self {
            Self::Int(i) => Some(*i),
            Self::Float(_) => None,
        }
    }

    /// Try to get this value as an f64.
    pub fn as_float(&self) -> f64 {
        match self {
            Self::Int(i) => *i as f64,
            Self::Float(f) => *f,
        }
    }
}

// ── FrontMatter ────────────────────────────────────────────────────────────

/// The front matter associated with a document, if any.
///
/// Parse failures degrade gracefully — the document still opens (FR-5.6).
#[derive(Debug, PartialEq, Default)]
pub enum FrontMatter {
    /// No front matter present.
    #[default]
    None,
    /// YAML front matter (`---` delimiters).
    Yaml(Result<Value, FmError>),
    /// TOML front matter (`+++` delimiters).
    Toml(Result<Value, FmError>),
}

impl FrontMatter {
    /// Check if front matter is present (not `None`).
    pub fn is_some(&self) -> bool {
        !matches!(self, Self::None)
    }

    /// Check if front matter parsed successfully.
    pub fn is_ok(&self) -> bool {
        match self {
            Self::Yaml(r) | Self::Toml(r) => r.is_ok(),
            Self::None => false,
        }
    }

    /// Get a reference to the parsed value, if present and ok.
    pub fn value(&self) -> Option<&Value> {
        match self {
            Self::Yaml(Ok(v)) | Self::Toml(Ok(v)) => Some(v),
            _ => None,
        }
    }
}

// ── Parsing ────────────────────────────────────────────────────────────────

/// Parse front matter from the beginning of a document.
///
/// If line 1 is exactly `---`, treats it as YAML front matter.
/// If line 1 is exactly `+++`, treats it as TOML front matter.
/// Otherwise returns `FrontMatter::None`.
///
/// Parse failures are captured in `Result` inside the `FrontMatter` variant
/// — they never prevent the document from opening (FR-5.6).
pub fn parse_front_matter(text: &str) -> FrontMatter {
    let first_line = text.lines().next().unwrap_or("");

    if first_line == "---" {
        parse_yaml_front_matter(text)
    } else if first_line == "+++" {
        parse_toml_front_matter(text)
    } else {
        FrontMatter::None
    }
}

/// Return the source span occupied by leading YAML or TOML front matter.
///
/// The span includes both delimiter lines and their terminating newline when
/// present. An unterminated opening delimiter owns the remainder of the file
/// so rendered Markdown never misinterprets metadata as document content.
pub(crate) fn front_matter_span(text: &str) -> Option<Range<usize>> {
    let delimiter = match text.lines().next()? {
        "---" => "---",
        "+++" => "+++",
        _ => return None,
    };
    let first_line_end = text.find('\n').map_or(text.len(), |index| index + 1);
    if first_line_end == text.len() {
        return Some(0..text.len());
    }

    let mut end = first_line_end;
    for line in text[first_line_end..].split_inclusive('\n') {
        end += line.len();
        if line.trim_end_matches(['\r', '\n']) == delimiter {
            return Some(0..end);
        }
    }
    Some(0..text.len())
}

/// Parse YAML front matter from document text.
fn parse_yaml_front_matter(text: &str) -> FrontMatter {
    use gray_matter::engine::YAML;
    use gray_matter::{Matter, Pod};

    let matter = Matter::<YAML>::new();
    match matter.parse::<Pod>(text) {
        Ok(parsed) => {
            let value = opt_pod_to_value(parsed.data.as_ref());
            FrontMatter::Yaml(Ok(value))
        }
        Err(e) => FrontMatter::Yaml(Err(FmError::from_parser(e))),
    }
}

/// Parse TOML front matter from document text.
fn parse_toml_front_matter(text: &str) -> FrontMatter {
    use gray_matter::engine::TOML;
    use gray_matter::{Matter, Pod};

    let mut matter = Matter::<TOML>::new();
    matter.delimiter = "+++".to_string();
    match matter.parse::<Pod>(text) {
        Ok(parsed) => {
            let value = opt_pod_to_value(parsed.data.as_ref());
            FrontMatter::Toml(Ok(value))
        }
        Err(e) => FrontMatter::Toml(Err(FmError::from_parser(e))),
    }
}

/// Convert a gray_matter `Pod` to our `Value` type.
fn pod_to_value(pod: &gray_matter::Pod) -> Value {
    match pod {
        gray_matter::Pod::Null => Value::Map(BTreeMap::new()),
        gray_matter::Pod::String(s) => Value::Str(s.clone()),
        gray_matter::Pod::Integer(i) => Value::Num(Num::Int(*i)),
        gray_matter::Pod::Float(f) => Value::Num(Num::Float(*f)),
        gray_matter::Pod::Boolean(b) => Value::Bool(*b),
        gray_matter::Pod::Array(arr) => Value::Seq(arr.iter().map(pod_to_value).collect()),
        gray_matter::Pod::Hash(hash) => {
            let mut btree = BTreeMap::new();
            for (k, v) in hash {
                btree.insert(k.clone(), pod_to_value(v));
            }
            Value::Map(btree)
        }
    }
}

/// Convert an optional `Pod` to our `Value` type.
fn opt_pod_to_value(pod: Option<&gray_matter::Pod>) -> Value {
    match pod {
        None => Value::Map(BTreeMap::new()),
        Some(p) => pod_to_value(p),
    }
}
