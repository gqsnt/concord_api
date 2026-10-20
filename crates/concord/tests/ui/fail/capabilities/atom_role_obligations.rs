// DIAG-01 (§19.1) / ATOM-01 (§5.1): generated atom-role obligations are
// role-specific spanned obligations. Each failure reports the stable
// `[C3-ATOM]` message and a correction at the owning placement (the authored
// reference, the direct placement's value, or the path template), never
// across the whole `api!` invocation. A `Vec` scalar does not silently mean
// repeated occurrences: the correction names both `each` and a scalar
// collection encoder.
use std::collections::HashSet;

use concord::{Binary, Bytes, api};

pub struct NotAnEncoder;

api! {
    client AtomRoleApi {
        base "https://atoms.example.test";

        GET header_block(values: Vec<String>) -> Bytes via Binary {
            headers { "X-Values" = values; }
        }

        GET direct(query tags: Vec<String>) -> Bytes via Binary;

        GET hole(ids: Vec<u32>) -> Bytes via Binary {
            path "items/{ids}";
        }

        GET adapter(id: u32) -> Bytes via Binary {
            query { "id" = id via crate::NotAnEncoder; }
        }

        GET unordered(tags: HashSet<String>) -> Bytes via Binary {
            query { "tag" = each tags; }
        }
    }
}

api! {
    client LabelRoleApi {
        base "https://labels.example.test";

        scope regional(labels: Vec<String>) {
            subdomain [labels];

            GET read() -> Bytes via Binary;
        }
    }
}

fn main() {}
