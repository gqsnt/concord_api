use std::borrow::Cow;
use std::sync::Arc;

use super::RateLimitError;
use crate::error::RateLimitErrorKind;
use crate::limits::MAX_RATE_LIMIT_KEY_BYTES;

/// Bounded writer used by [`RateKeyPart`] implementations.
///
/// Every public write operation adds explicit type and length framing. Custom
/// implementations should use a stable, globally meaningful type tag (for
/// example `"com.example.tenant-id/v1"`) and a deterministic byte encoding.
/// The writer stops at the first byte beyond its configured bound.
pub struct CanonicalKeyWriter {
    bytes: Vec<u8>,
    limit: usize,
}

impl CanonicalKeyWriter {
    pub(crate) fn new(limit: usize) -> Result<Self, RateLimitError> {
        if limit > MAX_RATE_LIMIT_KEY_BYTES {
            return Err(invalid_key());
        }
        Ok(Self {
            bytes: Vec::new(),
            limit,
        })
    }

    /// Writes one unambiguous tagged byte value.
    ///
    /// Tags must be nonempty printable ASCII and at most 255 bytes. Both the
    /// tag and value are length-framed before their bytes are retained.
    pub fn write_tagged_bytes(
        &mut self,
        type_tag: &str,
        value: &[u8],
    ) -> Result<(), RateLimitError> {
        if type_tag.is_empty()
            || type_tag.len() > u8::MAX as usize
            || !type_tag.bytes().all(|byte| byte.is_ascii_graphic())
        {
            return Err(invalid_key());
        }
        self.extend(&[type_tag.len() as u8])?;
        self.extend(type_tag.as_bytes())?;
        self.extend(&(value.len() as u64).to_be_bytes())?;
        self.extend(value)
    }

    fn extend(&mut self, bytes: &[u8]) -> Result<(), RateLimitError> {
        let next = self
            .bytes
            .len()
            .checked_add(bytes.len())
            .ok_or_else(invalid_key)?;
        if next > self.limit {
            return Err(invalid_key());
        }
        self.bytes.extend_from_slice(bytes);
        Ok(())
    }

    pub(crate) fn into_bytes(self) -> Box<[u8]> {
        self.bytes.into_boxed_slice()
    }
}

/// Deterministic canonical encoding for one rate-key dimension value.
///
/// This contract is intentionally separate from [`std::fmt::Display`]. Human
/// presentation may change without changing distributed quota identity.
/// Implementations must be deterministic for equal semantic values and must
/// not include credentials, process-local addresses, randomized hashes, or
/// unstable debug text.
pub trait RateKeyPart {
    /// Writes this value to Concord's finite canonical writer.
    fn write_canonical(&self, out: &mut CanonicalKeyWriter) -> Result<(), RateLimitError>;
}

impl<T> RateKeyPart for &T
where
    T: RateKeyPart + ?Sized,
{
    fn write_canonical(&self, out: &mut CanonicalKeyWriter) -> Result<(), RateLimitError> {
        (*self).write_canonical(out)
    }
}

impl<T> RateKeyPart for Box<T>
where
    T: RateKeyPart + ?Sized,
{
    fn write_canonical(&self, out: &mut CanonicalKeyWriter) -> Result<(), RateLimitError> {
        (**self).write_canonical(out)
    }
}

impl<T> RateKeyPart for Arc<T>
where
    T: RateKeyPart + ?Sized,
{
    fn write_canonical(&self, out: &mut CanonicalKeyWriter) -> Result<(), RateLimitError> {
        (**self).write_canonical(out)
    }
}

impl<'a, T> RateKeyPart for Cow<'a, T>
where
    T: RateKeyPart + ToOwned + ?Sized,
{
    fn write_canonical(&self, out: &mut CanonicalKeyWriter) -> Result<(), RateLimitError> {
        self.as_ref().write_canonical(out)
    }
}

impl RateKeyPart for str {
    fn write_canonical(&self, out: &mut CanonicalKeyWriter) -> Result<(), RateLimitError> {
        out.write_tagged_bytes("utf8", self.as_bytes())
    }
}

impl RateKeyPart for String {
    fn write_canonical(&self, out: &mut CanonicalKeyWriter) -> Result<(), RateLimitError> {
        self.as_str().write_canonical(out)
    }
}

impl RateKeyPart for bool {
    fn write_canonical(&self, out: &mut CanonicalKeyWriter) -> Result<(), RateLimitError> {
        out.write_tagged_bytes("bool", &[u8::from(*self)])
    }
}

impl RateKeyPart for char {
    fn write_canonical(&self, out: &mut CanonicalKeyWriter) -> Result<(), RateLimitError> {
        out.write_tagged_bytes("char", &(*self as u32).to_be_bytes())
    }
}

macro_rules! integer_impl {
    ($($type:ty => $tag:literal),* $(,)?) => {
        $(
            impl RateKeyPart for $type {
                fn write_canonical(
                    &self,
                    out: &mut CanonicalKeyWriter,
                ) -> Result<(), RateLimitError> {
                    out.write_tagged_bytes($tag, &self.to_be_bytes())
                }
            }
        )*
    };
}

integer_impl! {
    u8 => "u8",
    u16 => "u16",
    u32 => "u32",
    u64 => "u64",
    u128 => "u128",
    i8 => "i8",
    i16 => "i16",
    i32 => "i32",
    i64 => "i64",
    i128 => "i128",
}

impl RateKeyPart for usize {
    fn write_canonical(&self, out: &mut CanonicalKeyWriter) -> Result<(), RateLimitError> {
        out.write_tagged_bytes("usize", &(*self as u64).to_be_bytes())
    }
}

impl RateKeyPart for isize {
    fn write_canonical(&self, out: &mut CanonicalKeyWriter) -> Result<(), RateLimitError> {
        out.write_tagged_bytes("isize", &(*self as i64).to_be_bytes())
    }
}

pub(crate) fn encode_canonical(
    value: &dyn RateKeyPart,
    limit: usize,
) -> Result<Box<[u8]>, RateLimitError> {
    let mut writer = CanonicalKeyWriter::new(limit)?;
    value.write_canonical(&mut writer)?;
    Ok(writer.into_bytes())
}

fn invalid_key() -> RateLimitError {
    RateLimitError::new(RateLimitErrorKind::InvalidKey)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn encode(value: &dyn RateKeyPart) -> Box<[u8]> {
        encode_canonical(value, MAX_RATE_LIMIT_KEY_BYTES).expect("canonical value")
    }

    #[test]
    fn scalar_type_boundaries_are_distinct() {
        assert_ne!(encode(&1_u8), encode(&1_u16));
        assert_ne!(encode(&"1"), encode(&1_u8));
    }

    #[test]
    fn writer_rejects_the_first_excess_without_partial_success() {
        let encoded = encode(&"bounded");
        let mut exact = CanonicalKeyWriter::new(encoded.len()).expect("exact writer");
        "bounded"
            .write_canonical(&mut exact)
            .expect("exact boundary");

        let mut short = CanonicalKeyWriter::new(encoded.len() - 1).expect("short writer");
        assert!("bounded".write_canonical(&mut short).is_err());
    }
}
