use concord::api;

pub struct NoDisplay;

api! {
    client Api {
        base "https://example.test";
    }

    GET read(resource: crate::NoDisplay) -> Bytes {
        rate_limit { by [ep.resource]; 1 / 1s; }
    }
}

fn main() {}
