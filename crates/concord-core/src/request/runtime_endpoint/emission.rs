//! Owned, type-erased runtime values read by the shared symbolic evaluator.
//!
//! A runtime endpoint stores each caller value exactly as supplied (scalar,
//! outer-optional or ordered sequence) behind one of these erased slots. The
//! slots are only *read*: [`RuntimeEmission::present`] inspects the outer
//! slot and [`RuntimeEmission::emit`] encodes surviving occurrences when the
//! shared [`apply_head_program`](crate::request::apply_head_program)
//! evaluator selects them at plan or execution time. Construction and binding
//! never call an encoder, adapter or iterator.

use std::marker::PhantomData;

use crate::atom::{AtomEncoder, AtomError, BoundedAtomWriter, OrderedValues, RequestAtom};
use crate::request::{HeadEmissions, OccurrenceSink};

/// How one scalar atom is encoded: canonical [`RequestAtom`] or an explicit
/// [`AtomEncoder`] adapter.
pub(crate) trait ScalarEncoding<T: ?Sized>: 'static {
    /// The adapter's type name for value-free facts; `None` for canonical.
    fn adapter() -> Option<&'static str>;

    fn write(value: &T, writer: &mut BoundedAtomWriter<'_>) -> Result<(), AtomError>;
}

/// The value's own [`RequestAtom`] implementation.
pub(crate) struct Canonical;

/// An explicit per-atom [`AtomEncoder`] adapter.
pub(crate) struct Via<A>(PhantomData<fn() -> A>);

impl<T> ScalarEncoding<T> for Canonical
where
    T: RequestAtom + ?Sized,
{
    fn adapter() -> Option<&'static str> {
        None
    }

    fn write(value: &T, writer: &mut BoundedAtomWriter<'_>) -> Result<(), AtomError> {
        writer.write(value)
    }
}

impl<T, A> ScalarEncoding<T> for Via<A>
where
    T: ?Sized,
    A: AtomEncoder<T> + 'static,
{
    fn adapter() -> Option<&'static str> {
        Some(std::any::type_name::<A>())
    }

    fn write(value: &T, writer: &mut BoundedAtomWriter<'_>) -> Result<(), AtomError> {
        writer.write_with::<A, T>(value)
    }
}

/// One erased keyed emission source.
pub(crate) trait RuntimeEmission: Send + Sync {
    /// Reads the outer slot only; never asks an adapter.
    fn present(&self) -> bool;

    /// Encodes the surviving occurrences (one per scalar, one per item of a
    /// present sequence) into the bounded sink, stopping at the first error.
    fn emit(&self, sink: &mut OccurrenceSink<'_>) -> Result<(), AtomError>;
}

/// A scalar (`Some(value)`) or outer-optional scalar source.
pub(crate) struct ScalarSlot<T, E> {
    value: Option<T>,
    encoding: PhantomData<fn() -> E>,
}

impl<T, E> ScalarSlot<T, E> {
    pub(crate) const fn new(value: Option<T>) -> Self {
        Self {
            value,
            encoding: PhantomData,
        }
    }
}

impl<T, E> RuntimeEmission for ScalarSlot<T, E>
where
    T: Send + Sync + 'static,
    E: ScalarEncoding<T>,
{
    fn present(&self) -> bool {
        self.value.is_some()
    }

    fn emit(&self, sink: &mut OccurrenceSink<'_>) -> Result<(), AtomError> {
        match &self.value {
            Some(value) => sink.atom(|writer| E::write(value, writer)),
            None => Ok(()),
        }
    }
}

/// An ordered `each` sequence (`Some(values)`) or outer-optional sequence.
pub(crate) struct EachSlot<V, E> {
    values: Option<V>,
    encoding: PhantomData<fn() -> E>,
}

impl<V, E> EachSlot<V, E> {
    pub(crate) const fn new(values: Option<V>) -> Self {
        Self {
            values,
            encoding: PhantomData,
        }
    }
}

impl<V, E> RuntimeEmission for EachSlot<V, E>
where
    V: OrderedValues + Send + Sync + 'static,
    E: ScalarEncoding<V::Item>,
{
    fn present(&self) -> bool {
        self.values.is_some()
    }

    fn emit(&self, sink: &mut OccurrenceSink<'_>) -> Result<(), AtomError> {
        if let Some(values) = &self.values {
            // Each item is encoded once, in order; the first encoding or
            // head-limit failure stops iteration before a later item is read.
            for value in values.ordered_values() {
                sink.atom(|writer| E::write(value, writer))?;
            }
        }
        Ok(())
    }
}

/// The erased emission table of one runtime program segment.
#[derive(Default)]
pub(crate) struct EmissionTable {
    slots: Vec<Box<dyn RuntimeEmission>>,
}

impl EmissionTable {
    pub(crate) fn len(&self) -> usize {
        self.slots.len()
    }

    pub(crate) fn push(&mut self, slot: Box<dyn RuntimeEmission>) {
        self.slots.push(slot);
    }
}

impl HeadEmissions for EmissionTable {
    fn emission_present(&self, emission: u16) -> bool {
        self.slots
            .get(usize::from(emission))
            .is_some_and(|slot| slot.present())
    }

    fn emit(&self, emission: u16, sink: &mut OccurrenceSink<'_>) -> Result<(), AtomError> {
        match self.slots.get(usize::from(emission)) {
            Some(slot) => slot.emit(sink),
            None => Ok(()),
        }
    }
}

/// One erased runtime path atom (exactly one generated path segment).
pub(crate) trait RuntimePathAtom: Send + Sync {
    fn write(&self, writer: &mut BoundedAtomWriter<'_>) -> Result<(), AtomError>;
}

/// An owned path value and its encoding.
pub(crate) struct PathSlot<T, E> {
    value: T,
    encoding: PhantomData<fn() -> E>,
}

impl<T, E> PathSlot<T, E> {
    pub(crate) const fn new(value: T) -> Self {
        Self {
            value,
            encoding: PhantomData,
        }
    }
}

impl<T, E> RuntimePathAtom for PathSlot<T, E>
where
    T: Send + Sync + 'static,
    E: ScalarEncoding<T>,
{
    fn write(&self, writer: &mut BoundedAtomWriter<'_>) -> Result<(), AtomError> {
        E::write(&self.value, writer)
    }
}
