#![forbid(unsafe_code)]

use std::sync::Arc;

use concord_core::Request;
use concord_core::advanced::NoopRateLimiter;
use concord_macros::api;

api! {
    client RateLimitApi {
        base "https://rate-limit.example.test/";
        var tenant: String;
        rate_limits {
            application {
                by [host];
                500 / 10s;
            }
            tenant {
                by [host, endpoint, method, vars.tenant, ep.item];
                cost 2;
                100 / 1m;
            }
            observe concord_examples::RetryAfterObserver;
        }
        rate_limit application;
        GET Item(item: String) -> Bytes { rate_limit tenant; }
        GET Public() -> Bytes { rate_limit off; }
    }
}

fn main() -> Result<(), concord_core::prelude::ClientBuildError> {
    let client = RateLimitApi::builder()
        .tenant("example-tenant".to_owned())
        .runtime(|runtime| runtime.rate_limiter(Arc::new(NoopRateLimiter::new())))
        .build()?;
    let _: Request<_> = client.item("record-42".to_owned());
    let _: Request<_> = client.public();
    Ok(())
}
