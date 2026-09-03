use concord::api;

pub struct NoDisplay;

api! {
    client Api {
        timeout off;
        base "https://example.test";


    GET read(resource: crate::NoDisplay) -> Bytes using Bytes {
        rate_limit { key [endpoint.resource]; allow 1 per 1s; }
    }
}}

fn main() {}
