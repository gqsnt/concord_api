// LANG-02: at most one clause per singleton family and owner. Duplicated
// singleton clauses are errors, never last-writer-wins. One invocation per
// family because each closed declaration reports its first violation.
#![allow(unused_imports)]

use concord::{Binary, Bytes, Text, api};

api! {
    client DuplicateEndpointTimeout {
        base "http://duplicate.example.test/";

        GET read() -> Bytes via Binary {
            timeout 1s;
            path "read";
            timeout 2s;
        }
    }
}

api! {
    client DuplicateBase {
        base "http://one.example.test/";
        base "http://two.example.test/";
    }
}

api! {
    client DuplicateValues {
        base "http://duplicate.example.test/";
        values { query a: String; }
        values { query b: String; }
    }
}

api! {
    client DuplicateCredentials {
        base "https://duplicate.example.test/";
        credentials { secret a; credential x = api_key(secret.a); }
        credentials { secret b; credential y = api_key(secret.b); }
    }
}

api! {
    client DuplicateRegistry {
        base "https://duplicate.example.test/";
        rate_limits { windows first { allow 1 per 1s; } }
        rate_limits { windows second { allow 2 per 1s; } }
    }
}

api! {
    client DuplicateScopePath {
        base "http://duplicate.example.test/";

        scope s() {
            path "a";
            path "b";
            GET read() -> Bytes via Binary;
        }
    }
}

api! {
    client DuplicateScopeQuery {
        base "http://duplicate.example.test/";

        scope s() {
            query { "a" = "1"; }
            query { "b" = "2"; }
            GET read() -> Bytes via Binary;
        }
    }
}

api! {
    client DuplicateClientHeaders {
        base "http://duplicate.example.test/";
        headers { "X-A" = "1"; }
        headers off;
    }
}

api! {
    client DuplicateEndpointAuth {
        base "https://duplicate.example.test/";
        credentials { secret a; credential key = api_key(secret.a); }

        GET read() -> Bytes via Binary {
            auth { header "X-Key" = key; }
            auth off;
        }
    }
}

api! {
    client DuplicateScopeSubdomain {
        base "https://duplicate.example.test/";

        scope s() {
            subdomain ["a"];
            subdomain replace ["b"];
            GET read() -> Bytes via Binary;
        }
    }
}

api! {
    client DuplicateResponseDefault {
        base "https://duplicate.example.test/";
        response Text;
        response Binary;
    }
}

api! {
    client DuplicateRequestDefault {
        base "https://duplicate.example.test/";

        scope s() {
            request Text;
            request explicit;
            GET read() -> Bytes via Binary;
        }
    }
}

api! {
    client DuplicateEndpointId {
        base "https://duplicate.example.test/";

        GET read() -> Bytes via Binary {
            id "duplicate.one";
            id "duplicate.two";
        }
    }
}

api! {
    client DuplicateEndpointStatus {
        base "https://duplicate.example.test/";

        GET read() -> Bytes via Binary {
            status 200;
            status success;
        }
    }
}

api! {
    client DuplicateEndpointEffect {
        base "https://duplicate.example.test/";

        POST write() -> Bytes via Binary {
            effect idempotent;
            effect safe;
        }
    }
}

api! {
    client DuplicateEndpointAbsence {
        base "https://duplicate.example.test/";

        GET find() -> Option<String> via Text {
            absent on 404;
            absent on 410;
        }
    }
}

// LANG-02: one pagination selection per endpoint; a second `paginate` clause
// is rejected rather than replacing the first.
api! {
    client DuplicateEndpointPagination {
        base "https://duplicate.example.test/";

        GET list(query offset: u64 = 0) -> Vec<String> via Binary {
            paginate via OffsetPagination {
                init OffsetInit { offset: offset, page_size: PAGE_SIZE };
                cursor -> offset;
            }
            paginate via PagePagination {
                init PageInit { page: PAGE, page_size: PAGE_SIZE };
                cursor -> offset;
            }
        }
    }
}

api! {
    client DuplicateEndpointExposure {
        base "https://duplicate.example.test/";

        GET read() -> Bytes via Binary {
            expose accepted_bytes;
            expose accepted_bytes;
        }
    }
}

fn main() {}
