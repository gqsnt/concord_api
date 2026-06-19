use concord_core::prelude::*;
use concord_macros::api;

#[cfg(feature = "cache-moka")]
api! {
    client LargeCacheTtlApi {
        base "https://example.com"

        cache long_lived {
            http
            ttl 18446744073709551615s
        }
    }

    GET Ping
    path ["ping"]
    cache long_lived
    -> Text<String>
}

#[cfg(feature = "cache-moka")]
fn main() {}

#[cfg(not(feature = "cache-moka"))]
fn main() {}
