//! Explicit public atom adapters.

use std::fmt;

use concord_core::contracts::{AtomEncoder, AtomError, BoundedAtomWriter};

/// Explicit compatibility adapter from [`fmt::Display`] to one request atom.
///
/// Select this adapter with authored `via DisplayAtom` syntax. Concord never
/// treats `Display` as an implicit wire contract.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DisplayAtom;

impl<T> AtomEncoder<T> for DisplayAtom
where
    T: fmt::Display + ?Sized,
{
    fn write(value: &T, output: &mut BoundedAtomWriter<'_>) -> Result<(), AtomError> {
        output.write_display(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct DomainId(u32);

    impl fmt::Display for DomainId {
        fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(formatter, "id-{}", self.0)
        }
    }

    #[test]
    fn display_is_available_only_through_the_explicit_adapter() {
        fn assert_adapter<A: AtomEncoder<DomainId>>() {}
        assert_adapter::<DisplayAtom>();

        fn request_atom_is_not_required<T: ?Sized>() {
            let _ = std::any::type_name::<T>();
        }
        request_atom_is_not_required::<DomainId>();
    }
}
