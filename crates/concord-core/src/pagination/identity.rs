use std::borrow::Cow;
use std::cmp::Ordering;
use std::mem::size_of;

use http::HeaderMap;

use super::PaginationError;
use crate::error::PaginationErrorKind;

const FORMAT_PREFIX: &[u8] = b"CONCORD-PAGE-IDENTITY\x01";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum IdentityEncodingError {
    InvalidHeaderName,
    EncodedFieldLengthOverflow,
    CanonicalIdentityLengthOverflow,
    IdentityStorageCeilingExceeded,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct EncodingLimits {
    pub(super) max_field_length: u64,
    pub(super) max_total_length: usize,
    pub(super) max_retained_length: usize,
}

impl Default for EncodingLimits {
    fn default() -> Self {
        Self {
            max_field_length: u64::MAX,
            max_total_length: usize::MAX,
            max_retained_length: usize::MAX,
        }
    }
}

#[cfg(test)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct HeaderField {
    pub(crate) name: Vec<u8>,
    pub(crate) value: Vec<u8>,
}

#[cfg(test)]
impl HeaderField {
    pub(crate) fn new(name: impl AsRef<[u8]>, value: impl AsRef<[u8]>) -> Self {
        Self {
            name: name.as_ref().to_vec(),
            value: value.as_ref().to_vec(),
        }
    }
}

#[cfg(test)]
#[derive(Clone, Debug)]
pub(crate) struct IdentityInput<'a> {
    pub(crate) endpoint_discriminator: &'a [u8],
    pub(crate) method_token: &'a [u8],
    pub(crate) canonical_logical_url: &'a [u8],
    pub(crate) public_headers: &'a [HeaderField],
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CanonicalIdentity {
    bytes: Vec<u8>,
    retained_byte_len: usize,
}

impl CanonicalIdentity {
    #[cfg(test)]
    pub(crate) fn encode(input: IdentityInput<'_>) -> Result<Self, IdentityEncodingError> {
        Self::encode_with_limits(input, EncodingLimits::default())
    }

    #[cfg(test)]
    pub(crate) fn encode_with_limits(
        input: IdentityInput<'_>,
        limits: EncodingLimits,
    ) -> Result<Self, IdentityEncodingError> {
        let headers = input
            .public_headers
            .iter()
            .enumerate()
            .map(|(original_index, header)| {
                canonical_header_name(&header.name).map(|name| CanonicalHeader {
                    name,
                    value: &header.value,
                    original_index,
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        Self::encode_canonical(
            input.endpoint_discriminator,
            input.method_token,
            input.canonical_logical_url,
            headers,
            limits,
        )
    }

    fn encode_canonical(
        endpoint_discriminator: &[u8],
        method_token: &[u8],
        canonical_logical_url: &[u8],
        mut headers: Vec<CanonicalHeader<'_>>,
        limits: EncodingLimits,
    ) -> Result<Self, IdentityEncodingError> {
        headers.sort_by(|left, right| match left.name.cmp(&right.name) {
            Ordering::Equal => left.original_index.cmp(&right.original_index),
            other => other,
        });

        let mut encoder = Encoder::new(limits);
        encoder.fixed(FORMAT_PREFIX)?;
        encoder.field(1, endpoint_discriminator)?;
        encoder.field(2, method_token)?;
        encoder.field(3, canonical_logical_url)?;
        encoder.tag(4)?;
        encoder.count(headers.len())?;
        for header in headers {
            encoder.field(5, header.name.as_ref())?;
            encoder.field(6, header.value)?;
        }
        let (bytes, retained_byte_len) = encoder.finish();

        Ok(Self {
            bytes,
            retained_byte_len,
        })
    }

    pub(crate) fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub(crate) fn retained_byte_len(&self) -> usize {
        self.retained_byte_len
    }

    pub(crate) fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }

    #[cfg(test)]
    pub(crate) fn from_request_head(
        endpoint: &str,
        method: &http::Method,
        logical_url: &url::Url,
        public_headers: &HeaderMap,
    ) -> Result<Self, PaginationError> {
        Self::from_request_head_with_limit(
            endpoint,
            method,
            logical_url,
            public_headers,
            usize::MAX,
        )
    }

    pub(crate) fn from_request_head_with_limit(
        endpoint: &str,
        method: &http::Method,
        logical_url: &url::Url,
        public_headers: &HeaderMap,
        remaining_identity_bytes: usize,
    ) -> Result<Self, PaginationError> {
        let headers = public_headers
            .iter()
            .enumerate()
            .map(|(original_index, (name, value))| {
                canonical_header_name(name.as_str().as_bytes()).map(|name| CanonicalHeader {
                    name,
                    value: value.as_bytes(),
                    original_index,
                })
            })
            .collect::<Result<Vec<_>, _>>()
            .map_err(pagination_encoding_error)?;
        Self::encode_canonical(
            endpoint.as_bytes(),
            method.as_str().as_bytes(),
            logical_url.as_str().as_bytes(),
            headers,
            EncodingLimits {
                max_retained_length: remaining_identity_bytes,
                ..EncodingLimits::default()
            },
        )
        .map_err(pagination_encoding_error)
    }
}

struct CanonicalHeader<'a> {
    name: Cow<'a, [u8]>,
    value: &'a [u8],
    original_index: usize,
}

fn canonical_header_name(name: &[u8]) -> Result<Cow<'_, [u8]>, IdentityEncodingError> {
    if name.is_empty() || !name.iter().copied().all(is_header_token_byte) {
        return Err(IdentityEncodingError::InvalidHeaderName);
    }
    if name.iter().any(u8::is_ascii_uppercase) {
        Ok(Cow::Owned(
            name.iter().map(u8::to_ascii_lowercase).collect(),
        ))
    } else {
        Ok(Cow::Borrowed(name))
    }
}

fn pagination_encoding_error(error: IdentityEncodingError) -> PaginationError {
    let kind = match error {
        IdentityEncodingError::IdentityStorageCeilingExceeded => {
            PaginationErrorKind::IdentityStorageCeilingExceeded
        }
        IdentityEncodingError::InvalidHeaderName
        | IdentityEncodingError::EncodedFieldLengthOverflow
        | IdentityEncodingError::CanonicalIdentityLengthOverflow => PaginationErrorKind::Overflow,
    };
    PaginationError::new(kind)
}

fn is_header_token_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric()
        || matches!(
            byte,
            b'!' | b'#'
                | b'$'
                | b'%'
                | b'&'
                | b'\''
                | b'*'
                | b'+'
                | b'-'
                | b'.'
                | b'^'
                | b'_'
                | b'`'
                | b'|'
                | b'~'
        )
}

struct Encoder {
    bytes: Vec<u8>,
    retained_byte_len: usize,
    limits: EncodingLimits,
}

impl Encoder {
    fn new(limits: EncodingLimits) -> Self {
        Self {
            bytes: Vec::new(),
            retained_byte_len: 0,
            limits,
        }
    }

    fn finish(self) -> (Vec<u8>, usize) {
        (self.bytes, self.retained_byte_len)
    }

    fn checked_new_len(&self, additional: usize) -> Result<usize, IdentityEncodingError> {
        let new_len = self
            .bytes
            .len()
            .checked_add(additional)
            .ok_or(IdentityEncodingError::CanonicalIdentityLengthOverflow)?;
        if new_len > self.limits.max_total_length {
            return Err(IdentityEncodingError::CanonicalIdentityLengthOverflow);
        }
        Ok(new_len)
    }

    fn fixed(&mut self, bytes: &[u8]) -> Result<(), IdentityEncodingError> {
        self.checked_new_len(bytes.len())?;
        self.bytes.extend_from_slice(bytes);
        Ok(())
    }

    fn tag(&mut self, tag: u8) -> Result<(), IdentityEncodingError> {
        self.checked_new_len(1)?;
        self.bytes.push(tag);
        Ok(())
    }

    fn count(&mut self, count: usize) -> Result<(), IdentityEncodingError> {
        let count =
            u64::try_from(count).map_err(|_| IdentityEncodingError::EncodedFieldLengthOverflow)?;
        if count > self.limits.max_field_length {
            return Err(IdentityEncodingError::EncodedFieldLengthOverflow);
        }
        self.fixed(&count.to_be_bytes())
    }

    fn field(&mut self, tag: u8, value: &[u8]) -> Result<(), IdentityEncodingError> {
        let length = u64::try_from(value.len())
            .map_err(|_| IdentityEncodingError::EncodedFieldLengthOverflow)?;
        if length > self.limits.max_field_length {
            return Err(IdentityEncodingError::EncodedFieldLengthOverflow);
        }
        let additional = 1usize
            .checked_add(size_of::<u64>())
            .and_then(|length| length.checked_add(value.len()))
            .ok_or(IdentityEncodingError::CanonicalIdentityLengthOverflow)?;
        self.checked_new_len(additional)?;
        let retained_byte_len = self
            .retained_byte_len
            .checked_add(value.len())
            .ok_or(IdentityEncodingError::CanonicalIdentityLengthOverflow)?;
        if retained_byte_len > self.limits.max_retained_length {
            return Err(IdentityEncodingError::IdentityStorageCeilingExceeded);
        }

        self.bytes.push(tag);
        self.bytes.extend_from_slice(&length.to_be_bytes());
        self.bytes.extend_from_slice(value);
        self.retained_byte_len = retained_byte_len;
        Ok(())
    }
}

pub(crate) fn deterministic_digest(bytes: &[u8]) -> u64 {
    const OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;

    bytes.iter().fold(OFFSET_BASIS, |digest, byte| {
        (digest ^ u64::from(*byte)).wrapping_mul(PRIME)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalized_header_names_borrow_and_mixed_case_names_allocate_once() {
        assert!(matches!(
            canonical_header_name(b"x-normalized").expect("valid normalized header"),
            Cow::Borrowed(_)
        ));
        assert!(matches!(
            canonical_header_name(b"X-Mixed").expect("valid mixed-case header"),
            Cow::Owned(name) if name == b"x-mixed"
        ));
    }

    #[test]
    fn retained_budget_accepts_exact_and_withholds_first_excess_field() {
        let mut encoder = Encoder::new(EncodingLimits {
            max_retained_length: 3,
            ..EncodingLimits::default()
        });
        encoder.fixed(FORMAT_PREFIX).expect("format prefix");
        encoder.field(1, b"abc").expect("exact retained limit");
        let exact_bytes = encoder.bytes.clone();
        assert_eq!(encoder.retained_byte_len, 3);

        assert_eq!(
            encoder
                .field(2, b"x")
                .expect_err("first excess retained byte"),
            IdentityEncodingError::IdentityStorageCeilingExceeded
        );
        assert_eq!(encoder.bytes, exact_bytes);
        assert_eq!(encoder.retained_byte_len, 3);
    }
}
