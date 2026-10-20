// LANG-03 / V3-L02: a bare name binds only the current owner. A name that
// exists only on an ancestor is an error; the diagnostic suggests the exact
// qualification but never chooses it automatically.
use concord::{Binary, Bytes, api};

// Endpoint path hole naming a scope value.
api! {
    client EndpointHoleApi {
        base "https://a.example.test/";

        scope regional(id: String) {
            path "regions/{id}";

            GET item() -> Bytes via Binary {
                path "items/{id}";
            }
        }
    }
}

// Scope preamble naming a root value.
api! {
    client ScopePreambleApi {
        base "https://a.example.test/";
        values { query tenant: String; }

        scope regional() {
            headers { "X-Tenant" = tenant; }
            GET item() -> Bytes via Binary;
        }
    }
}

// Endpoint query statement naming a value both the root and a scope declare:
// every visible candidate is suggested.
api! {
    client AmbiguousSuggestionApi {
        base "https://a.example.test/";
        values { query("root_locale") locale: String; }

        scope regional(query("scope_locale") locale: String) {
            GET item() -> Bytes via Binary {
                query { "locale" = locale; }
            }
        }
    }
}

// Rate-limit bindings follow the same rule.
api! {
    client RateBindingApi {
        base "https://a.example.test/";
        values { query tenant: String; }
        rate_limits {
            policy per_tenant(tenant) {
                id "a.tenant";
                key [origin, "tenant" = tenant];
                allow 10 per 1s;
            }
        }

        GET item() -> Bytes via Binary {
            rate_limit per_tenant(tenant = tenant);
        }
    }
}

fn main() {}
