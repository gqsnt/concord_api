use concord::api;

api! {
    client BaseUserInfoApi {
        base "https://user@example.test/";
    }
}

api! {
    client BaseQueryApi {
        base "https://example.test/?page=1";
    }
}

api! {
    client BaseFragmentApi {
        base "https://example.test/#fragment";
    }
}

api! {
    client EmptyPathApi {
        base "https://example.test/";
        path [""];
    }
}

api! {
    client DotPathApi {
        base "https://example.test/";
        path ["."];
    }
}

api! {
    client DotDotPathApi {
        base "https://example.test/";
        path [".."];
    }
}

api! {
    client SlashPathApi {
        base "https://example.test/";
        path ["/"];
    }
}

api! {
    client BackslashPathApi {
        base "https://example.test/";
        path ["\\"];
    }
}

api! {
    client InvalidHostApi {
        base "https://example.test/";
        host ["bad_label"];
    }
}

api! {
    client SecretExpressionApi {
        base "https://example.test/";
        credentials {
            secret token: String;
            credential token = api_key(secret.token);
        }
    }

    GET read() -> NoContent {
        path [secret.token];
        auth header "X-Token" = token;
    }
}

api! {
    client ArbitraryExpressionApi {
        base "https://example.test/";
    }

    GET read() -> NoContent {
        path [make_path()];
    }
}

api! {
    client DuplicateAuthHeaderApi {
        base "https://example.test/";
        credentials {
            secret first: String;
            secret second: String;
            credential first = api_key(secret.first);
            credential second = api_key(secret.second);
        }
    }

    GET read() -> NoContent {
        auth header "X-Key" = first;
        auth header "x-key" = second;
    }
}

api! {
    client DuplicateAuthorizationApi {
        base "https://example.test/";
        credentials {
            secret first: String;
            secret second: String;
            credential first = bearer(secret.first);
            credential second = bearer(secret.second);
        }
    }

    GET read() -> NoContent {
        auth bearer first;
        auth bearer second;
    }
}

api! {
    client ReservedPublicHeaderApi {
        base "https://example.test/";
        headers {
            "Authorization" = "public";
        }
    }
}

api! {
    client EmptyInlineRateLimitApi {
        base "https://example.test/";
        rate_limit {}
    }
}

api! {
    client EmptyRateLimitProfileApi {
        base "https://example.test/";
        rate_limits {
            rule empty {}
        }
        rate_limit use empty;
    }
}

fn main() {}
