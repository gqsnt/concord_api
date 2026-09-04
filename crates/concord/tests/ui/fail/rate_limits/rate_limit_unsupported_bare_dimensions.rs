use concord::api;

api! {
    client MethodApi {
        timeout 30s;
        base "https://example.test";
        rate_limit { id "concord.rate_limit_unsupported_bare_dimensions.1"; cost 1;  key [method]; allow 1 per 1s; }
    }
}

api! {
    client PathApi {
        timeout 30s;
        base "https://example.test";
        rate_limit { id "concord.rate_limit_unsupported_bare_dimensions.2"; cost 1;  key [path]; allow 1 per 1s; }
    }
}

api! {
    client QueryApi {
        timeout 30s;
        base "https://example.test";
        rate_limit { id "concord.rate_limit_unsupported_bare_dimensions.3"; cost 1;  key [query]; allow 1 per 1s; }
    }
}

api! {
    client SafeUrlApi {
        timeout 30s;
        base "https://example.test";
        rate_limit { id "concord.rate_limit_unsupported_bare_dimensions.4"; cost 1;  key [observed_url]; allow 1 per 1s; }
    }
}

fn main() {}
