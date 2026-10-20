// CALL-01: generated navigation and builder names cannot collide with the
// generated core methods; collisions are rejected, never suffixed.
use concord::{Binary, Bytes, api};

api! {
    client PlanApi {
        base "https://example.test";
        GET plan() -> Bytes via Binary;
    }
}

api! {
    client ContractApi {
        base "https://example.test";
        GET contract() -> Bytes via Binary;
    }
}

api! {
    client UnboundApi {
        base "https://example.test";

        scope unbound() {
            GET read() -> Bytes via Binary;
        }
    }
}

api! {
    client CredentialsApi {
        base "https://example.test";
        GET credentials() -> Bytes via Binary;
    }
}

// A builder secret setter named like the builder's own `build`.
api! {
    client BuilderSecretApi {
        base "https://example.test";
        credentials {
            secret build;
            credential key = api_key(secret.build);
        }

        GET read() -> Bytes via Binary {
            auth { header "X-Key" = key; }
        }
    }
}

// Scope handles keep `bind` for runtime endpoints.
api! {
    client ScopeBindApi {
        base "https://example.test";

        scope regional() {
            GET bind() -> Bytes via Binary;
        }
    }
}

fn main() {}
