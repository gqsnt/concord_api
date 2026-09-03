use concord::api;

api! {
    client BaseUserInfoApi {
        timeout off;
        base "https://user@example.test/";
    }
}

api! {
    client BaseQueryApi {
        timeout off;
        base "https://example.test/?page=1";
    }
}

api! {
    client BaseFragmentApi {
        timeout off;
        base "https://example.test/#fragment";
    }
}

api! {
    client EmptyPathApi {
        timeout off;
        base "https://example.test/";
        path [""];
    }
}

api! {
    client DotPathApi {
        timeout off;
        base "https://example.test/";
        path ["."];
    }
}

api! {
    client DotDotPathApi {
        timeout off;
        base "https://example.test/";
        path [".."];
    }
}

api! {
    client SlashPathApi {
        timeout off;
        base "https://example.test/";
        path ["/"];
    }
}

api! {
    client BackslashPathApi {
        timeout off;
        base "https://example.test/";
        path ["\\"];
    }
}

api! {
    client InvalidHostApi {
        timeout off;
        base "https://example.test/";
        subdomain replace ["bad_label"];
    }
}

api! {
    client SecretExpressionApi {
        timeout off;
        base "https://example.test/";
        credentials {
            secret token;
            credential token = api_key(secret.token);
        }


    GET read() -> () using NoContent {
        path [secret.token];
        auth { header "X-Token" = token; }
    }
}}

api! {
    client ArbitraryExpressionApi {
        timeout off;
        base "https://example.test/";


    GET read() -> () using NoContent {
        path [make_path()];
    }
}}

api! {
    client DuplicateAuthHeaderApi {
        timeout off;
        base "https://example.test/";
        credentials {
            secret first;
            secret second;
            credential first = api_key(secret.first);
            credential second = api_key(secret.second);
        }


    GET read() -> () using NoContent {
        auth { header "X-Key" = first; }
        auth { header "x-key" = second; }
    }
}}

api! {
    client DuplicateAuthorizationApi {
        timeout off;
        base "https://example.test/";
        credentials {
            secret first;
            secret second;
            credential first = bearer(secret.first);
            credential second = bearer(secret.second);
        }


    GET read() -> () using NoContent {
        auth { bearer first; }
        auth { bearer second; }
    }
}}

api! {
    client ReservedPublicHeaderApi {
        timeout off;
        base "https://example.test/";
        headers {
            "Authorization" = "public";
        }
    }
}

api! {
    client EmptyInlineRateLimitApi {
        timeout off;
        base "https://example.test/";
        rate_limit {}
    }
}

api! {
    client EmptyRateLimitProfileApi {
        timeout off;
        base "https://example.test/";
        rate_limits {
            empty() {}
        }
        rate_limit empty();
    }
}

fn main() {}
