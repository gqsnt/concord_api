//! Cloudflare's resources as this slice reads them, and the typed request values.
//!
//! Field selection and optionality follow the cited reference pages (see `PROVENANCE.md`): the
//! workflow fields (`id`, `name`, record `type`, `ttl`) are required; descriptive fields are
//! optional so that a field Cloudflare omits or nulls does not make a zone or record unreadable.
//! Unknown fields are ignored. Timestamps are kept as the RFC 3339 text Cloudflare sends.

use serde::{Deserialize, Deserializer, Serialize};

macro_rules! identifier {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        ///
        /// Cloudflare documents identifiers as opaque strings of at most 32 characters; the SDK
        /// passes the value through unchanged (Concord encodes it as exactly one path segment).
        #[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Deserialize)]
        #[derive(Serialize)]
        #[serde(transparent)]
        pub struct $name(String);
serde_plain::derive_display_from_serialize!($name);

        impl $name {
            /// The identifier as sent and received.
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl From<String> for $name {
            fn from(value: String) -> Self {
                Self(value)
            }
        }

        impl From<&str> for $name {
            fn from(value: &str) -> Self {
                Self(value.to_owned())
            }
        }
    };
}

identifier!(
    /// A zone identifier (`zone_id`).
    ZoneId
);
identifier!(
    /// An account identifier (`account.id`).
    AccountId
);
identifier!(
    /// A DNS record identifier (`dns_record_id`).
    RecordId
);

/// A provider enumeration: the documented values, and `Other` for a value this SDK does not
/// know (kept exactly as Cloudflare sent it, never an error).
macro_rules! provider_enum {
    ($(#[$meta:meta])* $name:ident { $($(#[$vmeta:meta])* $variant:ident = $wire:literal,)* }) => {
        $(#[$meta])*
        #[derive(Clone, Debug, PartialEq, Eq, Hash)]
        #[derive(Serialize)]
        #[non_exhaustive]
        pub enum $name {
            $($(#[$vmeta])* #[serde(rename = $wire)] $variant,)*
            /// A value not documented on the pages this SDK cites, kept as sent.
            #[serde(untagged)]
            Other(String),
        }
serde_plain::derive_display_from_serialize!($name);

        impl $name {
            /// The wire spelling.
            pub fn as_str(&self) -> &str {
                match self {
                    $(Self::$variant => $wire,)*
                    Self::Other(value) => value,
                }
            }
        }

        impl From<String> for $name {
            fn from(value: String) -> Self {
                match value.as_str() {
                    $($wire => Self::$variant,)*
                    _ => Self::Other(value),
                }
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                String::deserialize(deserializer).map(Self::from)
            }
        }
    };
}

/// A request-only choice with a fixed wire spelling.
macro_rules! request_enum {
    ($(#[$meta:meta])* $name:ident { $($(#[$vmeta:meta])* $variant:ident = $wire:literal,)* }) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        #[derive(Serialize)]
        pub enum $name {
            $($(#[$vmeta])* #[serde(rename = $wire)] $variant,)*
        }
serde_plain::derive_display_from_serialize!($name);
    };
}

provider_enum!(
    /// A zone's status (`status`), also a List Zones filter.
    ZoneStatus {
        /// `initializing`
        Initializing = "initializing",
        /// `pending`
        Pending = "pending",
        /// `active`
        Active = "active",
        /// `moved`
        Moved = "moved",
    }
);

provider_enum!(
    /// A zone's type (`type`).
    ZoneType {
        /// `full`: DNS is hosted with Cloudflare.
        Full = "full",
        /// `partial`: a partner-hosted zone or a CNAME setup.
        Partial = "partial",
        /// `secondary`
        Secondary = "secondary",
        /// `internal`
        Internal = "internal",
    }
);

provider_enum!(
    /// A DNS record type (`type`), also a List DNS Records filter.
    RecordType {
        /// `A`
        A = "A",
        /// `AAAA`
        Aaaa = "AAAA",
        /// `CAA`
        Caa = "CAA",
        /// `CERT`
        Cert = "CERT",
        /// `CNAME`
        Cname = "CNAME",
        /// `DNSKEY`
        Dnskey = "DNSKEY",
        /// `DS`
        Ds = "DS",
        /// `HTTPS`
        Https = "HTTPS",
        /// `LOC`
        Loc = "LOC",
        /// `MX`
        Mx = "MX",
        /// `NAPTR`
        Naptr = "NAPTR",
        /// `NS`
        Ns = "NS",
        /// `OPENPGPKEY`
        Openpgpkey = "OPENPGPKEY",
        /// `PTR`
        Ptr = "PTR",
        /// `SMIMEA`
        Smimea = "SMIMEA",
        /// `SRV`
        Srv = "SRV",
        /// `SSHFP`
        Sshfp = "SSHFP",
        /// `SVCB`
        Svcb = "SVCB",
        /// `TLSA`
        Tlsa = "TLSA",
        /// `TXT`
        Txt = "TXT",
        /// `URI`
        Uri = "URI",
    }
);

request_enum!(
    /// List Zones `order`.
    ZoneOrder {
        /// `name`
        Name = "name",
        /// `status`
        Status = "status",
        /// `account.id`
        AccountId = "account.id",
        /// `account.name`
        AccountName = "account.name",
        /// `plan.id`
        PlanId = "plan.id",
    }
);

request_enum!(
    /// List DNS Records `order`.
    RecordOrder {
        /// `type`
        Type = "type",
        /// `name`
        Name = "name",
        /// `content`
        Content = "content",
        /// `ttl`
        Ttl = "ttl",
        /// `proxied`
        Proxied = "proxied",
    }
);

request_enum!(
    /// `direction` of a listing's order.
    Direction {
        /// `asc`
        Asc = "asc",
        /// `desc`
        Desc = "desc",
    }
);

/// List Zones `per_page`: documented as 5 to 50.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize)]
#[serde(transparent)]
pub struct ZonesPerPage(u8);
serde_plain::derive_display_from_serialize!(ZonesPerPage);

impl ZonesPerPage {
    /// The documented maximum, 50.
    pub const MAX: Self = Self(50);

    /// A page size within 5..=50, or `None`.
    pub fn new(value: u8) -> Option<Self> {
        (5..=50).contains(&value).then_some(Self(value))
    }

    /// The number sent.
    pub fn get(self) -> u8 {
        self.0
    }
}

/// List DNS Records `per_page`: documented as 1 to 5,000,000. A large page must still fit in
/// [`SUCCESS_BODY_LIMIT`](crate::SUCCESS_BODY_LIMIT).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize)]
#[serde(transparent)]
pub struct RecordsPerPage(u32);
serde_plain::derive_display_from_serialize!(RecordsPerPage);

impl RecordsPerPage {
    /// The documented maximum, 5,000,000.
    pub const MAX: Self = Self(5_000_000);

    /// A page size within 1..=5,000,000, or `None`.
    pub fn new(value: u32) -> Option<Self> {
        (1..=5_000_000).contains(&value).then_some(Self(value))
    }

    /// The number sent.
    pub fn get(self) -> u32 {
        self.0
    }
}

/// The account a zone belongs to.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[non_exhaustive]
pub struct ZoneAccount {
    /// The account identifier.
    #[serde(default)]
    pub id: Option<AccountId>,
    /// The account name.
    #[serde(default)]
    pub name: Option<String>,
}

/// A zone (List Zones items, Zone Details `result`).
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[non_exhaustive]
pub struct Zone {
    /// The zone identifier.
    pub id: ZoneId,
    /// The domain name.
    pub name: String,
    /// The zone status.
    #[serde(default)]
    pub status: Option<ZoneStatus>,
    /// The account the zone belongs to.
    #[serde(default)]
    pub account: ZoneAccount,
    /// The zone type (`type`).
    #[serde(default, rename = "type")]
    pub zone_type: Option<ZoneType>,
    /// Whether the zone only uses Cloudflare DNS services.
    #[serde(default)]
    pub paused: Option<bool>,
    /// The name servers Cloudflare assigns to the zone.
    #[serde(default)]
    pub name_servers: Vec<String>,
    /// The name servers before the move to Cloudflare.
    #[serde(default)]
    pub original_name_servers: Option<Vec<String>>,
    /// Seconds until development mode expires (positive) or since it expired (negative); 0 if
    /// it was never enabled.
    #[serde(default)]
    pub development_mode: Option<i64>,
    /// When the zone was created (RFC 3339 text).
    #[serde(default)]
    pub created_on: Option<String>,
    /// When ownership was last proven and the zone activated (RFC 3339 text).
    #[serde(default)]
    pub activated_on: Option<String>,
    /// When the zone was last modified (RFC 3339 text).
    #[serde(default)]
    pub modified_on: Option<String>,
}

/// A DNS record (List DNS Records items, DNS Record Details `result`).
///
/// The fields every record type shares. Type-specific structured fields (an MX priority, an SRV
/// `data` object) are not modelled by this slice.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[non_exhaustive]
pub struct DnsRecord {
    /// The record identifier.
    pub id: RecordId,
    /// The complete record name, including the zone name, in Punycode.
    pub name: String,
    /// The record type (`type`).
    #[serde(rename = "type")]
    pub record_type: RecordType,
    /// The record content, as text, for the types that carry it.
    #[serde(default)]
    pub content: Option<String>,
    /// Time to live in seconds; `1` means automatic ([`DnsRecord::ttl_is_automatic`]).
    pub ttl: u32,
    /// Whether the record receives Cloudflare's performance and security benefits.
    #[serde(default)]
    pub proxied: Option<bool>,
    /// Whether the record can be proxied.
    #[serde(default)]
    pub proxiable: Option<bool>,
    /// The record's comment.
    #[serde(default)]
    pub comment: Option<String>,
    /// The record's custom tags (`name:value`).
    #[serde(default)]
    pub tags: Vec<String>,
    /// When the record was created (RFC 3339 text).
    #[serde(default)]
    pub created_on: Option<String>,
    /// When the record was last modified (RFC 3339 text).
    #[serde(default)]
    pub modified_on: Option<String>,
}

impl DnsRecord {
    /// `ttl == 1`: Cloudflare chooses the TTL.
    pub fn ttl_is_automatic(&self) -> bool {
        self.ttl == 1
    }
}
