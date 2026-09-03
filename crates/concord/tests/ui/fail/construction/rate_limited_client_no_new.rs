use concord::{NoContent, api};

api! {
    client RateLimitedApi {
        base "https://rate.example.test";
        timeout off;

        rate_limit {
            id "rate.client.read";
            key [host, endpoint];
            allow 10 per 1m;
        }

        GET read() -> () using NoContent;
    }
}

fn main() {
    let _ = RateLimitedApi::new();
}
