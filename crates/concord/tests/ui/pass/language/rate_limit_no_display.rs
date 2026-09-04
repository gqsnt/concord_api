use concord::prelude::Bytes;
use concord::api;

pub struct Displayless;

api! {
    client Api {
        timeout 30s;
        base "https://example.test";
        values {
            client_value: crate::Displayless;
        }
        rate_limit { id "concord.rate_limit_no_display.1"; key [origin]; cost 1;  allow 10 per 1s; }


    scope regions(region: crate::Displayless) {
        rate_limit { id "concord.rate_limit_no_display.2"; key [origin]; cost 1;  allow 5 per 1s; }
        GET read(item: crate::Displayless) -> Bytes via Binary {
            rate_limit { id "concord.rate_limit_no_display.3"; key [origin]; cost 1;  allow 1 per 1s; }
        }
    }
}}

fn main() {
    let _ = core::mem::size_of::<Displayless>();
}
