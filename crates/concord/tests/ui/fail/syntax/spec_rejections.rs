use concord::api;

api! {
    client BaseUserInfoApi {
        timeout 30s;
        base "https://user@example.test/";
    }
}

api! {
    client BaseQueryApi {
        timeout 30s;
        base "https://example.test/?page=1";
    }
}

api! {
    client BaseFragmentApi {
        timeout 30s;
        base "https://example.test/#fragment";
    }
}

api! {
    client EmptyPathApi {
        timeout 30s;
        base "https://example.test/";
        path [""];
    }
}

api! {
    client DotPathApi {
        timeout 30s;
        base "https://example.test/";
        path ["."];
    }
}

api! {
    client DotDotPathApi {
        timeout 30s;
        base "https://example.test/";
        path [".."];
    }
}

api! {
    client SlashPathApi {
        timeout 30s;
        base "https://example.test/";
        path ["/"];
    }
}

api! {
    client BackslashPathApi {
        timeout 30s;
        base "https://example.test/";
        path ["\\"];
    }
}

api! {
    client InvalidHostApi {
        timeout 30s;
        base "https://example.test/";
        subdomain replace ["bad_label"];
    }
}

api! {
    client SecretExpressionApi {
        timeout 30s;
        base "https://example.test/";
        credentials {
            secret token;
            credential token = api_key(secret.token);
        }


    GET read() -> () via Empty {
        path [secret.token];
        auth { header "X-Token" = token; }
    }
}}

api! {
    client ArbitraryExpressionApi {
        timeout 30s;
        base "https://example.test/";


    GET read() -> () via Empty {
        path [make_path()];
    }
}}

api! {
    client DuplicateAuthHeaderApi {
        timeout 30s;
        base "https://example.test/";
        credentials {
            secret first;
            secret second;
            credential first = api_key(secret.first);
            credential second = api_key(secret.second);
        }


    GET read() -> () via Empty {
        auth { header "X-Key" = first; }
        auth { header "x-key" = second; }
    }
}}

api! {
    client DuplicateAuthorizationApi {
        timeout 30s;
        base "https://example.test/";
        credentials {
            secret first;
            secret second;
            credential first = bearer(secret.first);
            credential second = bearer(secret.second);
        }


    GET read() -> () via Empty {
        auth { bearer first; }
        auth { bearer second; }
    }
}}

api! {
    client ReservedPublicHeaderApi {
        timeout 30s;
        base "https://example.test/";
        headers {
            "Authorization" = "public";
        }
    }
}

api! {
    client EmptyInlineRateLimitApi {
        timeout 30s;
        base "https://example.test/";
        rate_limit { id "concord.spec_rejections.1"; key [origin]; cost 1; }
    }
}

api! {
    client EmptyRateLimitProfileApi {
        timeout 30s;
        base "https://example.test/";
        rate_limits {
            policy empty() { id "concord.spec_rejections.2"; key [origin]; cost 1; }
        }
        rate_limit empty();
    }
}

fn main() {}
