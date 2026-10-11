// §4.2: `#[default]` calls the exact payload's `Default`; the obligation is
// reported at the value declaration.
use concord::{AtomError, BoundedAtomWriter, RequestAtom, api};

pub struct NoDefault;

impl RequestAtom for NoDefault {
    fn write_atom(&self, out: &mut BoundedAtomWriter<'_>) -> Result<(), AtomError> {
        out.push_str("value")
    }
}

api! {
    client MissingDefaultApi {
        base "https://example.test";
        values {
            #[default]
            query value: NoDefault;
        }
    }
}

fn main() {}
