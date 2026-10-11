// RATE-01 / VALUE-04: rate attachments at every owner level never require
// `Display` (or any rate-key role) from values that are not key dimensions.
// The values here implement only the explicit RequestAtom role (no Display
// fallback exists) and are live through direct query placements (VALUE-06).
use concord::api;
use concord::prelude::Bytes;
use concord::{AtomError, Binary, BoundedAtomWriter, RequestAtom};

pub struct Displayless;

impl RequestAtom for Displayless {
    fn write_atom(&self, output: &mut BoundedAtomWriter<'_>) -> Result<(), AtomError> {
        output.push_str("opaque")
    }
}

api! {
    client Api {
        timeout 30s;
        base "https://example.test";
        values {
            query client_value: crate::Displayless;
        }
        rate_limit { id "concord.rate_limit_no_display.1"; key [origin]; cost 1; allow 10 per 1s; }

        scope regions(query region: crate::Displayless) {
            rate_limit { id "concord.rate_limit_no_display.2"; key [origin]; cost 1; allow 5 per 1s; }

            GET read(query item: crate::Displayless) -> Bytes via Binary {
                rate_limit { id "concord.rate_limit_no_display.3"; key [origin]; cost 1; allow 1 per 1s; }
            }
        }
    }
}

fn main() {
    let _ = Api::builder().set_client_value(Displayless);
}
