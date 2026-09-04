use concord::api;

pub struct NoDisplay;

api! {
    client Api {
        timeout 30s;
        base "https://example.test";


    GET read(resource: crate::NoDisplay) -> Bytes via Binary {
        rate_limit { id "concord.rate_limit_dimension_requires_display.1"; cost 1;  key [endpoint.resource]; allow 1 per 1s; }
    }
}}

fn main() {}
