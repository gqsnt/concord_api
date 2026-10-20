//! Symbolic query/header patch programs (PATCH-01..03, PRIVACY-01).
//!
//! A request's public query and header families are described as one ordered
//! program of operations collected from the root owner, every scope owner, the
//! endpoint (or runtime endpoint) in lexical order. Core evaluates the program
//! **before** invoking any encoder: outer presence of each emission source is
//! read first, every structural operation (introduction, exact replacement,
//! conditional replacement, append, removal and family clear) is applied to a
//! symbolic occurrence list, and only the surviving emissions are then encoded
//! in their final order. A suppressed ancestor emission therefore never runs
//! its encoder or adapter, and can never fail the request.
//!
//! Sensitivity is sticky per target key: if any operation anywhere in the
//! program (including one later removed, replaced or cleared by a family
//! fence) carried a sensitive emission for a key, every surviving occurrence
//! of that key is redacted in automatic observation.

use std::collections::HashSet;

use super::HeadBuilder;
use crate::atom::{AtomError, BoundedAtomWriter};
use crate::error::ApiError;
use crate::limits::MAX_FORMATTED_PUBLIC_VALUE_BYTES;

/// The request family a patch operation targets.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PatchFamily {
    /// Public query occurrences (case-sensitive keys).
    Query,
    /// Public header occurrences (case-insensitive names).
    Header,
}

/// One structural patch operation.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PatchKind {
    /// Plain introduction (structurally checked: no earlier potential owner).
    Introduce,
    /// Exact replacement: removes earlier occurrences, emits when present.
    Replace,
    /// Conditional replacement: absent keeps earlier occurrences; present
    /// (including present-empty `each`) removes them and emits.
    WhenPresent,
    /// Appends occurrences when present.
    Append,
    /// Removes every earlier occurrence of the key.
    Remove,
    /// Family fence (`query replace { .. }`, `query off`): clears every earlier
    /// occurrence of the family.
    Clear,
}

/// Marker for operations that carry no emission.
#[doc(hidden)]
pub const NO_EMISSION: u16 = u16::MAX;

/// One operation of a patch program.
#[doc(hidden)]
#[derive(Clone, Copy, Debug)]
pub struct PatchOp<'a> {
    /// Target family.
    pub family: PatchFamily,
    /// Structural operation.
    pub kind: PatchKind,
    /// Static or runtime-supplied key; empty for [`PatchKind::Clear`].
    pub key: &'a str,
    /// Emission identity resolved by the segment's [`HeadEmissions`].
    pub emission: u16,
    /// Whether this operation's emission is classified sensitive.
    pub sensitive: bool,
}

impl<'a> PatchOp<'a> {
    /// A keyed operation with an emission.
    #[must_use]
    pub const fn keyed(
        family: PatchFamily,
        kind: PatchKind,
        key: &'a str,
        emission: u16,
        sensitive: bool,
    ) -> Self {
        Self {
            family,
            kind,
            key,
            emission,
            sensitive,
        }
    }

    /// A removal of every earlier occurrence of `key`.
    #[must_use]
    pub const fn remove(family: PatchFamily, key: &'a str) -> Self {
        Self {
            family,
            kind: PatchKind::Remove,
            key,
            emission: NO_EMISSION,
            sensitive: false,
        }
    }

    /// A family fence clearing all earlier occurrences.
    #[must_use]
    pub const fn clear(family: PatchFamily) -> Self {
        Self {
            family,
            kind: PatchKind::Clear,
            key: "",
            emission: NO_EMISSION,
            sensitive: false,
        }
    }
}

/// Value readers for the emissions referenced by one program segment.
///
/// Implemented by generated endpoints and owners (with a `match` over their
/// emission identities) and by runtime endpoints. `emission_present` reads the
/// selected outer slot only; it never asks an adapter whether it has a value.
#[doc(hidden)]
pub trait HeadEmissions {
    /// Returns whether the emission's outer source is present.
    fn emission_present(&self, emission: u16) -> bool;

    /// Encodes the emission's occurrences (one for a scalar, zero or more for
    /// `each`) into the sink. Called only for surviving emissions.
    fn emit(&self, emission: u16, sink: &mut OccurrenceSink<'_>) -> Result<(), AtomError>;
}

/// One contiguous part of a program together with its value readers.
#[doc(hidden)]
#[derive(Clone, Copy)]
pub struct ProgramSegment<'a> {
    /// Operations in lexical order.
    pub ops: &'a [PatchOp<'a>],
    /// Readers for the operations' emissions.
    pub emissions: &'a dyn HeadEmissions,
}

/// Receives the encoded occurrences of one surviving emission and forwards
/// each one to the head builder immediately (bounded; first error latched).
#[doc(hidden)]
pub struct OccurrenceSink<'a> {
    builder: &'a mut HeadBuilder,
    family: PatchFamily,
    key: &'a str,
    redacted: bool,
    error: Option<ApiError>,
}

impl OccurrenceSink<'_> {
    /// Encodes one occurrence in a single bounded pass and records it.
    pub fn atom<F>(&mut self, encode: F) -> Result<(), AtomError>
    where
        F: FnOnce(&mut BoundedAtomWriter<'_>) -> Result<(), AtomError>,
    {
        if self.error.is_some() {
            return Err(AtomError::new(crate::atom::AtomErrorKind::Limit));
        }
        let mut value = String::new();
        {
            let mut writer = BoundedAtomWriter::new(&mut value, MAX_FORMATTED_PUBLIC_VALUE_BYTES);
            encode(&mut writer)?;
        }
        let pushed = match self.family {
            PatchFamily::Query => {
                self.builder
                    .push_prepared_query_value(self.key, &value, self.redacted)
            }
            PatchFamily::Header => {
                self.builder
                    .append_prepared_header_value(self.key, &value, self.redacted)
            }
        };
        match pushed {
            Ok(()) => Ok(()),
            Err(error) => {
                self.error = Some(error);
                Err(AtomError::new(crate::atom::AtomErrorKind::Limit))
            }
        }
    }

    /// Encodes one scalar value with its `RequestAtom` implementation.
    pub fn value<T>(&mut self, value: &T) -> Result<(), AtomError>
    where
        T: crate::atom::RequestAtom + ?Sized,
    {
        self.atom(|writer| writer.write(value))
    }

    /// Encodes one scalar value with an explicit adapter.
    pub fn value_with<A, T>(&mut self, value: &T) -> Result<(), AtomError>
    where
        A: crate::atom::AtomEncoder<T>,
        T: ?Sized,
    {
        self.atom(|writer| writer.write_with::<A, T>(value))
    }

    /// Encodes every item of an ordered sequence as one occurrence each.
    pub fn each<V>(&mut self, values: &V) -> Result<(), AtomError>
    where
        V: crate::atom::OrderedValues + ?Sized,
        V::Item: crate::atom::RequestAtom,
    {
        for value in values.ordered_values() {
            self.value(value)?;
        }
        Ok(())
    }

    /// Encodes every item of an ordered sequence with a per-item adapter.
    pub fn each_with<A, V>(&mut self, values: &V) -> Result<(), AtomError>
    where
        V: crate::atom::OrderedValues + ?Sized,
        A: crate::atom::AtomEncoder<V::Item>,
    {
        for value in values.ordered_values() {
            self.value_with::<A, V::Item>(value)?;
        }
        Ok(())
    }
}

fn normalized_key(family: PatchFamily, key: &str) -> String {
    match family {
        PatchFamily::Query => key.to_owned(),
        PatchFamily::Header => key.to_ascii_lowercase(),
    }
}

fn same_key(family: PatchFamily, left: &str, right: &str) -> bool {
    match family {
        PatchFamily::Query => left == right,
        PatchFamily::Header => left.eq_ignore_ascii_case(right),
    }
}

/// A surviving emission: (segment index, operation index).
type Survivor = (usize, usize);

/// Survivors kept inline before the selection spills to the heap. Typical
/// generated heads have a handful of query/header occurrences per family, so
/// selection normally allocates nothing (V3-O07 attribution: the former
/// per-family `Vec` was an avoidable allocation on every prepared head).
const INLINE_SURVIVORS: usize = 16;

/// The ordered survivor list of one family: inline up to
/// [`INLINE_SURVIVORS`] entries, then a `Vec`. Order and retain semantics are
/// exactly those of a `Vec`.
// The large inline variant is the point: the value lives on the stack of one
// head preparation, and boxing it would reintroduce the allocation.
#[allow(clippy::large_enum_variant)]
enum Survivors {
    Inline {
        items: [Survivor; INLINE_SURVIVORS],
        len: usize,
    },
    Heap(Vec<Survivor>),
}

impl Survivors {
    const fn new() -> Self {
        Self::Inline {
            items: [(0, 0); INLINE_SURVIVORS],
            len: 0,
        }
    }

    fn as_slice(&self) -> &[Survivor] {
        match self {
            Self::Inline { items, len } => &items[..*len],
            Self::Heap(items) => items,
        }
    }

    fn push(&mut self, survivor: Survivor) {
        match self {
            Self::Inline { items, len } if *len < INLINE_SURVIVORS => {
                items[*len] = survivor;
                *len += 1;
            }
            Self::Inline { items, len } => {
                let mut heap = Vec::with_capacity(INLINE_SURVIVORS * 2);
                heap.extend_from_slice(&items[..*len]);
                heap.push(survivor);
                *self = Self::Heap(heap);
            }
            Self::Heap(items) => items.push(survivor),
        }
    }

    fn retain(&mut self, mut keep: impl FnMut(&Survivor) -> bool) {
        match self {
            Self::Inline { items, len } => {
                let mut kept = 0;
                for index in 0..*len {
                    if keep(&items[index]) {
                        items[kept] = items[index];
                        kept += 1;
                    }
                }
                *len = kept;
            }
            Self::Heap(items) => items.retain(keep),
        }
    }

    fn clear(&mut self) {
        match self {
            Self::Inline { len, .. } => *len = 0,
            Self::Heap(items) => items.clear(),
        }
    }
}

/// Selects the surviving emissions of one family without encoding anything.
fn select(segments: &[ProgramSegment<'_>], family: PatchFamily) -> Vec<Survivor> {
    let mut survivors = Survivors::new();
    select_into(segments, family, &mut survivors);
    survivors.as_slice().to_vec()
}

/// Selects the surviving emissions of one family into `survivors` (cleared
/// first) without encoding anything.
fn select_into(segments: &[ProgramSegment<'_>], family: PatchFamily, survivors: &mut Survivors) {
    survivors.clear();
    for (segment_index, segment) in segments.iter().enumerate() {
        for (op_index, op) in segment.ops.iter().enumerate() {
            if op.family != family {
                continue;
            }
            let present =
                || op.emission != NO_EMISSION && segment.emissions.emission_present(op.emission);
            let remove_key = |survivors: &mut Survivors| {
                survivors.retain(|(s, o)| !same_key(family, segments[*s].ops[*o].key, op.key));
            };
            match op.kind {
                PatchKind::Clear => survivors.clear(),
                PatchKind::Introduce | PatchKind::Append => {
                    if present() {
                        survivors.push((segment_index, op_index));
                    }
                }
                PatchKind::Replace => {
                    remove_key(survivors);
                    if present() {
                        survivors.push((segment_index, op_index));
                    }
                }
                PatchKind::WhenPresent => {
                    if present() {
                        remove_key(survivors);
                        survivors.push((segment_index, op_index));
                    }
                }
                PatchKind::Remove => remove_key(survivors),
            }
        }
    }
}

/// Keys that carried a sensitive contribution anywhere in the program.
///
/// Empty (and allocation-free) when no operation of the family is
/// sensitive.
fn sticky_sensitive(segments: &[ProgramSegment<'_>], family: PatchFamily) -> HashSet<String> {
    segments
        .iter()
        .flat_map(|segment| segment.ops.iter())
        .filter(|op| op.family == family && op.sensitive && op.kind != PatchKind::Clear)
        .map(|op| normalized_key(family, op.key))
        .collect()
}

/// Evaluates a complete program and encodes only the surviving emissions, in
/// their final order, into the head builder.
///
/// Returns the first encoding or structural limit error; encoders of
/// suppressed emissions are never invoked.
#[doc(hidden)]
pub fn apply_head_program(
    builder: &mut HeadBuilder,
    segments: &[ProgramSegment<'_>],
) -> Result<(), ApiError> {
    // One inline survivor buffer serves both families, and a survivor's key
    // is normalized (allocated) for the sticky-sensitivity lookup only when
    // the family has a sensitive contribution at all, so the common head
    // evaluates its program without heap allocation.
    let mut survivors = Survivors::new();
    for family in [PatchFamily::Query, PatchFamily::Header] {
        select_into(segments, family, &mut survivors);
        let sensitive = sticky_sensitive(segments, family);
        for &(segment_index, op_index) in survivors.as_slice() {
            let segment = &segments[segment_index];
            let op = &segment.ops[op_index];
            let redacted =
                !sensitive.is_empty() && sensitive.contains(&normalized_key(family, op.key));
            let mut sink = OccurrenceSink {
                builder: &mut *builder,
                family,
                key: op.key,
                redacted,
                error: None,
            };
            let result = segment.emissions.emit(op.emission, &mut sink);
            if let Some(error) = sink.error.take() {
                return Err(error);
            }
            if result.is_err() {
                return Err(builder.invalid_public_value(family));
            }
        }
    }
    Ok(())
}

/// Program segments kept on the stack when one owner or endpoint appends its
/// own segment to an inherited chain.
const INLINE_SEGMENTS: usize = 16;

/// Calls `visit` once with `inherited` followed by `local`.
///
/// Generated owners and endpoints (and runtime endpoints) use this to extend
/// the program chain they receive from their owner. Chains shorter than
/// [`INLINE_SEGMENTS`] are composed in a stack array, so a nested scope no
/// longer allocates a `Vec` per level on every prepared head (V3-O07
/// attribution); longer chains fall back to a `Vec`. The composed order is
/// identical in both cases.
#[doc(hidden)]
pub fn visit_extended_program<'a, R>(
    inherited: &[ProgramSegment<'a>],
    local: ProgramSegment<'a>,
    visit: impl FnOnce(&[ProgramSegment<'a>]) -> R,
) -> R {
    let length = inherited.len();
    if length < INLINE_SEGMENTS {
        let mut segments = [local; INLINE_SEGMENTS];
        segments[..length].copy_from_slice(inherited);
        visit(&segments[..=length])
    } else {
        let mut segments = Vec::with_capacity(length + 1);
        segments.extend_from_slice(inherited);
        segments.push(local);
        visit(&segments)
    }
}

/// Returns the keys that would survive for inspection/testing, without
/// encoding values. Each entry is `(family, key)` in final order.
#[doc(hidden)]
#[must_use]
pub fn surviving_keys<'a>(segments: &[ProgramSegment<'a>]) -> Vec<(PatchFamily, &'a str)> {
    let mut keys = Vec::new();
    for family in [PatchFamily::Query, PatchFamily::Header] {
        for (segment_index, op_index) in select(segments, family) {
            keys.push((family, segments[segment_index].ops[op_index].key));
        }
    }
    keys
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use super::*;

    struct Values {
        present: Vec<bool>,
        calls: Vec<Cell<u32>>,
        values: Vec<Vec<&'static str>>,
        failing: Option<u16>,
    }

    impl Values {
        fn new(values: Vec<Option<Vec<&'static str>>>) -> Self {
            Self {
                present: values.iter().map(Option::is_some).collect(),
                calls: values.iter().map(|_| Cell::new(0)).collect(),
                values: values.into_iter().map(Option::unwrap_or_default).collect(),
                failing: None,
            }
        }
    }

    impl HeadEmissions for Values {
        fn emission_present(&self, emission: u16) -> bool {
            self.present[usize::from(emission)]
        }

        fn emit(&self, emission: u16, sink: &mut OccurrenceSink<'_>) -> Result<(), AtomError> {
            let index = usize::from(emission);
            self.calls[index].set(self.calls[index].get() + 1);
            if self.failing == Some(emission) {
                return Err(AtomError::new(crate::atom::AtomErrorKind::Limit));
            }
            for value in &self.values[index] {
                sink.value(*value)?;
            }
            Ok(())
        }
    }

    fn keyed(kind: PatchKind, key: &'static str, emission: u16) -> PatchOp<'static> {
        PatchOp::keyed(PatchFamily::Query, kind, key, emission, false)
    }

    fn keys(values: &Values, ops: &[PatchOp<'_>]) -> Vec<String> {
        let segments = [ProgramSegment {
            ops,
            emissions: values,
        }];
        surviving_keys(&segments)
            .into_iter()
            .map(|(_, key)| key.to_owned())
            .collect()
    }

    // Parent entries a=1, locale=fr, z=9 followed by one local operation
    // (spec §6.3 worked example).
    fn parent() -> Vec<PatchOp<'static>> {
        vec![
            keyed(PatchKind::Introduce, "a", 0),
            keyed(PatchKind::Introduce, "locale", 1),
            keyed(PatchKind::Introduce, "z", 2),
        ]
    }

    #[test]
    fn when_present_none_preserves_order_some_replaces_at_new_position() {
        let absent = Values::new(vec![
            Some(vec!["1"]),
            Some(vec!["fr"]),
            Some(vec!["9"]),
            None,
        ]);
        let mut ops = parent();
        ops.push(keyed(PatchKind::WhenPresent, "locale", 3));
        assert_eq!(keys(&absent, &ops), ["a", "locale", "z"]);

        let present = Values::new(vec![
            Some(vec!["1"]),
            Some(vec!["fr"]),
            Some(vec!["9"]),
            Some(vec!["de"]),
        ]);
        assert_eq!(keys(&present, &ops), ["a", "z", "locale"]);
    }

    #[test]
    fn exact_replace_none_clears_and_append_keeps_existing() {
        let values = Values::new(vec![
            Some(vec!["1"]),
            Some(vec!["fr"]),
            Some(vec!["9"]),
            None,
            Some(vec!["de"]),
        ]);
        let mut replace = parent();
        replace.push(keyed(PatchKind::Replace, "locale", 3));
        assert_eq!(keys(&values, &replace), ["a", "z"]);

        let mut append = parent();
        append.push(keyed(PatchKind::Append, "locale", 4));
        assert_eq!(keys(&values, &append), ["a", "locale", "z", "locale"]);
    }

    #[test]
    fn present_empty_each_when_present_clears_parent() {
        let values = Values::new(vec![
            Some(vec!["1"]),
            Some(vec!["fr"]),
            Some(vec!["9"]),
            Some(vec![]),
        ]);
        let mut ops = parent();
        ops.push(keyed(PatchKind::WhenPresent, "locale", 3));
        // The emission survives structurally (present) but produces zero
        // occurrences; the parent value is gone.
        assert_eq!(keys(&values, &ops), ["a", "z", "locale"]);
    }

    #[test]
    fn remove_and_family_clear_are_structural() {
        let values = Values::new(vec![Some(vec!["1"]), Some(vec!["fr"]), Some(vec!["9"])]);
        let mut remove = parent();
        remove.push(PatchOp::remove(PatchFamily::Query, "a"));
        assert_eq!(keys(&values, &remove), ["locale", "z"]);
        let mut clear = parent();
        clear.push(PatchOp::clear(PatchFamily::Query));
        assert!(keys(&values, &clear).is_empty());
    }

    #[test]
    fn header_identity_is_case_insensitive_and_query_is_not() {
        let values = Values::new(vec![Some(vec!["x"]), None]);
        let header_ops = [
            PatchOp::keyed(PatchFamily::Header, PatchKind::Introduce, "X-Tag", 0, false),
            PatchOp::keyed(PatchFamily::Header, PatchKind::Replace, "x-tag", 1, false),
        ];
        assert!(keys(&values, &header_ops).is_empty());
        let query_ops = [
            PatchOp::keyed(PatchFamily::Query, PatchKind::Introduce, "Tag", 0, false),
            PatchOp::keyed(PatchFamily::Query, PatchKind::Replace, "tag", 1, false),
        ];
        assert_eq!(keys(&values, &query_ops), ["Tag"]);
    }

    #[test]
    fn sensitivity_is_sticky_across_replacement_and_fences() {
        let ops = [
            PatchOp::keyed(PatchFamily::Query, PatchKind::Introduce, "tenant", 0, true),
            PatchOp::clear(PatchFamily::Query),
            PatchOp::keyed(PatchFamily::Query, PatchKind::Introduce, "tenant", 1, false),
        ];
        let values = Values::new(vec![Some(vec!["a"]), Some(vec!["b"])]);
        let segments = [ProgramSegment {
            ops: &ops,
            emissions: &values,
        }];
        let sensitive = sticky_sensitive(&segments, PatchFamily::Query);
        assert!(sensitive.contains("tenant"));
    }

    #[test]
    fn survivor_selection_past_the_inline_capacity_keeps_vec_semantics() {
        // More survivors than the inline buffer holds, with removals before
        // and after the spill: the order and retain behavior must match the
        // plain `Vec` algorithm exactly.
        const KEYS: [&str; 24] = [
            "k00", "k01", "k02", "k03", "k04", "k05", "k06", "k07", "k08", "k09", "k10", "k11",
            "k12", "k13", "k14", "k15", "k16", "k17", "k18", "k19", "k20", "k21", "k22", "k23",
        ];
        let values = Values::new(vec![Some(vec!["v"]); KEYS.len() + 2]);
        let mut ops: Vec<PatchOp<'static>> = KEYS
            .iter()
            .enumerate()
            .map(|(index, key)| {
                keyed(
                    PatchKind::Introduce,
                    key,
                    u16::try_from(index).expect("small"),
                )
            })
            .collect();
        ops.insert(3, PatchOp::remove(PatchFamily::Query, "k01"));
        ops.push(keyed(PatchKind::Replace, "k05", 24));
        ops.push(PatchOp::remove(PatchFamily::Query, "k20"));
        ops.push(keyed(PatchKind::Append, "k00", 25));
        let mut expected: Vec<&str> = KEYS
            .iter()
            .copied()
            .filter(|key| !matches!(*key, "k01" | "k05" | "k20"))
            .collect();
        expected.extend(["k05", "k00"]);
        assert_eq!(keys(&values, &ops), expected);

        let mut clear = ops.clone();
        clear.push(PatchOp::clear(PatchFamily::Query));
        clear.push(keyed(PatchKind::Introduce, "k07", 7));
        assert_eq!(keys(&values, &clear), ["k07"]);
    }

    #[test]
    fn extended_program_composes_the_same_chain_inline_and_on_the_heap() {
        let values = Values::new(vec![Some(vec!["v"])]);
        let ops = [keyed(PatchKind::Append, "k", 0)];
        let segment = ProgramSegment {
            ops: &ops,
            emissions: &values,
        };
        let local_ops = [keyed(PatchKind::Append, "local", 0)];
        let local = ProgramSegment {
            ops: &local_ops,
            emissions: &values,
        };
        for inherited_length in [0, 1, 3, INLINE_SEGMENTS - 1, INLINE_SEGMENTS, 40] {
            let inherited = vec![segment; inherited_length];
            let composed = visit_extended_program(&inherited, local, |segments| {
                segments
                    .iter()
                    .map(|segment| segment.ops[0].key)
                    .collect::<Vec<_>>()
            });
            let mut expected = vec!["k"; inherited_length];
            expected.push("local");
            assert_eq!(composed, expected, "{inherited_length}");
        }
    }

    #[test]
    fn suppressed_emissions_are_never_encoded() {
        let mut values = Values::new(vec![Some(vec!["1"]), Some(vec!["2"])]);
        values.failing = Some(0);
        let ops = [
            keyed(PatchKind::Introduce, "k", 0),
            keyed(PatchKind::Replace, "k", 1),
        ];
        let segments = [ProgramSegment {
            ops: &ops,
            emissions: &values,
        }];
        let survivors = select(&segments, PatchFamily::Query);
        assert_eq!(survivors, [(0, 1)]);
        assert_eq!(values.calls[0].get(), 0);
    }
}
