// Structural rejections of the selected grammar: ORIGIN-01 base URL shape,
// PATH-01 relative template segments, ORIGIN-02 DNS labels, private sources
// in public expressions, arbitrary expressions, AUTH-01 exclusive targets,
// HEAD-01 reserved public headers, rate policies without windows and
// LANG-02 empty concatenation.
#![allow(unused_imports)]

use concord::{Empty, api};

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
    client BaseSchemeApi {
        base "ftp://example.test/";
    }
}

api! {
    client EmptyPathApi {
        base "https://example.test/";
        path "";
    }
}

api! {
    client DotPathApi {
        base "https://example.test/";
        path ".";
    }
}

api! {
    client DotDotPathApi {
        base "https://example.test/";
        path "a/..";
    }
}

api! {
    client SlashPathApi {
        base "https://example.test/";
        path "/";
    }
}

api! {
    client BackslashPathApi {
        base "https://example.test/";
        path "\\";
    }
}

api! {
    client InvalidHostApi {
        base "https://example.test/";
        subdomain replace ["bad_label"];
    }
}

api! {
    client DottedHostApi {
        base "https://example.test/";
        subdomain ["eu.api"];
    }
}

api! {
    client SecretExpressionApi {
        base "https://example.test/";
        credentials {
            secret token;
            credential token = api_key(secret.token);
        }

        GET read() -> () via Empty {
            headers { "X-Public" = secret.token; }
            auth { header "X-Token" = token; }
        }
    }
}

// LANG-02 / ATOM-01: `concat [...]` builds one scalar atom from at least one
// part; an empty concatenation is invalid rather than an empty string.
api! {
    client EmptyConcatApi {
        base "https://example.test/";

        GET read() -> () via Empty {
            headers { "X-Trace" = concat []; }
        }
    }
}

api! {
    client ArbitraryExpressionApi {
        base "https://example.test/";

        GET read() -> () via Empty {
            headers { "X-Value" = make_value(); }
        }
    }
}

api! {
    client DuplicateAuthHeaderApi {
        base "https://example.test/";
        credentials {
            secret first;
            secret second;
            credential first = api_key(secret.first);
            credential second = api_key(secret.second);
        }

        GET read() -> () via Empty {
            auth {
                header "X-Key" = first;
                header "x-key" = second;
            }
        }
    }
}

api! {
    client DuplicateAuthorizationApi {
        base "https://example.test/";
        credentials {
            secret first;
            secret second;
            credential first = bearer(secret.first);
            credential second = bearer(secret.second);
        }

        GET read() -> () via Empty {
            auth {
                bearer first;
                bearer second;
            }
        }
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
        rate_limit { id "concord.spec_rejections.1"; key [origin]; cost 1; }
    }
}

api! {
    client EmptyRateLimitProfileApi {
        base "https://example.test/";
        rate_limits {
            policy empty() { id "concord.spec_rejections.2"; key [origin]; cost 1; }
        }
        rate_limit empty();
    }
}

fn main() {}
