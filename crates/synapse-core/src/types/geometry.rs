use std::{borrow::Cow, fmt, str::FromStr};

use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::{Deserialize, Serialize};

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Backend {
    Software,
    Vigem,
    Hardware,
    Auto,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PerceptionMode {
    A11yOnly,
    PixelOnly,
    Hybrid,
    Auto,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Point {
    pub x: i32,
    pub y: i32,
}

impl Point {
    #[must_use]
    pub fn distance_to(self, other: Self) -> f64 {
        let dx = f64::from(self.x) - f64::from(other.x);
        let dy = f64::from(self.y) - f64::from(other.y);
        dx.hypot(dy)
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl Rect {
    /// Returns true when a point is inside this rectangle.
    ///
    /// The right and bottom edges are exclusive. Non-positive width or height
    /// rectangles are empty.
    #[must_use]
    pub const fn contains(self, point: Point) -> bool {
        if self.w <= 0 || self.h <= 0 {
            return false;
        }

        let right = self.x.saturating_add(self.w);
        let bottom = self.y.saturating_add(self.h);
        point.x >= self.x && point.x < right && point.y >= self.y && point.y < bottom
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Size {
    pub w: u32,
    pub h: u32,
}

pub type SessionId = String;
const ELEMENT_ID_SCHEMA_PATTERN: &str = r"^-?0x[0-9a-fA-F]+:[0-9a-fA-F]+$";

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct ElementId(String);

impl ElementId {
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Parses and validates a public UIA element identifier.
    ///
    /// # Errors
    ///
    /// Returns an error when the identifier is not shaped as
    /// `<hwnd_hex>:<runtime_id_hex>`.
    pub fn parse(value: &str) -> Result<Self, ElementIdParseError> {
        value.parse()
    }

    /// Splits a validated element identifier into its HWND and UIA runtime id.
    ///
    /// # Errors
    ///
    /// Returns an error when this value was constructed from a non-canonical
    /// string that cannot be parsed as an element identifier.
    pub fn parts(&self) -> Result<ElementIdParts, ElementIdParseError> {
        parse_element_id_parts(&self.0)
    }
}

impl fmt::Display for ElementId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl From<ElementId> for String {
    fn from(value: ElementId) -> Self {
        value.0
    }
}

impl From<&ElementId> for String {
    fn from(value: &ElementId) -> Self {
        value.0.clone()
    }
}

impl TryFrom<String> for ElementId {
    type Error = ElementIdParseError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        parse_element_id_parts(&value)?;
        Ok(Self(value))
    }
}

impl FromStr for ElementId {
    type Err = ElementIdParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        parse_element_id_parts(value)?;
        Ok(Self(value.to_owned()))
    }
}

impl JsonSchema for ElementId {
    fn schema_name() -> Cow<'static, str> {
        "ElementId".into()
    }

    fn json_schema(_generator: &mut SchemaGenerator) -> Schema {
        json_schema!({
            "type": "string",
            "pattern": ELEMENT_ID_SCHEMA_PATTERN,
        })
    }
}

impl PartialEq<&str> for ElementId {
    fn eq(&self, other: &&str) -> bool {
        self.0 == *other
    }
}

impl PartialEq<ElementId> for &str {
    fn eq(&self, other: &ElementId) -> bool {
        *self == other.0
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ElementIdParts {
    pub hwnd: i64,
    pub runtime_id_hex: String,
}

#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum ElementIdParseError {
    #[error("element id must be shaped as '<hwnd_hex>:<runtime_id_hex>'")]
    MissingSeparator,
    #[error("element id hwnd must be hex with a 0x prefix")]
    InvalidHwnd,
    #[error("element id runtime id must be non-empty hex")]
    InvalidRuntimeId,
}

pub type EntityId = String;
pub type ReflexId = String;
pub type SubscriptionId = String;
pub type ProfileId = String;

#[must_use]
pub fn new_session_id() -> SessionId {
    uuid::Uuid::now_v7().to_string()
}

#[must_use]
pub fn new_reflex_id() -> ReflexId {
    uuid::Uuid::now_v7().to_string()
}

#[must_use]
pub fn new_subscription_id() -> SubscriptionId {
    uuid::Uuid::now_v7().to_string()
}

#[must_use]
pub fn element_id(hwnd: i64, runtime_id_hex: &str) -> ElementId {
    let hwnd_hex = if hwnd.is_negative() {
        format!("-0x{:x}", hwnd.unsigned_abs())
    } else {
        format!("0x{hwnd:x}")
    };
    ElementId(format!("{hwnd_hex}:{runtime_id_hex}"))
}

#[must_use]
pub fn entity_id(track: u64) -> EntityId {
    format!("track:{track}")
}

fn parse_element_id_parts(value: &str) -> Result<ElementIdParts, ElementIdParseError> {
    let (hwnd_raw, runtime_id_hex) = value
        .split_once(':')
        .ok_or(ElementIdParseError::MissingSeparator)?;
    let hwnd = parse_hwnd_hex(hwnd_raw)?;

    if runtime_id_hex.is_empty() || !runtime_id_hex.chars().all(|item| item.is_ascii_hexdigit()) {
        return Err(ElementIdParseError::InvalidRuntimeId);
    }

    Ok(ElementIdParts {
        hwnd,
        runtime_id_hex: runtime_id_hex.to_owned(),
    })
}

fn parse_hwnd_hex(value: &str) -> Result<i64, ElementIdParseError> {
    if let Some(hex) = value.strip_prefix("0x") {
        return i64::from_str_radix(hex, 16).map_err(|_err| ElementIdParseError::InvalidHwnd);
    }

    if let Some(hex) = value.strip_prefix("-0x") {
        let hwnd = i64::from_str_radix(hex, 16).map_err(|_err| ElementIdParseError::InvalidHwnd)?;
        return Ok(-hwnd);
    }

    Err(ElementIdParseError::InvalidHwnd)
}
