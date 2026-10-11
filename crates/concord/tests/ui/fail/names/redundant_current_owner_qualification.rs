// LANG-03 / §25.2: redundant qualification of the current owner, including
// v2 `endpoint.x`, is not canonical v3 and is rejected with a migration
// diagnostic naming the bare spelling.
use concord::{Binary, Bytes, api};

api! {
    client EndpointQualifiedApi {
        base "https://a.example.test/";

        GET item(id: String) -> Bytes via Binary {
            path "items/{endpoint.id}";
        }
    }
}

api! {
    client ScopeQualifiedApi {
        base "https://a.example.test/";

        scope regional(region: String) {
            path "regions/{regional.region}";
            GET item() -> Bytes via Binary;
        }
    }
}

api! {
    client ClientQualifiedApi {
        base "https://a.example.test/";
        values { tenant: String; }
        headers { "X-Tenant" = client.tenant; }

        GET item() -> Bytes via Binary;
    }
}

api! {
    client EndpointQualifiedHeaderApi {
        base "https://a.example.test/";

        GET item(trace: String) -> Bytes via Binary {
            headers { "X-Trace" = endpoint.trace; }
        }
    }
}

fn main() {}
