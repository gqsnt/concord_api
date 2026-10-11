use std::fmt;
use std::fmt::Write as _;
use std::num::{
    NonZeroI8, NonZeroI16, NonZeroI32, NonZeroI64, NonZeroI128, NonZeroIsize, NonZeroU8,
    NonZeroU16, NonZeroU32, NonZeroU64, NonZeroU128, NonZeroUsize,
};

/// Stable category for a public atom encoding failure.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AtomErrorKind {
    /// The atom exceeded the core-owned byte limit.
    Limit,
    /// The value could not produce its declared canonical atom.
    Encode,
}

/// Value-free failure from [`RequestAtom`] or [`AtomEncoder`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AtomError {
    kind: AtomErrorKind,
}

impl AtomError {
    /// Creates a value-free atom error in a stable category.
    #[must_use]
    pub const fn new(kind: AtomErrorKind) -> Self {
        Self { kind }
    }

    /// Returns the stable failure category.
    #[must_use]
    pub const fn kind(self) -> AtomErrorKind {
        self.kind
    }
}

impl fmt::Display for AtomError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self.kind {
            AtomErrorKind::Limit => "request atom exceeded its configured limit",
            AtomErrorKind::Encode => "request atom encoding failed",
        };
        formatter.write_str(message)
    }
}

impl std::error::Error for AtomError {}

/// Core-bounded destination supplied to public atom encoders.
///
/// The writer never retains bytes from a write that would cross the limit.
/// Extensions receive no placement, request, credential, or native transport
/// authority through this type.
pub struct BoundedAtomWriter<'a> {
    output: &'a mut String,
    limit: usize,
}

impl<'a> BoundedAtomWriter<'a> {
    pub(crate) fn new(output: &'a mut String, limit: usize) -> Self {
        Self { output, limit }
    }

    /// Appends UTF-8 text if the complete write fits the remaining bound.
    pub fn push_str(&mut self, value: &str) -> Result<(), AtomError> {
        let next = self
            .output
            .len()
            .checked_add(value.len())
            .ok_or_else(|| AtomError::new(AtomErrorKind::Limit))?;
        if next > self.limit {
            return Err(AtomError::new(AtomErrorKind::Limit));
        }
        self.output.push_str(value);
        Ok(())
    }

    /// Appends one Unicode scalar if its complete UTF-8 encoding fits.
    pub fn push_char(&mut self, value: char) -> Result<(), AtomError> {
        let mut encoded = [0_u8; 4];
        self.push_str(value.encode_utf8(&mut encoded))
    }

    /// Returns the number of bytes written so far.
    #[must_use]
    pub fn len(&self) -> usize {
        self.output.len()
    }

    /// Returns whether no bytes have been written.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.output.is_empty()
    }

    /// Returns the remaining core-owned byte capacity.
    #[must_use]
    pub fn remaining(&self) -> usize {
        self.limit.saturating_sub(self.output.len())
    }

    /// Writes one value through its canonical [`RequestAtom`] implementation.
    pub fn write<T>(&mut self, value: &T) -> Result<(), AtomError>
    where
        T: RequestAtom + ?Sized,
    {
        value.write_atom(self)
    }

    /// Writes one value through an explicit adapter.
    pub fn write_with<A, T>(&mut self, value: &T) -> Result<(), AtomError>
    where
        A: AtomEncoder<T>,
        T: ?Sized,
    {
        A::write(value, self)
    }

    /// Writes one value through an explicitly selected [`fmt::Display`]
    /// compatibility adapter while preserving this writer's byte bound.
    ///
    /// This does not make `Display` a protocol default. It exists so a named
    /// [`AtomEncoder`] can deliberately opt one placement into display
    /// formatting.
    pub fn write_display<T>(&mut self, value: &T) -> Result<(), AtomError>
    where
        T: fmt::Display + ?Sized,
    {
        struct DisplayWriter<'writer, 'output> {
            writer: &'writer mut BoundedAtomWriter<'output>,
            failure: Option<AtomError>,
        }

        impl fmt::Write for DisplayWriter<'_, '_> {
            fn write_str(&mut self, value: &str) -> fmt::Result {
                self.writer.push_str(value).map_err(|error| {
                    self.failure = Some(error);
                    fmt::Error
                })
            }
        }

        let mut adapter = DisplayWriter {
            writer: self,
            failure: None,
        };
        if write!(&mut adapter, "{value}").is_err() {
            return Err(adapter
                .failure
                .unwrap_or_else(|| AtomError::new(AtomErrorKind::Encode)));
        }
        Ok(())
    }
}

/// Owns one deterministic canonical public protocol atom for a value.
///
/// Core calls `write_atom` while it shapes a request head (planning, each
/// executed operation and each traversal page): for every route atom and for
/// each query/header emission that survives the authored program.
///
/// # Extension contract
///
/// * **Ownership.** The value is borrowed from a request, scope or client
///   snapshot for the duration of one call; the writer is borrowed from core.
///   Nothing may be retained past the call.
/// * **Concurrency.** Snapshot values are shared, so calls for independent
///   operations may run concurrently on different threads.
/// * **Cancellation.** The call is synchronous and cannot be preempted. It
///   runs before credential, rate-limit, body or transport work; dropping the
///   operation afterwards only prevents that later work.
/// * **Input sensitivity.** The value may be `#[sensitive]`: core redacts the
///   written atom in plans, errors and observers, but the implementation sees
///   the plain value and must not log or store it.
/// * **Errors.** Return a value-free [`AtomError`] ([`AtomErrorKind::Limit`]
///   when the writer rejects a write). Core discards the partial atom and
///   fails the request with a value-free configuration error before any
///   network work.
/// * **Resource obligations.** Output is bounded by the writer (at most
///   65,536 bytes per atom); implementations must keep any temporary
///   allocation and CPU work prompt, finite, side-effect-free and
///   deterministic for an unchanged semantic value.
/// * **Negative capabilities.** This role does not grant omission,
///   repetition, placement escaping, rate-key, body, retry, credential or
///   native request authority.
#[diagnostic::on_unimplemented(
    message = "[C3-ATOM] `{Self}` is not a scalar request atom",
    label = "this placement writes exactly one scalar atom",
    note = "`RequestAtom` owns one scalar path, DNS, query or header value; a sequence is never repeated implicitly",
    note = "for one query/header occurrence per item write `\"key\" = each value;`; for one scalar write `value via Encoder` with an `AtomEncoder<{Self}>`, or implement `RequestAtom` for a domain type"
)]
pub trait RequestAtom {
    /// Writes exactly one logical atom into a core-bounded destination.
    fn write_atom(&self, output: &mut BoundedAtomWriter<'_>) -> Result<(), AtomError>;
}

/// Explicitly adapts a value into one public protocol atom.
///
/// The adapter changes representation only. The authored placement retains
/// ownership of optionality, occurrence cardinality, and context validation.
/// Core calls `write` at the same points as [`RequestAtom::write_atom`]: while
/// shaping a request head, for route atoms and surviving query/header
/// emissions.
///
/// # Extension contract
///
/// * **Ownership.** Encoders are stateless marker types that Concord never
///   constructs; the value and the writer are borrowed for one call and
///   nothing may be retained.
/// * **Concurrency.** `write` is an associated function over shared snapshot
///   values; calls for independent operations may run concurrently.
/// * **Cancellation.** The call is synchronous and cannot be preempted; it
///   runs before credential, rate-limit, body or transport work.
/// * **Input sensitivity.** The value may be `#[sensitive]`; core redacts the
///   result in automatic output, but the encoder sees the plain value and
///   must not log or store it.
/// * **Errors.** Return a value-free [`AtomError`]; core discards the partial
///   atom and fails the request with a value-free configuration error before
///   any network work.
/// * **Resource obligations.** Output is bounded by the writer (at most
///   65,536 bytes per atom); temporary allocation and CPU work must stay
///   prompt, finite, side-effect-free and deterministic.
/// * **Negative capabilities.** An encoder cannot choose presence,
///   repetition, the native URL, escaping, the body, retries or credentials.
#[diagnostic::on_unimplemented(
    message = "[C3-ATOM] `{Self}` is not an atom encoder for `{T}`",
    label = "`via` selects `{Self}` as the atom adapter of a `{T}` value",
    note = "an atom adapter implements `AtomEncoder<{T}>`; it is not a body representation, rate-key encoder or `Display` bridge",
    note = "implement `AtomEncoder<{T}>` for the adapter, or remove `via` when `{T}` itself implements `RequestAtom`"
)]
pub trait AtomEncoder<T>
where
    T: ?Sized,
{
    /// Writes the adapter's one deterministic atom.
    fn write(value: &T, output: &mut BoundedAtomWriter<'_>) -> Result<(), AtomError>;
}

/// Supplies stable by-reference order for an authored `each` emission.
///
/// Core iterates `ordered_values` while shaping a request head and encodes
/// each item, in iteration order, as one occurrence of the authored key; an
/// empty sequence emits no occurrence.
///
/// # Extension contract
///
/// * **Ownership.** The collection is borrowed from a snapshot; items are
///   yielded by reference and never moved or retained by core.
/// * **Concurrency.** Snapshot values are shared, so independent operations
///   may iterate the same collection concurrently.
/// * **Cancellation.** Iteration is synchronous and cannot be preempted; it
///   runs before credential, rate-limit, body or transport work.
/// * **Input sensitivity.** Items may be `#[sensitive]`; core redacts their
///   occurrences in automatic output. Iteration itself must not log them.
/// * **Errors.** Iteration cannot fail. Core stops consuming the iterator at
///   the first item encoding failure or public head limit and fails the
///   request with a value-free configuration error.
/// * **Resource obligations.** The iterator must be finite and deterministic
///   for an unchanged value; core bounds the resulting occurrences by the
///   public head component, header count and byte limits.
/// * **Negative capabilities.** The order is the only authority: this trait
///   cannot emit occurrences itself or choose presence or keys, and an
///   implementation must not present an unordered (for example hash-map)
///   iteration order as protocol order.
#[diagnostic::on_unimplemented(
    message = "[C3-ATOM] `{Self}` is not a stable ordered sequence for `each`",
    label = "`each` iterates this source by reference in a stable order",
    note = "`each` requires `OrderedValues` (slices, arrays, `Vec` and other ordered collections); an unordered collection needs a deterministic ordered collection or adapter",
    note = "to emit one scalar occurrence instead, remove `each` and place the value directly (with `via Encoder` when it needs an adapter)"
)]
pub trait OrderedValues {
    /// Exact item type exposed to the selected atom encoder.
    type Item;

    /// Borrowing iterator that preserves semantic order.
    type Iter<'a>: Iterator<Item = &'a Self::Item>
    where
        Self: 'a,
        Self::Item: 'a;

    /// Iterates values in their declared stable order.
    fn ordered_values(&self) -> Self::Iter<'_>;
}

impl<T> RequestAtom for &T
where
    T: RequestAtom + ?Sized,
{
    fn write_atom(&self, output: &mut BoundedAtomWriter<'_>) -> Result<(), AtomError> {
        T::write_atom(*self, output)
    }
}

impl RequestAtom for str {
    fn write_atom(&self, output: &mut BoundedAtomWriter<'_>) -> Result<(), AtomError> {
        output.push_str(self)
    }
}

impl RequestAtom for String {
    fn write_atom(&self, output: &mut BoundedAtomWriter<'_>) -> Result<(), AtomError> {
        output.push_str(self)
    }
}

impl RequestAtom for char {
    fn write_atom(&self, output: &mut BoundedAtomWriter<'_>) -> Result<(), AtomError> {
        output.push_char(*self)
    }
}

impl RequestAtom for bool {
    fn write_atom(&self, output: &mut BoundedAtomWriter<'_>) -> Result<(), AtomError> {
        output.push_str(if *self { "true" } else { "false" })
    }
}

macro_rules! integer_atoms {
    ($($type:ty),+ $(,)?) => {
        $(
            impl RequestAtom for $type {
                fn write_atom(
                    &self,
                    output: &mut BoundedAtomWriter<'_>,
                ) -> Result<(), AtomError> {
                    output.push_str(&self.to_string())
                }
            }
        )+
    };
}

integer_atoms!(
    i8,
    i16,
    i32,
    i64,
    i128,
    isize,
    u8,
    u16,
    u32,
    u64,
    u128,
    usize,
    NonZeroI8,
    NonZeroI16,
    NonZeroI32,
    NonZeroI64,
    NonZeroI128,
    NonZeroIsize,
    NonZeroU8,
    NonZeroU16,
    NonZeroU32,
    NonZeroU64,
    NonZeroU128,
    NonZeroUsize,
);

impl<T> OrderedValues for Vec<T> {
    type Item = T;
    type Iter<'a>
        = std::slice::Iter<'a, T>
    where
        T: 'a;

    fn ordered_values(&self) -> Self::Iter<'_> {
        self.iter()
    }
}

impl<T, const N: usize> OrderedValues for [T; N] {
    type Item = T;
    type Iter<'a>
        = std::slice::Iter<'a, T>
    where
        T: 'a;

    fn ordered_values(&self) -> Self::Iter<'_> {
        self.iter()
    }
}

impl<T> OrderedValues for [T] {
    type Item = T;
    type Iter<'a>
        = std::slice::Iter<'a, T>
    where
        T: 'a;

    fn ordered_values(&self) -> Self::Iter<'_> {
        self.iter()
    }
}

impl<T> OrderedValues for Box<[T]> {
    type Item = T;
    type Iter<'a>
        = std::slice::Iter<'a, T>
    where
        T: 'a;

    fn ordered_values(&self) -> Self::Iter<'_> {
        self.iter()
    }
}

pub(crate) fn encode_atom<T>(value: &T, limit: usize) -> Result<String, AtomError>
where
    T: RequestAtom + ?Sized,
{
    let mut output = String::new();
    value.write_atom(&mut BoundedAtomWriter::new(&mut output, limit))?;
    Ok(output)
}

pub(crate) fn encode_atom_with<A, T>(value: &T, limit: usize) -> Result<String, AtomError>
where
    A: AtomEncoder<T>,
    T: ?Sized,
{
    let mut output = String::new();
    A::write(value, &mut BoundedAtomWriter::new(&mut output, limit))?;
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtins_are_exact_and_first_excess_is_atomic() {
        assert_eq!(encode_atom(&-42_i16, 3).expect("integer atom"), "-42");
        assert_eq!(encode_atom(&true, 4).expect("boolean atom"), "true");
        assert_eq!(encode_atom(&'é', 2).expect("character atom"), "é");

        let mut retained = String::from("ok");
        let mut writer = BoundedAtomWriter::new(&mut retained, 3);
        let error = writer.push_str("xx").expect_err("first excess");
        assert_eq!(error.kind(), AtomErrorKind::Limit);
        assert_eq!(retained, "ok");
    }

    #[test]
    fn ordered_values_cover_only_deliberate_ordered_sequences() {
        fn collect<V>(values: &V) -> Vec<&V::Item>
        where
            V: OrderedValues + ?Sized,
        {
            values.ordered_values().collect()
        }

        assert_eq!(collect(&vec![1, 2, 3]), vec![&1, &2, &3]);
        assert_eq!(collect(&[4, 5]), vec![&4, &5]);
        assert_eq!(collect(&Box::<[u8]>::from([6, 7])), vec![&6, &7]);
    }
}
