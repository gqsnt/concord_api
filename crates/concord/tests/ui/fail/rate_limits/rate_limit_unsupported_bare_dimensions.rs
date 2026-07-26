use concord::api;

api! {
    client MethodApi {
        base "https://example.test";
        rate_limit { by [method]; 1 / 1s; }
    }
}

api! {
    client PathApi {
        base "https://example.test";
        rate_limit { by [path]; 1 / 1s; }
    }
}

api! {
    client QueryApi {
        base "https://example.test";
        rate_limit { by [query]; 1 / 1s; }
    }
}

api! {
    client SafeUrlApi {
        base "https://example.test";
        rate_limit { by [safe_url]; 1 / 1s; }
    }
}

fn main() {}
