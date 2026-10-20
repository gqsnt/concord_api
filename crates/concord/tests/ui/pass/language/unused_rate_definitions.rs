// §12.2 (replaces the v2 "rate definition never attached" rejection): unused
// window sets and named policies are allowed and shown as unused by contract
// inspection. They create no artificial required input and no limiter
// requirement, so the zero-input `new()` constructor remains available
// (§7.3). An unused *formal* is still an error
// (fail/rate_limits/rate_limit_unused_formal.rs).
use concord::{Binary, Bytes, api};

api! {
    client UnusedRateApi {
        base "http://example.test";
        rate_limits {
            windows standard { allow 10 per 1s; allow 100 per 1m; }
            policy future_reads(account) {
                id "unused.future.reads";
                key [origin, "account" = account];
                using standard;
            }
        }

        GET read() -> Bytes via Binary;
    }
}

fn main() {
    let client = UnusedRateApi::new().expect("no effective rate policy, no limiter required");
    let _: unused_rate_api::ReadRequest = client.read();
    let _ = UnusedRateApi::contract();
}
