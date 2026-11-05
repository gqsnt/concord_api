//! The GraphQL layer: named operations as typed variables, the [`GraphQl`] request format, the
//! response envelope ([`GraphqlResponse`]) that keeps `errors` and `extensions.cost` beside
//! whatever `data` arrived, and the tri-state [`Var`] for nullable variables.
//!
//! # Wire contract (pinned `2026-10`, see `PROVIDER.md`)
//!
//! Every operation is a `POST` of `application/json` to `/admin/api/{version}/graphql.json`
//! with the body `{"query": DOCUMENT, "operationName": NAME, "variables": {..}}`. Shopify
//! answers most failures with **HTTP 200** and an `errors` array (codes in
//! `errors[].extensions.code`: `THROTTLED`, `ACCESS_DENIED`, `SHOP_INACTIVE`,
//! `INTERNAL_SERVER_ERROR`, `MAX_COST_EXCEEDED`, ...), possibly *with* partial `data`, and
//! reports the query cost and the leaky-bucket state in `extensions.cost`. Concord's `Json`
//! decodes the whole envelope as the typed output of the generated endpoint: deciding whether an
//! answer with errors is usable is the caller's policy, never this decoder's.

use std::fmt;

use concord::reqwest;
use concord::{CodecError, Encode};
use serde::de::{DeserializeOwned, Deserializer};
use serde::{Deserialize, Serialize, Serializer};

/// Whether an operation reads or writes. Ordinary GraphQL queries are semantic reads even though
/// they are sent with `POST`; a mutation (bulk job creation, cancellation) is a write.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OperationKind {
    /// A `query` document.
    Query,
    /// A `mutation` document.
    Mutation,
}

/// A named GraphQL operation of this SDK: the implementing type is its **variables** object,
/// and the associated items fix its document and its typed `data`.
pub trait Operation: Serialize {
    /// The operation name, sent as `operationName` and declared in [`DOCUMENT`](Self::DOCUMENT).
    const NAME: &'static str;
    /// The GraphQL document (one named operation).
    const DOCUMENT: &'static str;
    /// Query or mutation.
    const KIND: OperationKind;
    /// The type of the response's `data`.
    type Data: DeserializeOwned;
}

/// The request format of the GraphQL endpoint: encodes an [`Operation`]'s variables into the
/// standard GraphQL-over-HTTP JSON body, `application/json`.
///
/// `query` and `operationName` come from the operation's constants; `variables` is the value,
/// serialized by serde_json (strings escaped per JSON, non-ASCII sent as UTF-8, [`Var::Null`]
/// as `null`, [`Var::Absent`] omitted).
pub struct GraphQl(());

#[derive(Serialize)]
struct Body<'a, V> {
    query: &'static str,
    #[serde(rename = "operationName")]
    operation_name: &'static str,
    variables: &'a V,
}

impl<O: Operation> Encode<O> for GraphQl {
    const CONTENT_TYPE: Option<&'static str> = Some("application/json");

    fn encode(value: O) -> Result<reqwest::Body, CodecError> {
        let body = Body {
            query: O::DOCUMENT,
            operation_name: O::NAME,
            variables: &value,
        };
        serde_json::to_vec(&body)
            .map(reqwest::Body::from)
            .map_err(|e| CodecError::new("GraphQL variables could not be encoded").with_source(e))
    }
}

/// A nullable GraphQL variable with its three wire states: omitted from `variables`
/// ([`Absent`](Self::Absent)), explicitly `null` ([`Null`](Self::Null)), or a value.
///
/// GraphQL distinguishes the first two: an omitted variable takes the argument's default (or,
/// for `after`, "the `after` argument is ignored"), an explicit `null` is a value. Fields of this
/// type are written `#[serde(skip_serializing_if = "Var::is_absent")]`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Var<T> {
    /// Not sent.
    #[default]
    Absent,
    /// Sent as `null`.
    Null,
    /// Sent as the value.
    Value(T),
}

impl<T> Var<T> {
    /// Whether the variable is omitted (for `skip_serializing_if`).
    pub fn is_absent(&self) -> bool {
        matches!(self, Self::Absent)
    }

    /// `Value(v)` for `Some(v)`, [`Null`](Self::Null) for `None`.
    pub fn or_null(value: Option<T>) -> Self {
        value.map_or(Self::Null, Self::Value)
    }

    /// `Value(v)` for `Some(v)`, [`Absent`](Self::Absent) for `None`.
    pub fn or_absent(value: Option<T>) -> Self {
        value.map_or(Self::Absent, Self::Value)
    }
}

impl<T: Serialize> Serialize for Var<T> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            // Reached only without `skip_serializing_if`: absent cannot be expressed in place.
            Self::Absent | Self::Null => serializer.serialize_none(),
            Self::Value(value) => value.serialize(serializer),
        }
    }
}

/// `deserialize_with` for a field the document selects and the schema declares nullable: `null`
/// is `None`, a value is `Some`, and an **absent** field is a decode error (serde's default would
/// silently read a missing field as `None`). Selected non-null fields are plain types, so absent
/// or `null` both fail.
///
/// # Errors
///
/// The deserializer's error.
pub fn nullable<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer)
}

/// A nullable field the document selects only on request (`field @include(if: $flag)`), with
/// its three response states: not in the response because the operation did not ask for it
/// ([`NotIncluded`](Self::NotIncluded)), `null` ([`Null`](Self::Null)), or a value.
///
/// Fields of this type are written `#[serde(default)]`: an absent member is `NotIncluded`. This is
/// the one place where absence is not a decode error (see [`nullable`]): the response cannot say
/// whether it was asked for, so a caller that set the flag and still reads `NotIncluded` has an
/// answer that does not match its request.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Included<T> {
    /// Not in the response: the flag was not set.
    #[default]
    NotIncluded,
    /// `null`.
    Null,
    /// The value.
    Value(T),
}

impl<T> Included<T> {
    /// Whether the member was in the response (`null` or a value).
    pub fn is_included(&self) -> bool {
        !matches!(self, Self::NotIncluded)
    }

    /// The value, if the member was included and not `null`.
    pub fn value(&self) -> Option<&T> {
        match self {
            Self::Value(value) => Some(value),
            _ => None,
        }
    }
}

impl<'de, T: Deserialize<'de>> Deserialize<'de> for Included<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(Option::<T>::deserialize(deserializer)?.map_or(Self::Null, Self::Value))
    }
}

/// The `data` member of a response, with the distinction GraphQL makes: absent when the request
/// failed before execution (a parse, validation, cost or throttling error), `null` when an error
/// propagated to the root, otherwise the (possibly partial) data.
#[derive(Clone, Default, PartialEq)]
pub enum Data<D> {
    /// No `data` member.
    #[default]
    Absent,
    /// `"data": null`.
    Null,
    /// The data; fields an error nulled are `None` where the schema allows it.
    Present(D),
}

impl<D> Data<D> {
    /// The data, if present.
    pub fn present(self) -> Option<D> {
        match self {
            Self::Present(data) => Some(data),
            _ => None,
        }
    }

    /// A reference to the data, if present.
    pub fn as_present(&self) -> Option<&D> {
        match self {
            Self::Present(data) => Some(data),
            _ => None,
        }
    }
}

impl<D> fmt::Debug for Data<D> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Absent => "Absent",
            Self::Null => "Null",
            Self::Present(_) => "Present(..)",
        })
    }
}

fn data_member<'de, De, D>(deserializer: De) -> Result<Data<D>, De::Error>
where
    De: Deserializer<'de>,
    D: Deserialize<'de>,
{
    Ok(Option::<D>::deserialize(deserializer)?.map_or(Data::Null, Data::Present))
}

fn null_as_empty<'de, De, T>(deserializer: De) -> Result<Vec<T>, De::Error>
where
    De: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Ok(Option::<Vec<T>>::deserialize(deserializer)?.unwrap_or_default())
}

/// The response of operation `O`: the envelope of its own `Data`. Endpoints declare
/// `-> Answer<ProductsQuery>` so their output follows the operation's `Data` type.
pub type Answer<O> = GraphqlResponse<<O as Operation>::Data>;

/// A GraphQL response: `data`, `errors` and `extensions`, all kept.
///
/// `Debug` prints the shape of `data`, the errors (codes and paths, messages redacted) and the
/// cost — never record contents.
#[derive(Deserialize)]
#[serde(bound = "D: Deserialize<'de>")]
pub struct GraphqlResponse<D> {
    /// The data, absent, `null` or (possibly partially) present.
    #[serde(default, deserialize_with = "data_member")]
    pub data: Data<D>,
    /// The errors, in order (empty when the member is absent or `null`).
    #[serde(default, deserialize_with = "null_as_empty")]
    pub errors: Vec<GraphqlError>,
    /// The extensions; Shopify reports the query cost here.
    #[serde(default)]
    pub extensions: Option<ResponseExtensions>,
}

impl<D> GraphqlResponse<D> {
    /// The cost report, if Shopify sent one.
    pub fn cost(&self) -> Option<&QueryCost> {
        self.extensions.as_ref().and_then(|e| e.cost.as_ref())
    }

    /// Whether any error carries `code == THROTTLED`.
    pub fn is_throttled(&self) -> bool {
        self.errors
            .iter()
            .any(|e| e.code() == Some(&ErrorCode::Throttled))
    }

    /// Whether data is present with errors beside it (a partial answer).
    pub fn is_partial(&self) -> bool {
        matches!(self.data, Data::Present(_)) && !self.errors.is_empty()
    }
}

impl<D> fmt::Debug for GraphqlResponse<D> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GraphqlResponse")
            .field("data", &self.data)
            .field("errors", &self.errors)
            .field("extensions", &self.extensions)
            .finish()
    }
}

/// The response `extensions`: the typed cost report and every other member, kept dynamic.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
pub struct ResponseExtensions {
    /// `extensions.cost`.
    #[serde(default)]
    pub cost: Option<QueryCost>,
    /// Members this SDK does not model.
    #[serde(flatten)]
    pub other: serde_json::Map<String, serde_json::Value>,
}

/// `extensions.cost`: the calculated cost of the query and the app's bucket after it.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueryCost {
    /// Cost calculated before execution from the selection and `first`/`last`.
    pub requested_query_cost: f64,
    /// Cost of the executed query (often lower; the difference is refunded). `null` when the
    /// query did not run (assumption for throttled requests, see `PROVIDER.md`).
    #[serde(deserialize_with = "nullable")]
    pub actual_query_cost: Option<f64>,
    /// The bucket after this request.
    pub throttle_status: ThrottleStatus,
    /// Members this SDK does not model (the `fields` breakdown requested with
    /// `Shopify-GraphQL-Cost-Debug: 1`).
    #[serde(flatten)]
    pub other: serde_json::Map<String, serde_json::Value>,
}

/// `extensions.cost.throttleStatus`: the leaky bucket of this app on this shop.
#[derive(Clone, Copy, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThrottleStatus {
    /// Bucket size in cost points.
    pub maximum_available: f64,
    /// Points available now.
    pub currently_available: f64,
    /// Points restored per second.
    pub restore_rate: f64,
}

impl ThrottleStatus {
    /// Seconds until `cost` points are available at the restore rate (0 when they already are;
    /// `None` when the restore rate is not positive). An estimate for the caller's own backoff
    /// policy; this SDK never waits or retries.
    pub fn seconds_until_available(&self, cost: f64) -> Option<f64> {
        let missing = cost - self.currently_available;
        if missing <= 0.0 {
            return Some(0.0);
        }
        (self.restore_rate > 0.0).then(|| missing / self.restore_rate)
    }
}

/// One entry of `errors`.
///
/// The message is provider text that can quote request values (an id, a search term): `Debug`
/// shows its length only, and [`message`](Self::message) reads it deliberately.
#[derive(Clone, PartialEq, Deserialize)]
pub struct GraphqlError {
    message: String,
    /// Locations in the document.
    #[serde(default)]
    pub locations: Vec<SourceLocation>,
    /// The response path of the failing field (`["inventoryItem", "unitCost"]`); absent for
    /// request-level errors.
    #[serde(default)]
    pub path: Option<Vec<PathSegment>>,
    /// The error's extensions (`code` and dynamic members such as `documentation`, `cost`,
    /// `maxCost`, `requestId`).
    #[serde(default)]
    pub extensions: Option<ErrorExtensions>,
}

impl GraphqlError {
    /// The provider's message. May quote request values: do not log it unreviewed.
    pub fn message(&self) -> &str {
        &self.message
    }

    /// `extensions.code`, if present.
    pub fn code(&self) -> Option<&ErrorCode> {
        self.extensions.as_ref().and_then(|e| e.code.as_ref())
    }

    /// Whether the path starts with these field names (indices are skipped).
    pub fn path_starts_with(&self, fields: &[&str]) -> bool {
        let Some(path) = &self.path else {
            return false;
        };
        let names = path.iter().filter_map(|s| match s {
            PathSegment::Field(name) => Some(name.as_str()),
            PathSegment::Index(_) => None,
        });
        let mut names = names;
        fields.iter().all(|f| names.next() == Some(*f))
    }
}

impl fmt::Debug for GraphqlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GraphqlError")
            .field("code", &self.code())
            .field("path", &self.path)
            .field("message_bytes", &self.message.len())
            .finish_non_exhaustive()
    }
}

impl fmt::Display for GraphqlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.code() {
            Some(code) => write!(f, "GraphQL error {code}")?,
            None => f.write_str("GraphQL error")?,
        }
        if let Some(path) = &self.path {
            f.write_str(" at ")?;
            for (i, segment) in path.iter().enumerate() {
                if i > 0 {
                    f.write_str(".")?;
                }
                match segment {
                    PathSegment::Field(name) => f.write_str(name)?,
                    PathSegment::Index(index) => write!(f, "{index}")?,
                }
            }
        }
        Ok(())
    }
}

/// A `line`/`column` location in the document.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
pub struct SourceLocation {
    /// 1-based line.
    pub line: u32,
    /// 1-based column.
    pub column: u32,
}

/// One segment of an error path: a field name (or alias) or a list index.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(untagged)]
pub enum PathSegment {
    /// A list index.
    Index(u64),
    /// A field name or alias (schema text, not data).
    Field(String),
}

/// `errors[].extensions`.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct ErrorExtensions {
    /// The error code.
    #[serde(default)]
    pub code: Option<ErrorCode>,
    /// Other members, kept dynamic: for `MAX_COST_EXCEEDED`, `cost` is a **number** here (not
    /// the response-level cost object).
    #[serde(flatten)]
    pub other: serde_json::Map<String, serde_json::Value>,
}

/// `errors[].extensions.code`. Unknown codes are kept as [`Other`](Self::Other).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum ErrorCode {
    /// The bucket is empty (like `429`).
    Throttled,
    /// Missing or insufficient access (like `401`/`403`), also per field.
    AccessDenied,
    /// The shop is not active.
    ShopInactive,
    /// A Shopify-side error (in place of a `500`).
    InternalServerError,
    /// The single query cost exceeds the maximum.
    MaxCostExceeded,
    /// Another code.
    Other(String),
}

impl ErrorCode {
    /// The wire spelling.
    pub fn as_str(&self) -> &str {
        match self {
            Self::Throttled => "THROTTLED",
            Self::AccessDenied => "ACCESS_DENIED",
            Self::ShopInactive => "SHOP_INACTIVE",
            Self::InternalServerError => "INTERNAL_SERVER_ERROR",
            Self::MaxCostExceeded => "MAX_COST_EXCEEDED",
            Self::Other(code) => code,
        }
    }
}

impl fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for ErrorCode {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let code = String::deserialize(deserializer)?;
        Ok(match code.as_str() {
            "THROTTLED" => Self::Throttled,
            "ACCESS_DENIED" => Self::AccessDenied,
            "SHOP_INACTIVE" => Self::ShopInactive,
            "INTERNAL_SERVER_ERROR" => Self::InternalServerError,
            "MAX_COST_EXCEEDED" => Self::MaxCostExceeded,
            _ => Self::Other(code),
        })
    }
}

/// A mutation's `userErrors` entry (`UserError`, and `BulkOperationUserError` with its `code`).
#[derive(Clone, PartialEq, Eq, Deserialize)]
pub struct UserError {
    /// The input path of the offending field, if any.
    #[serde(default)]
    pub field: Option<Vec<String>>,
    message: String,
    /// The code, for `BulkOperationUserError` (`INVALID`, `LIMIT_REACHED`,
    /// `OPERATION_IN_PROGRESS`); absent for plain `UserError`.
    #[serde(default)]
    pub code: Option<String>,
}

impl UserError {
    /// The provider's message. May quote input: do not log it unreviewed.
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Debug for UserError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("UserError")
            .field("field", &self.field)
            .field("code", &self.code)
            .field("message_bytes", &self.message.len())
            .finish()
    }
}

/// Escapes `text` as a GraphQL string literal (with its quotes), for the one place a value must
/// be written *inside* a document: the query string of a bulk operation, which takes no
/// variables. `"` and `\` are escaped, control characters become `\uXXXX`, everything else
/// (non-ASCII included) is kept as UTF-8, per the GraphQL `StringValue` grammar.
pub fn string_literal(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 || c == '\u{7f}' => {
                out.push_str(&format!("\\u{:04X}", c as u32));
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}
