// §12.3: `origin` and `endpoint` are the only built-in dimensions; every other
// dimension is an explicit `"label" = value`. Bare names get a migration
// diagnostic, and a label must bind a declared value.
#![allow(unused_imports)]

use concord::{Binary, Bytes, api};

api! {
    client MethodApi {
        base "https://example.test";
        rate_limit { id "concord.rate_limit_unsupported_bare_dimensions.1"; key [method]; allow 1 per 1s; }
    }
}

api! {
    client PathApi {
        base "https://example.test";
        rate_limit { id "concord.rate_limit_unsupported_bare_dimensions.2"; key [path]; allow 1 per 1s; }
    }
}

api! {
    client QueryApi {
        base "https://example.test";
        rate_limit { id "concord.rate_limit_unsupported_bare_dimensions.3"; key [query]; allow 1 per 1s; }
    }
}

api! {
    client SafeUrlApi {
        base "https://example.test";
        rate_limit { id "concord.rate_limit_unsupported_bare_dimensions.4"; key [observed_url]; allow 1 per 1s; }
    }
}

api! {
    client LabelledMethodApi {
        base "https://example.test";
        rate_limit { id "concord.rate_limit_unsupported_bare_dimensions.5"; key ["method" = method]; allow 1 per 1s; }
    }
}

fn main() {}
