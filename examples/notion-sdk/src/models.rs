//! Narrow typed payloads. Unknown discriminators retain the entire JSON object.
//! Malformed payloads of a supported type fail decoding instead of becoming unknowns.
//!
//! New property and block discriminators need no registration to be retained as `Unknown`.
//! Common identity fields (and a block's `has_children`) must still decode. Unknown blocks
//! remain ordinary items in provider order and can have separately retrieved children.

use serde::{
    Deserialize, Deserializer,
    de::{DeserializeOwned, Error as _},
};
use serde_json::Value;
use std::collections::BTreeMap;

fn field<T: DeserializeOwned>(raw: &Value, name: &str) -> Result<T, String> {
    let value = raw
        .get(name)
        .ok_or_else(|| format!("missing field {name}"))?;
    // Do not put response content in SDK decoding diagnostics.
    serde_json::from_value(value.clone()).map_err(|_| format!("invalid field {name}"))
}

fn object<'de, D: Deserializer<'de>>(d: D, expected: &str) -> Result<String, D::Error> {
    let value = String::deserialize(d)?;
    if value == expected {
        Ok(value)
    } else {
        Err(D::Error::custom("unexpected resource object"))
    }
}
fn page_object<'de, D: Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    object(d, "page")
}
fn list_object<'de, D: Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    object(d, "list")
}

/// One standard Notion list response. Pagination invariants are checked by the client.
#[derive(Clone, Debug, Deserialize)]
pub struct List<T> {
    /// Always `list`.
    #[serde(deserialize_with = "list_object")]
    pub object: String,
    /// Results in provider order.
    pub results: Vec<T>,
    /// Opaque cursor, or null at exhaustion.
    pub next_cursor: Option<String>,
    /// Whether another page exists, including after an empty result page.
    pub has_more: bool,
    /// Query completeness signal, when supplied by Notion.
    pub request_status: Option<RequestStatus>,
}

/// Query completeness; unfamiliar statuses are retained and refused as complete outcomes.
#[derive(Clone, Debug, Deserialize)]
pub struct RequestStatus {
    /// `complete` or `incomplete` in the pinned API.
    #[serde(rename = "type")]
    pub kind: String,
    /// Provider reason for an incomplete query, such as `query_result_limit_reached`.
    pub incomplete_reason: Option<String>,
}

/// A page's properties. Content is available only through block children.
/// Even a successful response need not contain every paginated property value.
#[derive(Clone, Debug, Deserialize)]
pub struct Page {
    /// Always `page`.
    #[serde(deserialize_with = "page_object")]
    pub object: String,
    /// Page identifier.
    pub id: String,
    /// Present property values, keyed by property name.
    pub properties: BTreeMap<String, Property>,
    /// Page location when returned by the API.
    pub url: Option<String>,
}

/// Query results can include data sources in a wiki, as well as pages.
#[derive(Clone, Debug)]
pub enum QueryResult {
    /// A page with the supported typed property subset.
    Page(Page),
    /// A non-page result, retained for diagnostics without claiming to model it.
    Other {
        /// Resource discriminator.
        object: String,
        /// Full JSON object.
        raw: Value,
    },
}

impl QueryResult {
    /// Borrows a page result, if this result is a page.
    pub fn as_page(&self) -> Option<&Page> {
        match self {
            Self::Page(page) => Some(page),
            Self::Other { .. } => None,
        }
    }
}

impl<'de> Deserialize<'de> for QueryResult {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let raw = Value::deserialize(d)?;
        let object: String = field(&raw, "object").map_err(D::Error::custom)?;
        if object == "page" {
            serde_json::from_value(raw)
                .map(Self::Page)
                .map_err(|_| D::Error::custom("invalid page result"))
        } else {
            Ok(Self::Other { object, raw })
        }
    }
}

/// A property ID and its typed value.
#[derive(Clone, Debug)]
pub struct Property {
    /// Stable provider ID, including any provider URL encoding, preserved unchanged.
    pub id: String,
    /// Supported value or retained unknown payload.
    pub value: PropertyValue,
}

/// The supported page property subset.
#[derive(Clone, Debug)]
pub enum PropertyValue {
    /// Title rich-text fragments.
    Title(Vec<RichText>),
    /// Rich-text property fragments.
    RichText(Vec<RichText>),
    /// Number, including an unset null value.
    Number(Option<f64>),
    /// Checkbox value.
    Checkbox(bool),
    /// Selected option, including an unset null value.
    Select(Option<SelectOption>),
    /// Date range, including an unset null value.
    Date(Option<DateValue>),
    /// An unsupported or future property type. Full payload includes pagination flags.
    Unknown {
        /// Provider discriminator.
        kind: String,
        /// Entire original property object.
        raw: Value,
    },
}

/// A select option returned by the API.
#[derive(Clone, Debug, Deserialize)]
pub struct SelectOption {
    /// Stable option ID.
    pub id: String,
    /// Display name.
    pub name: String,
    /// Provider color name (open vocabulary).
    pub color: String,
}

/// ISO 8601 date values, kept as strings without timezone conversion.
#[derive(Clone, Debug, Deserialize)]
pub struct DateValue {
    /// Start date or timestamp.
    pub start: String,
    /// Optional end date or timestamp.
    pub end: Option<String>,
    /// Optional provider timezone.
    pub time_zone: Option<String>,
}

impl<'de> Deserialize<'de> for Property {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let raw = Value::deserialize(d)?;
        let id = field(&raw, "id").map_err(D::Error::custom)?;
        let kind: String = field(&raw, "type").map_err(D::Error::custom)?;
        let value = match kind.as_str() {
            "title" => PropertyValue::Title(field(&raw, "title").map_err(D::Error::custom)?),
            "rich_text" => {
                PropertyValue::RichText(field(&raw, "rich_text").map_err(D::Error::custom)?)
            }
            "number" => PropertyValue::Number(field(&raw, "number").map_err(D::Error::custom)?),
            "checkbox" => {
                PropertyValue::Checkbox(field(&raw, "checkbox").map_err(D::Error::custom)?)
            }
            "select" => PropertyValue::Select(field(&raw, "select").map_err(D::Error::custom)?),
            "date" => PropertyValue::Date(field(&raw, "date").map_err(D::Error::custom)?),
            _ => PropertyValue::Unknown { kind, raw },
        };
        Ok(Self { id, value })
    }
}

/// Rich text rendered as plain text: text and equations are supported, styling is omitted.
#[derive(Clone, Debug)]
pub enum RichText {
    /// Text content and optional hyperlink.
    Text(Text),
    /// Equation expression, without typesetting.
    Equation(Equation),
    /// Mentions and all other unimplemented/future types, kept intact.
    Unknown {
        /// Provider discriminator.
        kind: String,
        /// Entire original rich-text object, including plain_text and annotations.
        raw: Value,
    },
}

/// Text rich-text payload.
#[derive(Clone, Debug, Deserialize)]
pub struct Text {
    /// Literal text.
    pub content: String,
    /// Hyperlink, retained but not emitted by the plain-text renderer.
    pub link: Option<TextLink>,
}
/// A rich-text hyperlink.
#[derive(Clone, Debug, Deserialize)]
pub struct TextLink {
    /// Destination URL; the SDK never follows it.
    pub url: String,
}
/// Equation rich-text payload.
#[derive(Clone, Debug, Deserialize)]
pub struct Equation {
    /// LaTeX expression.
    pub expression: String,
}

impl<'de> Deserialize<'de> for RichText {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let raw = Value::deserialize(d)?;
        let kind: String = field(&raw, "type").map_err(D::Error::custom)?;
        match kind.as_str() {
            "text" => field(&raw, "text")
                .map(Self::Text)
                .map_err(D::Error::custom),
            "equation" => field(&raw, "equation")
                .map(Self::Equation)
                .map_err(D::Error::custom),
            _ => Ok(Self::Unknown { kind, raw }),
        }
    }
}

/// A block at one level of the tree. `has_children` requires separate requests.
#[derive(Clone, Debug)]
pub struct Block {
    /// Block identifier.
    pub id: String,
    /// Whether this block has children to retrieve independently.
    pub has_children: bool,
    /// Typed subset or retained unknown payload.
    pub value: BlockValue,
}

/// Supported block payloads.
#[derive(Clone, Debug)]
pub enum BlockValue {
    /// Paragraph text.
    Paragraph(TextBlock),
    /// Level-one heading.
    Heading1(TextBlock),
    /// Level-two heading.
    Heading2(TextBlock),
    /// Level-three heading.
    Heading3(TextBlock),
    /// Bulleted list item.
    BulletedListItem(TextBlock),
    /// Checkbox item.
    ToDo(ToDoBlock),
    /// Horizontal divider.
    Divider,
    /// All unimplemented and future block types.
    Unknown {
        /// Provider discriminator.
        kind: String,
        /// Entire original block, including identity and has_children.
        raw: Value,
    },
}

/// The rich text shared by paragraph, heading and list payloads.
#[derive(Clone, Debug, Deserialize)]
pub struct TextBlock {
    /// Rich-text fragments in order.
    pub rich_text: Vec<RichText>,
}
/// A to-do block's payload.
#[derive(Clone, Debug, Deserialize)]
pub struct ToDoBlock {
    /// Rich-text fragments in order.
    pub rich_text: Vec<RichText>,
    /// Checkbox state.
    pub checked: bool,
}

impl<'de> Deserialize<'de> for Block {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let raw = Value::deserialize(d)?;
        let obj: String = field(&raw, "object").map_err(D::Error::custom)?;
        if obj != "block" {
            return Err(D::Error::custom("expected block object"));
        }
        let id = field(&raw, "id").map_err(D::Error::custom)?;
        let has_children = field(&raw, "has_children").map_err(D::Error::custom)?;
        let kind: String = field(&raw, "type").map_err(D::Error::custom)?;
        let value = match kind.as_str() {
            "paragraph" => BlockValue::Paragraph(field(&raw, &kind).map_err(D::Error::custom)?),
            "heading_1" => BlockValue::Heading1(field(&raw, &kind).map_err(D::Error::custom)?),
            "heading_2" => BlockValue::Heading2(field(&raw, &kind).map_err(D::Error::custom)?),
            "heading_3" => BlockValue::Heading3(field(&raw, &kind).map_err(D::Error::custom)?),
            "bulleted_list_item" => {
                BlockValue::BulletedListItem(field(&raw, &kind).map_err(D::Error::custom)?)
            }
            "to_do" => BlockValue::ToDo(field(&raw, &kind).map_err(D::Error::custom)?),
            "divider" => {
                let _: BTreeMap<String, Value> =
                    field(&raw, "divider").map_err(D::Error::custom)?;
                BlockValue::Divider
            }
            _ => BlockValue::Unknown { kind, raw },
        };
        Ok(Self {
            id,
            has_children,
            value,
        })
    }
}
