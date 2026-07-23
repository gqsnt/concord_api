#![forbid(unsafe_code)]

use std::sync::Arc;

use concord_core::Request;
use concord_core::advanced::NoopRateLimiter;
use concord_macros::api;

api! {
    client RateLimitApi {
        base "https://rate-limit.example.test/";
        vars {
            tenant: String;
        }
        rate_limit { 500 / 10s; }
        rate_limit_observer concord_examples::RetryAfterObserver;
    }

    GET Item(item: String) -> Bytes {
        rate_limit {
            cost 2;
            100 / 1m;
        }
    }
    GET Public() -> Bytes;
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
