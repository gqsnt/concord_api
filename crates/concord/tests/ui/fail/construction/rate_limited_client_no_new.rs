// §7.3 / §12.5: a client with any reachable effective rate policy requires an
// explicit limiter choice, so no zero-argument `new()` is generated. The
// `endpoint` key dimension requires the covered endpoint's explicit ID
// (IDENTITY-01).
use concord::{Empty, api};

api! {
    client RateLimitedApi {
        base "https://rate.example.test";
        timeout 30s;

        rate_limit {
            id "rate.client.read";
            key [origin, endpoint];
            cost 1;
            allow 10 per 1m;
        }

        GET read() -> () via Empty {
            id "rate.read";
        }
    }
}

fn main() {
    let _ = RateLimitedApi::new();
}
