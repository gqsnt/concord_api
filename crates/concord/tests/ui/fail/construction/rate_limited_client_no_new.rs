use concord::{Empty, api};

api! {
    client RateLimitedApi {
        base "https://rate.example.test";
        timeout 30s;

        rate_limit { cost 1;
            id "rate.client.read";
            key [origin, endpoint];
            allow 10 per 1m;
        }

        GET read() -> () via Empty;
    }
}

fn main() {
    let _ = RateLimitedApi::new();
}
