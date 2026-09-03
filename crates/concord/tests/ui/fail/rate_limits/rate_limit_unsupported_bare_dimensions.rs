use concord::api;

api! {
    client MethodApi {
        timeout off;
        base "https://example.test";
        rate_limit { key [method]; allow 1 per 1s; }
    }
}

api! {
    client PathApi {
        timeout off;
        base "https://example.test";
        rate_limit { key [path]; allow 1 per 1s; }
    }
}

api! {
    client QueryApi {
        timeout off;
        base "https://example.test";
        rate_limit { key [query]; allow 1 per 1s; }
    }
}

api! {
    client SafeUrlApi {
        timeout off;
        base "https://example.test";
        rate_limit { key [observed_url]; allow 1 per 1s; }
    }
}

fn main() {}
