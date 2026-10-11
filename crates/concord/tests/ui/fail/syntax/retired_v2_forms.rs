// §25.2 / MIGRATE-03: recognized retired v2 forms produce a precise
// `[C3-MIGRATE]` diagnostic naming the canonical v3 spelling; the clean v3
// parser never silently retains the old meaning.
#![allow(unused_imports)]

use concord::{Binary, Bytes, Text, api};

api! {
    client VarsBlockApi {
        base "https://example.test/";
        vars { tenant: String; }
    }
}

api! {
    client HostListApi {
        base "https://example.test/";
        host ["api"];
    }
}

api! {
    client PathListApi {
        base "https://example.test/";

        GET read(id: String) -> Bytes via Binary {
            path ["items", id];
        }
    }
}

api! {
    client UsingRepresentationApi {
        base "https://example.test/";

        GET read() -> String using Text;
    }
}

api! {
    client UsingBodyRepresentationApi {
        base "https://example.test/";

        POST write(body text: String using Text) -> Bytes via Binary;
    }
}

api! {
    client FmtAtomApi {
        base "https://example.test/";

        GET read(id: String) -> Bytes via Binary {
            headers { "X-Item" = fmt["items-", id]; }
        }
    }
}

api! {
    client AppendOperatorApi {
        base "https://example.test/";

        GET read(id: String) -> Bytes via Binary {
            query { "id" += id; }
        }
    }
}

api! {
    client RedactAttributeApi {
        base "https://example.test/";
        values { #[redact] query tenant: String; }
    }
}

api! {
    client VarsReferenceApi {
        base "https://example.test/";
        values { tenant: String; }
        headers { "X-Tenant" = vars.tenant; }
    }
}

api! {
    client EpReferenceApi {
        base "https://example.test/";

        GET read(id: String) -> Bytes via Binary {
            headers { "X-Id" = ep.id; }
        }
    }
}

api! {
    client TypedSecretApi {
        base "https://example.test/";
        credentials {
            secret token: String;
        }
    }
}

api! {
    client LegacyRateLabelApi {
        base "https://example.test/";
        values { tenant: String; }
        rate_limit {
            id "legacy.tenant";
            key [origin, tenant];
            allow 1 per 1s;
        }
    }
}

api! {
    client LegacyPaginationApi {
        base "https://example.test/";

        GET read(query offset: u64) -> Bytes via Binary {
            paginate via crate::Controller {
                offset -> offset;
            }
        }
    }
}

fn main() {}
