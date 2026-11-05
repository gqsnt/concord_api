//! Typed identifiers and values: API version, instance URL, access token, record and job ids,
//! object and field API names, opaque locators and bounded numeric parameters.
//!
//! Every type validates on construction (or deserialization) and formats with `Display` exactly
//! as Salesforce expects it on the wire, so the declaration can place it directly. Opaque values
//! (the token, query and result locators) have a redacted `Debug`; their `Display` is the wire
//! value because Concord formats path and query values with `Display` (the declaration marks
//! those values `#[sensitive]`, so Concord's own output withholds them).

use std::fmt;
use std::str::FromStr;

use concord::reqwest::Url;
use serde::{Deserialize, Deserializer, Serialize};

/// Why a value was refused. Static text only: the refused input is never echoed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InvalidValue {
    what: &'static str,
    reason: &'static str,
}

impl InvalidValue {
    pub(crate) const fn new(what: &'static str, reason: &'static str) -> Self {
        Self { what, reason }
    }

    /// The kind of value (`"record id"`, `"API version"`, ...).
    pub fn what(&self) -> &'static str {
        self.what
    }

    /// Why it was refused.
    pub fn reason(&self) -> &'static str {
        self.reason
    }
}

impl fmt::Display for InvalidValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid {}: {}", self.what, self.reason)
    }
}

impl std::error::Error for InvalidValue {}

// ---------------------------------------------------------------------------------------------
// API version
// ---------------------------------------------------------------------------------------------

/// A Salesforce API version (`68.0`), sent in every path as `/services/data/v68.0/`.
///
/// `Display` is `68.0` (without the `v`). Parsing accepts `68.0` and `v68.0`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ApiVersion {
    major: u16,
    minor: u8,
}

impl ApiVersion {
    /// The version this SDK slice is pinned to and tested against: the version shown by the
    /// Salesforce REST and Bulk API 2.0 documentation retrieved on 2026-11-04 (see
    /// `PROVIDER.md`).
    pub const PINNED: ApiVersion = ApiVersion {
        major: 68,
        minor: 0,
    };

    /// The oldest version every operation of this slice exists in: parallel result pages
    /// (`GET jobs/query/{id}/resultPages`) are "available in API version 58.0 and later".
    pub const MINIMUM: ApiVersion = ApiVersion {
        major: 58,
        minor: 0,
    };

    /// `major.minor`.
    pub const fn new(major: u16, minor: u8) -> Self {
        Self { major, minor }
    }

    /// The major part (`68`).
    pub fn major(self) -> u16 {
        self.major
    }

    /// The minor part (`0`).
    pub fn minor(self) -> u8 {
        self.minor
    }

    /// The path segment, `v68.0`.
    pub fn segment(self) -> String {
        format!("v{self}")
    }
}

impl fmt::Display for ApiVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}", self.major, self.minor)
    }
}

impl fmt::Debug for ApiVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ApiVersion({self})")
    }
}

impl FromStr for ApiVersion {
    type Err = InvalidValue;

    fn from_str(text: &str) -> Result<Self, InvalidValue> {
        const WHAT: &str = "API version";
        let text = text.strip_prefix('v').unwrap_or(text);
        let (major, minor) = text
            .split_once('.')
            .ok_or(InvalidValue::new(WHAT, "expected MAJOR.MINOR"))?;
        let digits =
            |s: &str| !s.is_empty() && s.len() <= 3 && s.bytes().all(|b| b.is_ascii_digit());
        if !digits(major) || !digits(minor) {
            return Err(InvalidValue::new(WHAT, "expected MAJOR.MINOR digits"));
        }
        let major: u16 = major
            .parse()
            .map_err(|_| InvalidValue::new(WHAT, "major out of range"))?;
        let minor: u8 = minor
            .parse()
            .map_err(|_| InvalidValue::new(WHAT, "minor out of range"))?;
        if major == 0 {
            return Err(InvalidValue::new(WHAT, "major must not be 0"));
        }
        Ok(Self { major, minor })
    }
}

/// The `apiVersion` of a job document. The Bulk API 2.0 tables type it as a string while every
/// documented example sends a JSON number (`"apiVersion" : 46.0`); both are accepted.
impl<'de> Deserialize<'de> for ApiVersion {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Wire {
            Number(serde_json::Number),
            Text(String),
        }
        let text = match Wire::deserialize(deserializer)? {
            Wire::Number(n) => {
                // `46.0` keeps its text form; an integer `46` means `46.0`.
                let t = n.to_string();
                if t.contains('.') { t } else { format!("{t}.0") }
            }
            Wire::Text(t) => t,
        };
        text.parse().map_err(serde::de::Error::custom)
    }
}

// ---------------------------------------------------------------------------------------------
// Instance URL and token
// ---------------------------------------------------------------------------------------------

/// The org's instance (My Domain) URL, the origin every request goes to and every returned link
/// must stay on: `https://MyDomainName.my.salesforce.com`.
///
/// `https` is required, except for loopback addresses and hosts under the reserved `.test` and
/// `.localhost` names (test fixtures). No user information, path (other than `/`), query or
/// fragment.
#[derive(Clone, PartialEq, Eq)]
pub struct InstanceUrl(Url);

impl InstanceUrl {
    /// Parses and validates an instance URL.
    ///
    /// # Errors
    ///
    /// [`InvalidValue`] for anything else than a bare `https` origin (or a test origin over
    /// `http`).
    pub fn parse(text: &str) -> Result<Self, InvalidValue> {
        const WHAT: &str = "instance URL";
        let url = Url::parse(text).map_err(|_| InvalidValue::new(WHAT, "not a URL"))?;
        let host = url
            .host_str()
            .ok_or(InvalidValue::new(WHAT, "no host"))?
            .to_ascii_lowercase();
        let test_host = host == "localhost"
            || host.ends_with(".localhost")
            || host.ends_with(".test")
            || host == "127.0.0.1"
            || host == "[::1]";
        match url.scheme() {
            "https" => {}
            "http" if test_host => {}
            "http" => return Err(InvalidValue::new(WHAT, "https is required")),
            _ => return Err(InvalidValue::new(WHAT, "unsupported scheme")),
        }
        if !url.username().is_empty() || url.password().is_some() {
            return Err(InvalidValue::new(WHAT, "user information is not allowed"));
        }
        if url.path() != "/" || url.query().is_some() || url.fragment().is_some() {
            return Err(InvalidValue::new(
                WHAT,
                "an instance URL is an origin: no path, query or fragment",
            ));
        }
        Ok(Self(url))
    }

    /// The URL (with its trailing `/`).
    pub fn url(&self) -> &Url {
        &self.0
    }

    /// The URL text, without the trailing `/` (`https://acme.my.salesforce.com`).
    pub fn as_str(&self) -> &str {
        self.0.as_str().trim_end_matches('/')
    }

    /// Whether `other` has the same scheme, host and port.
    pub fn same_origin(&self, other: &Url) -> bool {
        self.0.scheme() == other.scheme()
            && self.0.host_str().map(str::to_ascii_lowercase)
                == other.host_str().map(str::to_ascii_lowercase)
            && self.0.port_or_known_default() == other.port_or_known_default()
    }
}

impl fmt::Debug for InstanceUrl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // The instance origin is not a secret (Concord shows the base URL too).
        write!(f, "InstanceUrl({})", self.as_str())
    }
}

/// An OAuth access token (sent as `Authorization: Bearer ...`). `Debug` is redacted and there is
/// no `Display`.
#[derive(Clone)]
pub struct AccessToken(String);

impl AccessToken {
    /// Wraps a token. An empty token is refused; other validation is the server's.
    ///
    /// # Errors
    ///
    /// [`InvalidValue`] for an empty token or one containing whitespace or control characters
    /// (it would not form a valid header value).
    pub fn new(token: impl Into<String>) -> Result<Self, InvalidValue> {
        let token = token.into();
        if token.is_empty() {
            return Err(InvalidValue::new("access token", "empty"));
        }
        if token
            .bytes()
            .any(|b| b.is_ascii_whitespace() || b.is_ascii_control())
        {
            return Err(InvalidValue::new(
                "access token",
                "whitespace or control characters",
            ));
        }
        Ok(Self(token))
    }

    pub(crate) fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for AccessToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("AccessToken(<redacted>)")
    }
}

// ---------------------------------------------------------------------------------------------
// Ids
// ---------------------------------------------------------------------------------------------

fn salesforce_id(what: &'static str, text: &str) -> Result<(), InvalidValue> {
    if !(text.len() == 15 || text.len() == 18) {
        return Err(InvalidValue::new(
            what,
            "a Salesforce id has 15 or 18 characters",
        ));
    }
    if !text.bytes().all(|b| b.is_ascii_alphanumeric()) {
        return Err(InvalidValue::new(what, "a Salesforce id is alphanumeric"));
    }
    Ok(())
}

macro_rules! id_type {
    ($(#[$doc:meta])* $name:ident, $what:literal) => {
        $(#[$doc])*
        #[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
            /// Validates a 15- or 18-character alphanumeric Salesforce id.
            ///
            /// # Errors
            ///
            /// [`InvalidValue`] for any other text.
            pub fn new(text: impl Into<String>) -> Result<Self, InvalidValue> {
                let text = text.into();
                salesforce_id($what, &text)?;
                Ok(Self(text))
            }

            /// The id text.
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl fmt::Debug for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}({})", stringify!($name), self.0)
            }
        }

        impl FromStr for $name {
            type Err = InvalidValue;
            fn from_str(text: &str) -> Result<Self, InvalidValue> {
                Self::new(text)
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                let text = String::deserialize(deserializer)?;
                Self::new(text).map_err(serde::de::Error::custom)
            }
        }
    };
}

id_type!(
    /// A record id (`001...` Account, `003...` Contact), 15 or 18 characters.
    RecordId,
    "record id"
);
id_type!(
    /// A Bulk API 2.0 query job id (`750...`).
    QueryJobId,
    "query job id"
);
id_type!(
    /// A Bulk API 2.0 ingest job id (`750...`).
    IngestJobId,
    "ingest job id"
);

// ---------------------------------------------------------------------------------------------
// API names
// ---------------------------------------------------------------------------------------------

/// Checks an API name: an ASCII letter, then letters, digits and underscores; no trailing
/// underscore; at most two consecutive underscores (the `__c`/`__r` suffix separator).
fn api_name(what: &'static str, text: &str) -> Result<(), InvalidValue> {
    let bytes = text.as_bytes();
    let Some(first) = bytes.first() else {
        return Err(InvalidValue::new(what, "empty"));
    };
    if !first.is_ascii_alphabetic() {
        return Err(InvalidValue::new(what, "must start with a letter"));
    }
    if text.len() > 120 {
        return Err(InvalidValue::new(what, "too long"));
    }
    if !bytes
        .iter()
        .all(|b| b.is_ascii_alphanumeric() || *b == b'_')
    {
        return Err(InvalidValue::new(
            what,
            "letters, digits and underscores only",
        ));
    }
    if text.ends_with('_') || text.contains("___") {
        return Err(InvalidValue::new(what, "misplaced underscores"));
    }
    Ok(())
}

macro_rules! name_type {
    ($(#[$doc:meta])* $name:ident, $what:literal) => {
        $(#[$doc])*
        #[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
            /// Validates an API name.
            ///
            /// # Errors
            ///
            /// [`InvalidValue`] for text that is not an API name.
            pub fn new(text: impl Into<String>) -> Result<Self, InvalidValue> {
                let text = text.into();
                api_name($what, &text)?;
                Ok(Self(text))
            }

            /// The name.
            pub fn as_str(&self) -> &str {
                &self.0
            }

            /// Whether this is a custom name (`__c` suffix).
            pub fn is_custom(&self) -> bool {
                self.0.ends_with("__c")
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl fmt::Debug for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}({})", stringify!($name), self.0)
            }
        }

        impl FromStr for $name {
            type Err = InvalidValue;
            fn from_str(text: &str) -> Result<Self, InvalidValue> {
                Self::new(text)
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                let text = String::deserialize(deserializer)?;
                Self::new(text).map_err(serde::de::Error::custom)
            }
        }
    };
}

name_type!(
    /// An sObject API name (`Account`, `Contact`, `Invoice__c`).
    SObjectName,
    "sObject name"
);
name_type!(
    /// A field API name (`Name`, `AccountId`, `Loyalty_Tier__c`).
    FieldName,
    "field name"
);

/// A comma-separated field list for `?fields=` (`Id,Name,Industry`). At least one field.
#[derive(Clone, PartialEq, Eq)]
pub struct FieldList(Vec<FieldName>);

impl FieldList {
    /// The list; refused when empty.
    ///
    /// # Errors
    ///
    /// [`InvalidValue`] when `fields` is empty or repeats a name.
    pub fn new(fields: impl IntoIterator<Item = FieldName>) -> Result<Self, InvalidValue> {
        let fields: Vec<FieldName> = fields.into_iter().collect();
        if fields.is_empty() {
            return Err(InvalidValue::new("field list", "empty"));
        }
        for (i, f) in fields.iter().enumerate() {
            if fields[..i]
                .iter()
                .any(|g| g.as_str().eq_ignore_ascii_case(f.as_str()))
            {
                return Err(InvalidValue::new("field list", "repeated field"));
            }
        }
        Ok(Self(fields))
    }

    /// The fields, in order.
    pub fn fields(&self) -> &[FieldName] {
        &self.0
    }
}

impl fmt::Display for FieldList {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (i, field) in self.0.iter().enumerate() {
            if i > 0 {
                f.write_str(",")?;
            }
            f.write_str(field.as_str())?;
        }
        Ok(())
    }
}

impl fmt::Debug for FieldList {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "FieldList({self})")
    }
}

// ---------------------------------------------------------------------------------------------
// Opaque locators
// ---------------------------------------------------------------------------------------------

macro_rules! locator_type {
    ($(#[$doc:meta])* $name:ident, $what:literal, $max:literal, $extra:expr) => {
        $(#[$doc])*
        #[derive(Clone, PartialEq, Eq, Hash)]
        pub struct $name(String);

        impl $name {
            /// Validates the locator text (a conservative character set: an unexpected
            /// character is refused rather than re-encoded).
            ///
            /// # Errors
            ///
            /// [`InvalidValue`] for empty, overlong or unexpected text. The text is not echoed.
            pub fn new(text: impl Into<String>) -> Result<Self, InvalidValue> {
                let text = text.into();
                let extra: &[u8] = $extra;
                if text.is_empty() {
                    return Err(InvalidValue::new($what, "empty"));
                }
                if text.len() > $max {
                    return Err(InvalidValue::new($what, "too long"));
                }
                if !text
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || extra.contains(&b))
                {
                    return Err(InvalidValue::new($what, "unexpected character"));
                }
                Ok(Self(text))
            }

            /// The wire value, **unredacted**.
            pub fn expose(&self) -> &str {
                &self.0
            }
        }

        /// The wire value (Concord formats path and query values with `Display`). The
        /// declaration marks every placement of it `#[sensitive]`.
        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl fmt::Debug for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(concat!(stringify!($name), "(..)"))
            }
        }
    };
}

locator_type!(
    /// A REST query locator, the last segment of `nextRecordsUrl`
    /// (`/services/data/v68.0/query/01gD0000002HU6KIAW-2000`). Opaque: never constructed from
    /// a count or an offset.
    QueryLocator,
    "query locator",
    128,
    b"-"
);
locator_type!(
    /// A Bulk API 2.0 result locator: the `Sforce-Locator` header of a result page, or the
    /// `locator` parameter of a `resultUrl` / result-pages `nextRecordsUrl`. Opaque: "use only
    /// the value from the Sforce-Locator header. Don't try to guess what it is."
    ResultLocator,
    "result locator",
    512,
    b"-_.~="
);

// ---------------------------------------------------------------------------------------------
// Bounded numbers
// ---------------------------------------------------------------------------------------------

/// `Sforce-Query-Options: batchSize=N` for REST queries: 200 to 2,000 (documented; the server may
/// still return fewer).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BatchSize(u16);

impl BatchSize {
    /// Validates 200..=2000.
    ///
    /// # Errors
    ///
    /// [`InvalidValue`] outside the documented range.
    pub fn new(size: u16) -> Result<Self, InvalidValue> {
        if !(200..=2000).contains(&size) {
            return Err(InvalidValue::new(
                "batch size",
                "the documented range is 200 to 2000",
            ));
        }
        Ok(Self(size))
    }

    /// The size.
    pub fn get(self) -> u16 {
        self.0
    }
}

impl fmt::Display for BatchSize {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// `maxRecords` of a Bulk API 2.0 result request: the most records per result set (at least 1).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct MaxRecords(u32);

impl MaxRecords {
    /// At least 1.
    ///
    /// # Errors
    ///
    /// [`InvalidValue`] for 0.
    pub fn new(max: u32) -> Result<Self, InvalidValue> {
        if max == 0 {
            return Err(InvalidValue::new("maxRecords", "must be at least 1"));
        }
        Ok(Self(max))
    }

    /// The value.
    pub fn get(self) -> u32 {
        self.0
    }
}

impl fmt::Display for MaxRecords {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_parse_and_print() {
        assert_eq!("68.0".parse::<ApiVersion>().unwrap(), ApiVersion::PINNED);
        assert_eq!("v58.0".parse::<ApiVersion>().unwrap(), ApiVersion::MINIMUM);
        assert_eq!(ApiVersion::PINNED.segment(), "v68.0");
        assert!("68".parse::<ApiVersion>().is_err());
        assert!("68.0.1".parse::<ApiVersion>().is_err());
        assert!("x.0".parse::<ApiVersion>().is_err());
        let n: ApiVersion = serde_json::from_str("46.0").unwrap();
        assert_eq!(n, ApiVersion::new(46, 0));
        let s: ApiVersion = serde_json::from_str("\"68.0\"").unwrap();
        assert_eq!(s, ApiVersion::PINNED);
    }

    #[test]
    fn instance_urls() {
        assert!(InstanceUrl::parse("https://acme.my.salesforce.com").is_ok());
        assert!(InstanceUrl::parse("http://org-a.my.salesforce.test:8080").is_ok());
        assert!(InstanceUrl::parse("http://acme.my.salesforce.com").is_err());
        assert!(InstanceUrl::parse("https://u:p@acme.my.salesforce.com").is_err());
        assert!(InstanceUrl::parse("https://acme.my.salesforce.com/services").is_err());
        assert!(InstanceUrl::parse("https://acme.my.salesforce.com/?x=1").is_err());
    }

    #[test]
    fn ids_and_names() {
        assert!(RecordId::new("001R0000005hDFYIA2").is_ok());
        assert!(RecordId::new("001R0000005hDFY").is_ok());
        assert!(RecordId::new("001R0000005hDF").is_err());
        assert!(RecordId::new("001R0000005hDFY/A2").is_err());
        assert!(FieldName::new("Loyalty_Tier__c").unwrap().is_custom());
        assert!(FieldName::new("Name").is_ok());
        assert!(FieldName::new("1Name").is_err());
        assert!(FieldName::new("Name'--").is_err());
        assert!(FieldName::new("Bad___c").is_err());
        assert!(FieldList::new([]).is_err());
        let list = FieldList::new([
            FieldName::new("Id").unwrap(),
            FieldName::new("Name").unwrap(),
        ])
        .unwrap();
        assert_eq!(list.to_string(), "Id,Name");
    }

    #[test]
    fn locators_are_redacted_in_debug() {
        let q = QueryLocator::new("01gD0000002HU6KIAW-2000").unwrap();
        assert_eq!(format!("{q:?}"), "QueryLocator(..)");
        assert_eq!(q.to_string(), "01gD0000002HU6KIAW-2000");
        assert!(QueryLocator::new("a/b").is_err());
        assert!(ResultLocator::new("MTAwMDA").is_ok());
        assert!(ResultLocator::new("a b").is_err());
        assert!(ResultLocator::new("a%2F").is_err());
        let token = AccessToken::new("00Dsecret!token").unwrap();
        assert!(!format!("{token:?}").contains("secret"));
    }
}
