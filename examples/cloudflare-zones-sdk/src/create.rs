//! Validated create inputs. The read models remain open to unknown provider values.

use std::net::{Ipv4Addr, Ipv6Addr};

use serde::Serialize;

use crate::RecordType;

/// A create-input constraint failure. Contains no caller-supplied values.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
#[derive(concord::derive::Error)]
#[display(with = display_record_input_error)]
pub enum RecordInputError {
    /// The create target's zone identifier must contain 1..=32 characters.
    ZoneId,
    /// The record type needs fields or content validation outside this create slice.
    UnsupportedType,
    /// The complete ASCII/Punycode name must be 1..=255 bytes with valid DNS labels.
    Name,
    /// Content does not match the selected record type.
    Content,
    /// TTL must be 1 (automatic), or 60..=86400 (30..=86400 for Enterprise).
    Ttl,
    /// Only A, AAAA and CNAME records may enable proxying.
    Proxied,
}

fn display_record_input_error(
    value: &RecordInputError,
    f: &mut std::fmt::Formatter<'_>,
) -> std::fmt::Result {
    f.write_str(match value {
        RecordInputError::ZoneId => "create requires a zone identifier of 1..=32 characters",
        RecordInputError::UnsupportedType => {
            "create supports A, AAAA, CNAME, NS, PTR and TXT records"
        }
        RecordInputError::Name => "invalid complete ASCII/Punycode DNS record name",
        RecordInputError::Content => "invalid content for the DNS record type",
        RecordInputError::Ttl => {
            "TTL must be automatic (1) or within the zone plan's documented range"
        }
        RecordInputError::Proxied => "proxying requires an A, AAAA or CNAME record",
    })
}

// ZoneId stays permissive for existing readers. Validate only the new create target, after
// Concord has encoded the scope identifier as one segment, before any network I/O.
pub(crate) fn validate_target<T>(
    prepared: &concord::Prepared<T, crate::ErrorEnvelope>,
) -> Result<(), RecordInputError> {
    let segment = prepared
        .request()
        .url()
        .path_segments()
        .and_then(|mut segments| segments.nth_back(1))
        .ok_or(RecordInputError::ZoneId)?;
    let decoded = percent_encoding::percent_decode_str(segment)
        .decode_utf8()
        .map_err(|_| RecordInputError::ZoneId)?;
    if (1..=32).contains(&decoded.chars().count()) {
        Ok(())
    } else {
        Err(RecordInputError::ZoneId)
    }
}

/// A validated TTL for creation. `1` means automatic.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct RecordTtl(u32);

impl RecordTtl {
    /// Cloudflare chooses the TTL.
    pub const AUTOMATIC: Self = Self(1);

    /// A TTL for any zone: `1` or `60..=86400` seconds.
    pub fn new(seconds: u32) -> Result<Self, RecordInputError> {
        Self::checked(seconds, 60)
    }

    /// A TTL for an Enterprise zone: `1` or `30..=86400` seconds.
    /// The caller must know that the selected zone has an Enterprise plan.
    pub fn enterprise(seconds: u32) -> Result<Self, RecordInputError> {
        Self::checked(seconds, 30)
    }

    fn checked(seconds: u32, minimum: u32) -> Result<Self, RecordInputError> {
        if seconds == 1 || (minimum..=86400).contains(&seconds) {
            Ok(Self(seconds))
        } else {
            Err(RecordInputError::Ttl)
        }
    }
}

/// A validated content-based create body, with private fields and no unchecked deserializer.
///
/// Supports A, AAAA, CNAME, NS, PTR and TXT. Other types require additional structured fields
/// or validation and are refused. Names must already be ASCII/Punycode (underscore labels and
/// a leading wildcard are accepted). TXT content uses quoted DNS character strings; long
/// strings are left for Cloudflare to split as documented. Cross-record conflicts remain
/// provider errors. See `PROVENANCE.md` for the exact reference and read date.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct NewDnsRecord {
    #[serde(rename = "type")]
    record_type: &'static str,
    name: String,
    content: String,
    ttl: RecordTtl,
    #[serde(skip_serializing_if = "Option::is_none")]
    proxied: Option<bool>,
}

impl NewDnsRecord {
    /// Validate the complete body before it can be handed to `dns_records().create(..)`.
    /// `proxied: true` is accepted only for A, AAAA and CNAME; `None` omits the field.
    ///
    /// # Errors
    /// Returns the failing constraint, without retaining the supplied values.
    pub fn new(
        record_type: RecordType,
        name: impl Into<String>,
        content: impl Into<String>,
        ttl: RecordTtl,
        proxied: Option<bool>,
    ) -> Result<Self, RecordInputError> {
        let kind = match record_type {
            RecordType::A => "A",
            RecordType::Aaaa => "AAAA",
            RecordType::Cname => "CNAME",
            RecordType::Ns => "NS",
            RecordType::Ptr => "PTR",
            RecordType::Txt => "TXT",
            _ => return Err(RecordInputError::UnsupportedType),
        };
        let name = name.into();
        if !dns_name(&name, true) {
            return Err(RecordInputError::Name);
        }
        let content = content.into();
        let valid = match kind {
            "A" => content.parse::<Ipv4Addr>().is_ok(),
            "AAAA" => content.parse::<Ipv6Addr>().is_ok(),
            "CNAME" => {
                dns_name(&content, false)
                    && !name
                        .trim_end_matches('.')
                        .eq_ignore_ascii_case(content.trim_end_matches('.'))
            }
            "NS" | "PTR" => dns_name(&content, false),
            "TXT" => quoted_txt(&content),
            _ => unreachable!(),
        };
        if !valid {
            return Err(RecordInputError::Content);
        }
        if proxied == Some(true) && !matches!(kind, "A" | "AAAA" | "CNAME") {
            return Err(RecordInputError::Proxied);
        }
        Ok(Self {
            record_type: kind,
            name,
            content,
            ttl,
            proxied,
        })
    }
}

fn dns_name(value: &str, record_name: bool) -> bool {
    if value.is_empty() || value.len() > 255 || !value.is_ascii() {
        return false;
    }
    value
        .strip_suffix('.')
        .unwrap_or(value)
        .split('.')
        .enumerate()
        .all(|(index, label)| {
            if record_name && index == 0 && label == "*" {
                return true;
            }
            !label.is_empty()
                && label.len() <= 63
                && !label.starts_with('-')
                && !label.ends_with('-')
                && label
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-' || (record_name && b == b'_'))
        })
}

// Quoted strings separated by whitespace; escaped characters/decimal octets stay untouched.
fn quoted_txt(value: &str) -> bool {
    let mut bytes = value.bytes().peekable();
    let mut strings = 0;
    loop {
        while bytes.peek().is_some_and(u8::is_ascii_whitespace) {
            bytes.next();
        }
        if bytes.peek().is_none() {
            return strings > 0;
        }
        if bytes.next() != Some(b'"') {
            return false;
        }
        loop {
            match bytes.next() {
                Some(b'"') => break,
                Some(b'\\') => {
                    let Some(escaped) = bytes.next() else {
                        return false;
                    };
                    if escaped.is_ascii_digit() {
                        let (Some(b), Some(c)) = (bytes.next(), bytes.next()) else {
                            return false;
                        };
                        if !b.is_ascii_digit() || !c.is_ascii_digit() {
                            return false;
                        }
                        let octet = u16::from(escaped - b'0') * 100
                            + u16::from(b - b'0') * 10
                            + u16::from(c - b'0');
                        if octet > 255 {
                            return false;
                        }
                    }
                }
                Some(b) if b < 0x20 || b == 0x7f => return false,
                Some(_) => {}
                None => return false,
            }
        }
        strings += 1;
        if bytes.peek().is_some_and(|b| !b.is_ascii_whitespace()) {
            return false;
        }
    }
}
