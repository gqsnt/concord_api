// VALUE-04 (§4.4): there is no user registration of ordinary-value
// conversions. `IntoValue` is sealed by a private library-owned conversion
// set, so a downstream `impl IntoValue<X> for Y` is rejected with a coded
// correction instead of silently widening what generated setters accept
// (which could change inference for unrelated fields).
use concord::{AtomError, BoundedAtomWriter, IntoValue, RequestAtom};

pub struct ItemId(pub String);

impl RequestAtom for ItemId {
    fn write_atom(&self, output: &mut BoundedAtomWriter<'_>) -> Result<(), AtomError> {
        output.push_str(&self.0)
    }
}

/// A downstream type that would like to convert into `ItemId`.
pub struct Tag(pub String);

impl IntoValue<ItemId> for Tag {
    fn into_value(self) -> ItemId {
        ItemId(self.0)
    }
}

fn main() {}
