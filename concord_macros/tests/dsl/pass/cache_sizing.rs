use concord_core::prelude::*;
use concord_macros::api;

#[cfg(feature = "cache-moka")]
api! {
    client CacheSizingApi {
        base "https://example.com"

        policies {
            cache base {
                http
                ttl 60s
                capacity 10_000 entries
                max_body 2 mib
                shared
            }

            cache child extends base {
                capacity 1_000 entries
                max_body 512 kib
            }
        }

        defaults {
            cache child
        }
    }

    GET Ping
    path ["ping"]
    cache {
        max_body 128 kb
    }
    -> Json<()>
}

#[cfg(feature = "cache-moka")]
fn main() {}

#[cfg(not(feature = "cache-moka"))]
fn main() {}
