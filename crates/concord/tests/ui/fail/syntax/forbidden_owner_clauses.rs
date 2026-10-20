// LANG-02 / §3.3 ownership table: each family belongs to specific owners.
// Client-only registries, endpoint-only selections, scope-only fences and
// client/scope-only subdomains are rejected elsewhere.
#![allow(unused_imports)]

use concord::{Binary, Bytes, Text, api};

// Client-only registries inside a scope.
api! {
    client ScopeBaseApi {
        base "https://owner.example.test";

        scope s() {
            base "https://other.example.test";
            GET read() -> Bytes via Binary;
        }
    }
}

api! {
    client ScopeValuesApi {
        base "https://owner.example.test";

        scope s() {
            values { query tenant: String; }
            GET read() -> Bytes via Binary;
        }
    }
}

api! {
    client ScopeCredentialsApi {
        base "https://owner.example.test";

        scope s() {
            credentials { secret key; }
            GET read() -> Bytes via Binary;
        }
    }
}

api! {
    client ScopeRateRegistryApi {
        base "https://owner.example.test";

        scope accounts() {
            rate_limits {
                policy account_reads() { id "owner.accounts"; key [origin]; allow 1 per 1s; }
            }
            GET read() -> Bytes via Binary;
        }
    }
}

// Endpoint-only selections on a scope.
api! {
    client ScopeStatusApi {
        base "https://owner.example.test";

        scope s() {
            status 204;
            GET read() -> Bytes via Binary;
        }
    }
}

api! {
    client ScopeIdApi {
        base "https://owner.example.test";

        scope s() {
            id "owner.scope";
            GET read() -> Bytes via Binary;
        }
    }
}

api! {
    client ClientEffectApi {
        base "https://owner.example.test";
        effect safe;
    }
}

// Client-only registries on an endpoint.
api! {
    client EndpointValuesApi {
        base "https://owner.example.test";

        GET read() -> Bytes via Binary {
            values { query tenant: String; }
        }
    }
}

// Subdomains belong to clients and scopes, never endpoints.
api! {
    client EndpointSubdomainApi {
        base "https://owner.example.test";

        GET read() -> Bytes via Binary {
            subdomain ["api"];
        }
    }
}

// Representation defaults are owner clauses; endpoints select inline.
api! {
    client EndpointResponseDefaultApi {
        base "https://owner.example.test";

        GET read() -> String {
            response Text;
        }
    }
}

// Representation fences are scope-only.
api! {
    client ClientFenceApi {
        base "https://owner.example.test";
        response explicit;
    }
}

// Preamble clauses precede children.
api! {
    client LatePreambleApi {
        base "https://owner.example.test";

        GET read() -> Bytes via Binary;

        headers { "X-Late" = "1"; }
    }
}

fn main() {}
