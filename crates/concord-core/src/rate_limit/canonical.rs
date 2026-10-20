use std::borrow::Cow;
use std::sync::Arc;

use super::RateLimitError;
use crate::error::RateLimitErrorKind;
use crate::limits::{MAX_RATE_KEY_ENCODING_ID_BYTES, MAX_RATE_LIMIT_KEY_BYTES};

/// Stable encoding contract name of Concord's built-in UTF-8 text encoding.
///
/// `str`, `String`, and the built-in `origin` and `endpoint` dimensions use
/// this contract. Its value bytes use the canonical `utf8` tagged framing.
pub const UTF8_ENCODING_ID: &str = "concord.utf8/1";

/// Bounded writer used by [`RateKeyPart`] and [`RateKeyEncoder`] implementations.
///
/// Every write appends exactly
/// `u8(tag.len) || tag.ascii_bytes || u64_be(bytes.len) || bytes`; several
/// writes concatenate in call order. The writer checks the complete
/// prospective append before retaining any byte of it, so an excessive value
/// never leaves a partial record behind.
///
/// Ownership and sensitivity: the writer owns the protected bytes it has
/// accumulated and has no `Debug` or `Display` implementation. Custom
/// implementations should use a stable, globally meaningful type tag (for
/// example `"com.example.tenant-id/v1"`) and a deterministic byte encoding.
/// The writer is not shared across threads and performs no I/O.
pub struct CanonicalKeyWriter {
    bytes: Vec<u8>,
    limit: usize,
}

impl CanonicalKeyWriter {
    pub(crate) fn new(limit: usize) -> Result<Self, RateLimitError> {
        if limit > MAX_RATE_LIMIT_KEY_BYTES {
            return Err(invalid_configuration());
        }
        Ok(Self {
            bytes: Vec::new(),
            limit,
        })
    }

    /// Writes one unambiguous tagged byte value.
    ///
    /// Tags must be nonempty ASCII graphic text and at most 255 bytes. Both
    /// the tag and value are length-framed. The complete record is checked
    /// against the remaining bound before anything is retained.
    ///
    /// # Errors
    ///
    /// Returns [`RateLimitErrorKind::InvalidKey`] for an invalid tag or when
    /// the record would exceed the writer's bound.
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
        let value_length = u64::try_from(value.len()).map_err(|_| invalid_key())?;
        let record = 1_usize
            .checked_add(type_tag.len())
            .and_then(|bytes| bytes.checked_add(8))
            .and_then(|bytes| bytes.checked_add(value.len()))
            .ok_or_else(invalid_key)?;
        let next = self
            .bytes
            .len()
            .checked_add(record)
            .ok_or_else(invalid_key)?;
        if next > self.limit {
            return Err(invalid_key());
        }
        self.bytes.reserve(record);
        self.bytes.push(type_tag.len() as u8);
        self.bytes.extend_from_slice(type_tag.as_bytes());
        self.bytes.extend_from_slice(&value_length.to_be_bytes());
        self.bytes.extend_from_slice(value);
        Ok(())
    }

    fn into_bytes(self) -> Arc<[u8]> {
        Arc::from(self.bytes)
    }
}

/// Deterministic canonical encoding for one rate-key dimension value.
///
/// This contract is intentionally separate from [`std::fmt::Display`] and
/// from public atom encoders: human presentation may change without changing
/// quota identity.
///
/// `ENCODING_ID` is a stable, nonempty ASCII graphic contract/version name of
/// at most [`crate::rate_limit::MAX_RATE_KEY_ENCODING_ID_BYTES`] bytes, such as
/// `"com.example.tenant/1"`. It is schema metadata carried separately from the
/// value bytes; it is **not** inserted into the canonical bytes. Changing the
/// meaning of `write_canonical` requires a new `ENCODING_ID`. An unchanged ID
/// is a trusted semantic promise that Concord cannot verify. Because this
/// trait has an associated constant it is not dyn-compatible; Concord encodes
/// every dimension through the typed [`encode_canonical`] function.
///
/// Built-in V3 contracts:
///
/// | Type | `ENCODING_ID` | Payload |
/// |---|---|---|
/// | `str`, `String` | `concord.utf8/1` | tag `utf8`, UTF-8 bytes |
/// | `bool` | `concord.bool/1` | tag `bool`, one byte 0/1 |
/// | `char` | `concord.char/1` | tag `char`, scalar as u32 big-endian |
/// | `u8`..`u128`, `i8`..`i128` | `concord.<type>/1` | tag `<type>`, big-endian two's complement |
/// | `usize` / `isize` | `concord.usize/1` / `concord.isize/1` | tag `usize`/`isize`, 64-bit big-endian |
///
/// `&T`, `Box<T>`, `Arc<T>` and `Cow<'_, T>` forward to `T` with the same
/// `ENCODING_ID`.
///
/// # Extension contract
///
/// * **Ownership.** The value and the core-owned [`CanonicalKeyWriter`] are
///   borrowed for one call; the written bytes become protected key material
///   owned by core.
/// * **Concurrency.** Snapshot values are shared, so calls for independent
///   operations may run concurrently.
/// * **Cancellation.** The call is synchronous and cannot be preempted. Keys
///   are encoded while the request head is shaped, before credential
///   acquisition, limiter admission, body preparation or dispatch.
/// * **Input sensitivity.** Values are protected: Concord never renders them
///   in plans, errors or observers, and passes the bytes only to the
///   selected [`RateLimiter`](crate::rate_limit::RateLimiter). The encoding
///   must not include credentials, process-local addresses, randomized
///   hashes or unstable debug text.
/// * **Errors.** Propagate writer failures, or reject a value that cannot be
///   encoded canonically, with a [`RateLimitError`]; core reports it as a
///   value-free rate-limit configuration error before any network work.
/// * **Resource obligations.** The writer bounds the bytes by the remaining
///   hard key maximum ([`MAX_RATE_LIMIT_KEY_BYTES`]); the complete key is
///   then checked against the selected limiter's preparation key limit
///   (1,024 bytes by default) before admission. The implementation must be
///   prompt, side-effect free and deterministic for equal semantic values.
/// * **Negative capabilities.** A key part cannot choose its label, policy,
///   window or cost, cannot see other dimensions, and cannot admit or deny a
///   request.
#[diagnostic::on_unimplemented(
    message = "[C3-RATE] `{Self}` has no canonical rate-key encoding",
    label = "this rate-key dimension needs a stable `RateKeyPart` encoding",
    note = "a custom rate-key dimension is encoded through a named, stable `RateKeyPart` contract (strings, integers, `bool` and `char` are built in); it is never an atom encoder or `Display` bridge",
    note = "write `\"label\" = value via Encoder` with an `Encoder: RateKeyEncoder<{Self}>`, or implement `RateKeyPart` for the domain type"
)]
pub trait RateKeyPart {
    /// Stable encoding contract name recorded in the policy schema.
    const ENCODING_ID: &'static str;

    /// Writes this value to Concord's finite canonical writer.
    ///
    /// # Errors
    ///
    /// Propagate any writer failure; an implementation may also reject a
    /// value it cannot encode canonically.
    fn write_canonical(&self, out: &mut CanonicalKeyWriter) -> Result<(), RateLimitError>;
}

/// Canonical rate-key encoding of a value type through a separate adapter.
///
/// This is the `via` form of a rate-key dimension: `E: RateKeyEncoder<T>`
/// encodes a `T` that has no suitable [`RateKeyPart`] implementation, or
/// selects a different stable contract for one. It is never an atom encoder
/// or `Display` adapter. The same `ENCODING_ID` rules as [`RateKeyPart`]
/// apply.
///
/// # Extension contract
///
/// * **Ownership.** Encoders are stateless marker types; Concord never
///   constructs them. The value and the writer are borrowed for one call.
/// * **Concurrency.** `write_canonical` is an associated function over shared
///   snapshot values; calls for independent operations may run concurrently.
/// * **Cancellation.** The call is synchronous and cannot be preempted; it
///   runs while the request head is shaped, before credential acquisition,
///   limiter admission, body preparation or dispatch.
/// * **Input sensitivity.** Values and the written bytes are protected key
///   material that Concord never renders; the encoding must not include
///   credentials, process-local addresses, randomized hashes or unstable
///   debug text.
/// * **Errors.** Propagate writer failures, or reject a value that cannot be
///   encoded canonically, with a [`RateLimitError`]; core reports it as a
///   value-free rate-limit configuration error before any network work.
/// * **Resource obligations.** The writer bounds the key bytes; the encoder
///   must be prompt, side-effect free and deterministic for equal semantic
///   values.
/// * **Negative capabilities.** An encoder cannot choose its label, policy,
///   window or cost, cannot see other dimensions, and cannot admit or deny a
///   request.
#[diagnostic::on_unimplemented(
    message = "[C3-RATE] `{Self}` is not a rate-key encoder for `{T}`",
    label = "`via` selects `{Self}` as the rate-key encoder of a `{T}` dimension",
    note = "implement `RateKeyEncoder<{T}>` for the encoder, or remove `via` when `{T}` itself implements `RateKeyPart`"
)]
pub trait RateKeyEncoder<T: ?Sized> {
    /// Stable encoding contract name recorded in the policy schema.
    const ENCODING_ID: &'static str;

    /// Writes `value` to Concord's finite canonical writer.
    ///
    /// # Errors
    ///
    /// Propagate any writer failure; an implementation may also reject a
    /// value it cannot encode canonically.
    fn write_canonical(value: &T, out: &mut CanonicalKeyWriter) -> Result<(), RateLimitError>;
}

/// One canonically encoded, protected rate-key dimension value.
///
/// The value retains its encoding contract name and its canonical bytes. It
/// deliberately has no `Debug` or `Display` implementation because the bytes
/// can contain operationally sensitive identifiers. Cloning shares the
/// immutable bytes.
#[derive(Clone)]
pub struct EncodedKeyPart {
    encoding_id: &'static str,
    bytes: Arc<[u8]>,
}

impl EncodedKeyPart {
    /// Returns the validated encoding contract name.
    #[must_use]
    pub const fn encoding_id(&self) -> &'static str {
        self.encoding_id
    }

    /// Returns the protected canonical bytes.
    ///
    /// Callers must protect these bytes in their own storage and logs.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Returns the number of canonical bytes.
    #[must_use]
    pub fn len(&self) -> usize {
        self.bytes.len()
    }

    /// Returns whether no canonical byte was written. Always false for a
    /// successfully encoded part.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }

    /// Consumes this part and returns a copy of its protected canonical bytes.
    #[must_use]
    pub fn into_vec(self) -> Vec<u8> {
        self.bytes.to_vec()
    }

    pub(crate) fn shared_bytes(&self) -> Arc<[u8]> {
        Arc::clone(&self.bytes)
    }
}

/// Returns whether `id` is a valid `ENCODING_ID` (nonempty ASCII graphic, at
/// most [`crate::rate_limit::MAX_RATE_KEY_ENCODING_ID_BYTES`] bytes).
///
/// Generated code may evaluate this in a constant assertion to reject an
/// invalid custom encoding contract at compile time.
#[must_use]
pub const fn is_valid_encoding_id(id: &str) -> bool {
    is_bounded_ascii_graphic(id, MAX_RATE_KEY_ENCODING_ID_BYTES)
}

pub(crate) const fn is_bounded_ascii_graphic(value: &str, limit: usize) -> bool {
    let bytes = value.as_bytes();
    if bytes.is_empty() || bytes.len() > limit {
        return false;
    }
    let mut index = 0;
    while index < bytes.len() {
        if !bytes[index].is_ascii_graphic() {
            return false;
        }
        index += 1;
    }
    true
}

/// Encodes one rate-key value through its own [`RateKeyPart`] contract.
///
/// `limit` bounds the canonical bytes and must not exceed
/// [`MAX_RATE_LIMIT_KEY_BYTES`]. The encoding ID is validated before the
/// value is written. Encoding performs no allocation beyond the retained
/// bounded bytes and no I/O.
///
/// # Errors
///
/// * [`RateLimitErrorKind::InvalidConfiguration`] for an invalid
///   `ENCODING_ID` or a limit above the hard maximum.
/// * [`RateLimitErrorKind::InvalidKey`] when the value exceeds `limit`, writes
///   nothing, or its implementation rejects it.
pub fn encode_canonical<T>(value: &T, limit: usize) -> Result<EncodedKeyPart, RateLimitError>
where
    T: RateKeyPart + ?Sized,
{
    encode_with(T::ENCODING_ID, limit, |out| value.write_canonical(out))
}

/// Encodes one rate-key value through an explicit [`RateKeyEncoder`].
///
/// Identical to [`encode_canonical`] except that the encoding contract and
/// bytes come from `E`.
///
/// # Errors
///
/// As for [`encode_canonical`].
pub fn encode_canonical_with<E, T>(
    value: &T,
    limit: usize,
) -> Result<EncodedKeyPart, RateLimitError>
where
    E: RateKeyEncoder<T>,
    T: ?Sized,
{
    encode_with(E::ENCODING_ID, limit, |out| E::write_canonical(value, out))
}

fn encode_with<F>(
    encoding_id: &'static str,
    limit: usize,
    write: F,
) -> Result<EncodedKeyPart, RateLimitError>
where
    F: FnOnce(&mut CanonicalKeyWriter) -> Result<(), RateLimitError>,
{
    if !is_valid_encoding_id(encoding_id) {
        return Err(invalid_configuration());
    }
    let mut writer = CanonicalKeyWriter::new(limit)?;
    write(&mut writer)?;
    if writer.bytes.is_empty() {
        return Err(invalid_key());
    }
    Ok(EncodedKeyPart {
        encoding_id,
        bytes: writer.into_bytes(),
    })
}

impl<T> RateKeyPart for &T
where
    T: RateKeyPart + ?Sized,
{
    const ENCODING_ID: &'static str = T::ENCODING_ID;

    fn write_canonical(&self, out: &mut CanonicalKeyWriter) -> Result<(), RateLimitError> {
        (**self).write_canonical(out)
    }
}

impl<T> RateKeyPart for Box<T>
where
    T: RateKeyPart + ?Sized,
{
    const ENCODING_ID: &'static str = T::ENCODING_ID;

    fn write_canonical(&self, out: &mut CanonicalKeyWriter) -> Result<(), RateLimitError> {
        (**self).write_canonical(out)
    }
}

impl<T> RateKeyPart for Arc<T>
where
    T: RateKeyPart + ?Sized,
{
    const ENCODING_ID: &'static str = T::ENCODING_ID;

    fn write_canonical(&self, out: &mut CanonicalKeyWriter) -> Result<(), RateLimitError> {
        (**self).write_canonical(out)
    }
}

impl<T> RateKeyPart for Cow<'_, T>
where
    T: RateKeyPart + ToOwned + ?Sized,
{
    const ENCODING_ID: &'static str = T::ENCODING_ID;

    fn write_canonical(&self, out: &mut CanonicalKeyWriter) -> Result<(), RateLimitError> {
        self.as_ref().write_canonical(out)
    }
}

impl RateKeyPart for str {
    const ENCODING_ID: &'static str = UTF8_ENCODING_ID;

    fn write_canonical(&self, out: &mut CanonicalKeyWriter) -> Result<(), RateLimitError> {
        out.write_tagged_bytes("utf8", self.as_bytes())
    }
}

impl RateKeyPart for String {
    const ENCODING_ID: &'static str = UTF8_ENCODING_ID;

    fn write_canonical(&self, out: &mut CanonicalKeyWriter) -> Result<(), RateLimitError> {
        self.as_str().write_canonical(out)
    }
}

impl RateKeyPart for bool {
    const ENCODING_ID: &'static str = "concord.bool/1";

    fn write_canonical(&self, out: &mut CanonicalKeyWriter) -> Result<(), RateLimitError> {
        out.write_tagged_bytes("bool", &[u8::from(*self)])
    }
}

impl RateKeyPart for char {
    const ENCODING_ID: &'static str = "concord.char/1";

    fn write_canonical(&self, out: &mut CanonicalKeyWriter) -> Result<(), RateLimitError> {
        out.write_tagged_bytes("char", &u32::from(*self).to_be_bytes())
    }
}

macro_rules! integer_impl {
    ($($type:ty => $tag:literal, $id:literal),* $(,)?) => {
        $(
            impl RateKeyPart for $type {
                const ENCODING_ID: &'static str = $id;

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
    u8 => "u8", "concord.u8/1",
    u16 => "u16", "concord.u16/1",
    u32 => "u32", "concord.u32/1",
    u64 => "u64", "concord.u64/1",
    u128 => "u128", "concord.u128/1",
    i8 => "i8", "concord.i8/1",
    i16 => "i16", "concord.i16/1",
    i32 => "i32", "concord.i32/1",
    i64 => "i64", "concord.i64/1",
    i128 => "i128", "concord.i128/1",
}

impl RateKeyPart for usize {
    const ENCODING_ID: &'static str = "concord.usize/1";

    fn write_canonical(&self, out: &mut CanonicalKeyWriter) -> Result<(), RateLimitError> {
        // Baseline 64-bit canonical conversion: identical bytes on every
        // supported target width.
        let value = u64::try_from(*self).map_err(|_| invalid_key())?;
        out.write_tagged_bytes("usize", &value.to_be_bytes())
    }
}

impl RateKeyPart for isize {
    const ENCODING_ID: &'static str = "concord.isize/1";

    fn write_canonical(&self, out: &mut CanonicalKeyWriter) -> Result<(), RateLimitError> {
        let value = i64::try_from(*self).map_err(|_| invalid_key())?;
        out.write_tagged_bytes("isize", &value.to_be_bytes())
    }
}

fn invalid_key() -> RateLimitError {
    RateLimitError::new(RateLimitErrorKind::InvalidKey)
}

fn invalid_configuration() -> RateLimitError {
    RateLimitError::new(RateLimitErrorKind::InvalidConfiguration)
}

#[cfg(test)]
mod tests;
