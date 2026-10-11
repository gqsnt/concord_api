// CALL-03 / §7.4 / §10.4: `accepted_bytes()` exists only on a bound request
// of an endpoint that declares `expose accepted_bytes;` (an inert unbound
// intent has no terminals). Unsupported capabilities are absent methods, not
// runtime "not supported" errors. (Source-only `store_as_<credential>()` is
// storage_terminal_source_only.rs; page terminals are
// fail/pagination/non_paginated_no_paginate.rs.)
use concord::{Binary, Bytes, api};

api! {
    client ExposureApi {
        base "https://exposure.example.test";

        GET exposed() -> Bytes via Binary {
            expose accepted_bytes;
        }
        GET ordinary() -> Bytes via Binary;
    }
}

fn main() {
    let client = ExposureApi::new().unwrap();
    let _ = client.exposed().accepted_bytes();
    let _ = client.ordinary().accepted_bytes();
    let _ = ExposureApi::unbound().exposed().accepted_bytes();
}
