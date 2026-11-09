//! Context values and GraphQL scalars of the pinned schema: the API version, the shop, the access
//! token, opaque cursors, signed result URLs, global ids, `Decimal`/`Money`, `DateTime` and
//! `UnsignedInt64`.
//!
//! Diagnostics: the access token, cursors and signed URLs never appear in `Debug` (there is no
//! `Display` for them); global ids, shop handles and versions do, since they identify objects
//! rather than authorize anything.

use std::fmt;
use std::marker::PhantomData;
use std::str::FromStr;

use concord::reqwest::Url;
use serde::de::{self, Deserializer, Visitor};
use serde::{Deserialize, Serialize, Serializer};

/// A Shopify Admin API version (`YYYY-MM`, a quarterly release). The SDK pins
/// [`ApiVersion::PINNED`] and sends it in the request path; a response served under another
/// version (Shopify "falls forward" from a retired version) is refused by
/// [`ShopContext::send`](crate::ShopContext::send).
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ApiVersion {
    year: u16,
    month: u8,
}

impl ApiVersion {
    /// `2026-10`, the latest stable version on 4 November 2026 (see `PROVIDER.md`).
    pub const V2026_10: Self = Self {
        year: 2026,
        month: 10,
    };

    /// The version this SDK slice implements and tests.
    pub const PINNED: Self = Self::V2026_10;

    /// A version from its parts; Shopify releases in January, April, July and October.
    ///
    /// # Errors
    ///
    /// [`InvalidValue`] for a month other than 1, 4, 7 or 10, or a year outside 2000–9999.
    pub fn new(year: u16, month: u8) -> Result<Self, InvalidValue> {
        if !(2000..=9999).contains(&year) || !matches!(month, 1 | 4 | 7 | 10) {
            return Err(InvalidValue("API version"));
        }
        Ok(Self { year, month })
    }

    /// The year.
    pub fn year(self) -> u16 {
        self.year
    }

    /// The month (1, 4, 7 or 10).
    pub fn month(self) -> u8 {
        self.month
    }
}

impl FromStr for ApiVersion {
    type Err = InvalidValue;

    /// Parses `YYYY-MM` exactly (`2026-10`).
    fn from_str(s: &str) -> Result<Self, InvalidValue> {
        let invalid = InvalidValue("API version");
        let (year, month) = s.split_once('-').ok_or(invalid)?;
        if year.len() != 4
            || month.len() != 2
            || !(year.bytes().chain(month.bytes())).all(|b| b.is_ascii_digit())
        {
            return Err(invalid);
        }
        Self::new(
            year.parse().map_err(|_| invalid)?,
            month.parse().map_err(|_| invalid)?,
        )
    }
}

impl fmt::Display for ApiVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:04}-{:02}", self.year, self.month)
    }
}

impl fmt::Debug for ApiVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ApiVersion({self})")
    }
}

/// A value refused before any request exists. Names the kind of value, never the value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InvalidValue(pub &'static str);

impl fmt::Display for InvalidValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid {}", self.0)
    }
}

impl std::error::Error for InvalidValue {}

/// A shop's permanent `myshopify.com` handle (`acme` for `acme.myshopify.com`), the first label
/// of the Admin API host `https://{shop}.myshopify.com`.
///
/// `Display` is the handle: it is the declaration's `subdomain "{shop}"` value.
#[derive(Clone, PartialEq, Eq, Hash, concord::Debug)]
pub struct ShopDomain(String);

impl ShopDomain {
    /// The shop of `handle` (`acme`) or of its full domain (`acme.myshopify.com`), compared
    /// case-insensitively and stored lowercase.
    ///
    /// # Errors
    ///
    /// [`InvalidValue`] unless the handle is 1–63 ASCII letters, digits or inner hyphens (one DNS
    /// label).
    pub fn new(domain: &str) -> Result<Self, InvalidValue> {
        let lower = domain.trim().to_ascii_lowercase();
        let handle = lower.strip_suffix(".myshopify.com").unwrap_or(&lower);
        let valid = !handle.is_empty()
            && handle.len() <= 63
            && handle
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-')
            && !handle.starts_with('-')
            && !handle.ends_with('-');
        if !valid {
            return Err(InvalidValue("shop domain"));
        }
        Ok(Self(handle.to_owned()))
    }

    /// The handle, `acme`.
    pub fn handle(&self) -> &str {
        &self.0
    }

    /// The full domain, `acme.myshopify.com`.
    pub fn myshopify_domain(&self) -> String {
        format!("{}.myshopify.com", self.0)
    }
}

impl fmt::Display for ShopDomain {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// An Admin API access token (sent as `X-Shopify-Access-Token`). No `Display`; `Debug` is
/// redacted. [`expose`](Self::expose) is the one deliberate way to read it.
#[derive(Clone, PartialEq, Eq, concord::Debug)]
pub struct AccessToken(#[sensitive] String);

impl AccessToken {
    /// The token.
    ///
    /// # Errors
    ///
    /// [`InvalidValue`] for an empty token or one that is not a valid header value (visible
    /// ASCII), so a token can never fail later when a request is built.
    pub fn new(token: impl Into<String>) -> Result<Self, InvalidValue> {
        let token = token.into();
        if token.is_empty() || !token.bytes().all(|b| (0x21..=0x7e).contains(&b)) {
            return Err(InvalidValue("access token"));
        }
        Ok(Self(token))
    }

    /// The secret value. Do not log it.
    pub fn expose(&self) -> &str {
        &self.0
    }
}

/// An opaque connection cursor (`pageInfo.endCursor`). Passed back verbatim as `after`; never
/// printed (`Debug` is `Cursor(<redacted>)`).
#[derive(Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
#[derive(concord::Debug)]
pub struct Cursor(#[sensitive] String);

impl Cursor {
    /// A cursor obtained elsewhere (a stored checkpoint).
    pub fn new(cursor: impl Into<String>) -> Self {
        Self(cursor.into())
    }

    /// The opaque value. Do not log it.
    pub fn expose(&self) -> &str {
        &self.0
    }
}

/// A signed, expiring download URL of a bulk operation's result (`url`, `partialDataUrl`). The
/// signature authorizes the download, so the URL is a secret: `Debug` shows the host only, and
/// there is no `Display`.
#[derive(Clone, PartialEq, Eq)]
pub struct SignedUrl(Url);

impl SignedUrl {
    /// A URL obtained elsewhere.
    ///
    /// # Errors
    ///
    /// [`InvalidValue`] unless it is an absolute `https` (or, for loopback tests, `http`) URL with
    /// a host.
    pub fn parse(url: &str) -> Result<Self, InvalidValue> {
        let invalid = InvalidValue("signed URL");
        let url = Url::parse(url).map_err(|_| invalid)?;
        if !matches!(url.scheme(), "https" | "http") || url.host_str().is_none() {
            return Err(invalid);
        }
        Ok(Self(url))
    }

    /// The host (not secret), for diagnostics.
    pub fn host(&self) -> &str {
        self.0.host_str().unwrap_or_default()
    }

    /// The full URL, signature included. Do not log it.
    pub fn expose(&self) -> &Url {
        &self.0
    }
}

impl fmt::Debug for SignedUrl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SignedUrl")
            .field("host", &self.host())
            .finish_non_exhaustive()
    }
}

impl<'de> Deserialize<'de> for SignedUrl {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        Self::parse(&text).map_err(|_| de::Error::custom("not an absolute http(s) URL"))
    }
}

/// The GraphQL `Decimal` (and `Money`) scalar: an arbitrary-precision decimal serialized as a
/// string (`"29.99"`). Kept as its exact text; this SDK does no arithmetic on money.
#[derive(Clone, PartialEq, Eq, Hash, Serialize)]
#[serde(transparent)]
#[derive(concord::Debug)]
pub struct Decimal(String);

impl Decimal {
    /// The decimal text as the API sent it.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Whether the value is negative.
    pub fn is_negative(&self) -> bool {
        self.0.starts_with('-')
    }
}

impl<'de> Deserialize<'de> for Decimal {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        let digits = text.strip_prefix('-').unwrap_or(&text);
        let (whole, fraction) = digits.split_once('.').unwrap_or((digits, ""));
        let valid = !whole.is_empty()
            && whole.bytes().all(|b| b.is_ascii_digit())
            && fraction.bytes().all(|b| b.is_ascii_digit())
            && !(digits.contains('.') && fraction.is_empty());
        if !valid {
            return Err(de::Error::custom("not a Decimal string"));
        }
        Ok(Self(text))
    }
}

serde_plain::derive_display_from_serialize!(Decimal);

/// The GraphQL `DateTime` scalar (ISO 8601 text such as `2026-10-01T12:00:00Z`), kept as text:
/// the SDK compares and reports it, it does no calendar arithmetic.
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
#[derive(concord::Debug)]
pub struct DateTime(String);

impl DateTime {
    /// The timestamp text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// The GraphQL `UnsignedInt64` scalar, "encoded as a string of base-10 digits" (`"57"`).
///
/// A JSON number is also accepted (lenient reading, never produced by the documented schema).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct UnsignedInt64(pub u64);

impl<'de> Deserialize<'de> for UnsignedInt64 {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct V;
        impl Visitor<'_> for V {
            type Value = UnsignedInt64;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("an UnsignedInt64 string")
            }
            fn visit_str<E: de::Error>(self, v: &str) -> Result<UnsignedInt64, E> {
                if v.is_empty() || !v.bytes().all(|b| b.is_ascii_digit()) {
                    return Err(E::custom("not an UnsignedInt64 string"));
                }
                v.parse()
                    .map(UnsignedInt64)
                    .map_err(|_| E::custom("UnsignedInt64 out of range"))
            }
            fn visit_u64<E: de::Error>(self, v: u64) -> Result<UnsignedInt64, E> {
                Ok(UnsignedInt64(v))
            }
        }
        deserializer.deserialize_any(V)
    }
}

/// The kind of object a [`Gid`] designates (`Product` in `gid://shopify/Product/1`).
pub trait GidKind {
    /// The type segment of the global id.
    const TYPE: &'static str;
}

/// A Shopify global id, `gid://shopify/{Type}/{id}`, checked to designate a `K` when it is parsed
/// or deserialized. The remainder after the type is opaque (an `InventoryLevel` id carries a
/// query, `.../InventoryLevel/1?inventory_item_id=2`).
#[derive(concord::Debug)]
pub struct Gid<K> {
    raw: String,
    #[sensitive]
    kind: PhantomData<fn() -> K>,
}

impl<K: GidKind> Gid<K> {
    /// Parses a global id of kind `K`.
    ///
    /// # Errors
    ///
    /// [`InvalidValue`] unless it starts with `gid://shopify/{K::TYPE}/` followed by a non-empty
    /// remainder without whitespace or control characters.
    pub fn parse(raw: &str) -> Result<Self, InvalidValue> {
        let rest = raw
            .strip_prefix("gid://shopify/")
            .and_then(|r| r.strip_prefix(K::TYPE))
            .and_then(|r| r.strip_prefix('/'));
        match rest {
            Some(rest) if !rest.is_empty() && rest.bytes().all(|b| (0x21..=0x7e).contains(&b)) => {
                Ok(Self {
                    raw: raw.to_owned(),
                    kind: PhantomData,
                })
            }
            _ => Err(InvalidValue(K::TYPE)),
        }
    }

    /// The id text, `gid://shopify/Product/1`.
    pub fn as_str(&self) -> &str {
        &self.raw
    }

    /// The part after the type, `1` (or `1?inventory_item_id=2`).
    pub fn tail(&self) -> &str {
        &self.raw["gid://shopify/".len() + K::TYPE.len() + 1..]
    }
}

impl<K> Clone for Gid<K> {
    fn clone(&self) -> Self {
        Self {
            raw: self.raw.clone(),
            kind: PhantomData,
        }
    }
}

impl<K> PartialEq for Gid<K> {
    fn eq(&self, other: &Self) -> bool {
        self.raw == other.raw
    }
}

impl<K> Eq for Gid<K> {}

impl<K> std::hash::Hash for Gid<K> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.raw.hash(state);
    }
}

impl<K> PartialOrd for Gid<K> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl<K> Ord for Gid<K> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.raw.cmp(&other.raw)
    }
}

impl<K> fmt::Display for Gid<K> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.raw)
    }
}

impl<K> Serialize for Gid<K> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.raw)
    }
}

impl<'de, K: GidKind> Deserialize<'de> for Gid<K> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(deserializer)?;
        Self::parse(&raw).map_err(|_| de::Error::custom("global id of another type"))
    }
}

macro_rules! kinds {
    ($($(#[$doc:meta])* $kind:ident => $alias:ident;)*) => {$(
        $(#[$doc])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        pub enum $kind {}
        impl GidKind for $kind {
            const TYPE: &'static str = stringify!($kind);
        }
        #[doc = concat!("The global id of a `", stringify!($kind), "`.")]
        pub type $alias = Gid<$kind>;
    )*};
}

kinds! {
    /// A product.
    Product => ProductId;
    /// A product variant.
    ProductVariant => ProductVariantId;
    /// A collection.
    Collection => CollectionId;
    /// A location.
    Location => LocationId;
    /// An inventory item.
    InventoryItem => InventoryItemId;
    /// An inventory level.
    InventoryLevel => InventoryLevelId;
    /// A bulk operation.
    BulkOperation => BulkOperationId;
    /// A shop.
    Shop => ShopId;
    /// A metafield.
    Metafield => MetafieldId;
}
