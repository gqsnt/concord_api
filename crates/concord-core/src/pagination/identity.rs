use std::cmp::Ordering;

use http::HeaderMap;

use super::PaginationError;
use crate::error::PaginationErrorKind;

const FORMAT_PREFIX: &[u8] = b"CONCORD-PAGE-IDENTITY\x01";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum IdentityEncodingError {
    InvalidHeaderName,
    EncodedFieldLengthOverflow,
    CanonicalIdentityLengthOverflow,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct EncodingLimits {
    pub(super) max_field_length: u64,
    pub(super) max_total_length: usize,
}

impl Default for EncodingLimits {
    fn default() -> Self {
        Self {
            max_field_length: u64::MAX,
            max_total_length: usize::MAX,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct HeaderField {
    pub(crate) name: Vec<u8>,
    pub(crate) value: Vec<u8>,
}

impl HeaderField {
    pub(crate) fn new(name: impl AsRef<[u8]>, value: impl AsRef<[u8]>) -> Self {
        Self {
            name: name.as_ref().to_vec(),
            value: value.as_ref().to_vec(),
        }
    }
}

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
    pub(crate) fn encode(input: IdentityInput<'_>) -> Result<Self, IdentityEncodingError> {
        Self::encode_with_limits(input, EncodingLimits::default())
    }

    pub(crate) fn encode_with_limits(
        input: IdentityInput<'_>,
        limits: EncodingLimits,
    ) -> Result<Self, IdentityEncodingError> {
        let mut encoder = Encoder::new(limits);
        encoder.fixed(FORMAT_PREFIX)?;
        encoder.field(1, input.endpoint_discriminator)?;
        encoder.field(2, input.method_token)?;
        encoder.field(3, input.canonical_logical_url)?;

        let mut headers = input
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
        headers.sort_by(|left, right| match left.name.cmp(&right.name) {
            Ordering::Equal => left.original_index.cmp(&right.original_index),
            other => other,
        });

        let retained_byte_len = checked_component_byte_len(&input, &headers)?;

        encoder.tag(4)?;
        encoder.count(headers.len())?;
        for header in headers {
            encoder.field(5, &header.name)?;
            encoder.field(6, header.value)?;
        }

        Ok(Self {
            bytes: encoder.finish(),
            retained_byte_len,
        })
    }

    pub(crate) fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub(crate) fn retained_byte_len(&self) -> usize {
        self.retained_byte_len
    }

    pub(crate) fn from_request_head(
        endpoint: &str,
        method: &http::Method,
        logical_url: &url::Url,
        public_headers: &HeaderMap,
    ) -> Result<Self, PaginationError> {
        let fields = public_headers
            .iter()
            .map(|(name, value)| HeaderField::new(name.as_str().as_bytes(), value.as_bytes()))
            .collect::<Vec<_>>();
        Self::encode(IdentityInput {
            endpoint_discriminator: endpoint.as_bytes(),
            method_token: method.as_str().as_bytes(),
            canonical_logical_url: logical_url.as_str().as_bytes(),
            public_headers: &fields,
        })
        .map_err(|_error| PaginationError::new(PaginationErrorKind::Overflow))
    }
}

struct CanonicalHeader<'a> {
    name: Vec<u8>,
    value: &'a [u8],
    original_index: usize,
}

fn checked_component_byte_len(
    input: &IdentityInput<'_>,
    headers: &[CanonicalHeader<'_>],
) -> Result<usize, IdentityEncodingError> {
    let mut total = 0usize;
    for length in [
        input.endpoint_discriminator.len(),
        input.method_token.len(),
        input.canonical_logical_url.len(),
    ] {
        total = total
            .checked_add(length)
            .ok_or(IdentityEncodingError::CanonicalIdentityLengthOverflow)?;
    }
    for header in headers {
        total = total
            .checked_add(header.name.len())
            .and_then(|sum| sum.checked_add(header.value.len()))
            .ok_or(IdentityEncodingError::CanonicalIdentityLengthOverflow)?;
    }
    Ok(total)
}

fn canonical_header_name(name: &[u8]) -> Result<Vec<u8>, IdentityEncodingError> {
    if name.is_empty() || !name.iter().copied().all(is_header_token_byte) {
        return Err(IdentityEncodingError::InvalidHeaderName);
    }
    Ok(name.iter().map(u8::to_ascii_lowercase).collect())
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
    limits: EncodingLimits,
}

impl Encoder {
    fn new(limits: EncodingLimits) -> Self {
        Self {
            bytes: Vec::new(),
            limits,
        }
    }

    fn finish(self) -> Vec<u8> {
        self.bytes
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
        self.tag(tag)?;
        self.fixed(&length.to_be_bytes())?;
        self.fixed(value)
    }
}

pub(crate) fn deterministic_digest(bytes: &[u8]) -> u64 {
    const OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;

    bytes.iter().fold(OFFSET_BASIS, |digest, byte| {
        (digest ^ u64::from(*byte)).wrapping_mul(PRIME)
    })
}
