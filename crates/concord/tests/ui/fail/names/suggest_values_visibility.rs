// Sibling scopes, descendant scopes, and other endpoints are not visible
// reference owners. Exact and nearby declarations provide location notes,
// never a suggested reference that would still be invalid at the use site.
#![allow(unused_imports)]

use concord::{Binary, Bytes, api};

api! {
    client SuggestSiblingExactApi {
        base "https://values.example.test/";
        scope hidden(query tenant: String) {
            GET first() -> Bytes via Binary;
        }
        scope public() {
            GET read() -> Bytes via Binary {
                headers { "X-Tenant" = tenant; }
            }
        }
    }
}

api! {
    client SuggestSiblingTypoApi {
        base "https://values.example.test/";
        scope hidden(query tenant: String) {
            GET first() -> Bytes via Binary;
        }
        scope public() {
            GET read() -> Bytes via Binary {
                headers { "X-Tenant" = tennat; }
            }
        }
    }
}

api! {
    client SuggestDescendantExactApi {
        base "https://values.example.test/";
        scope outer() {
            headers { "X-Tenant" = tenant; }
            scope inner(query tenant: String) {
                GET read() -> Bytes via Binary;
            }
        }
    }
}

api! {
    client SuggestDescendantTypoApi {
        base "https://values.example.test/";
        scope outer() {
            headers { "X-Tenant" = tennat; }
            scope inner(query tenant: String) {
                GET read() -> Bytes via Binary;
            }
        }
    }
}

api! {
    client SuggestOtherEndpointExactApi {
        base "https://values.example.test/";
        GET first(query tenant: String) -> Bytes via Binary;
        GET read() -> Bytes via Binary {
            headers { "X-Tenant" = tenant; }
        }
    }
}

api! {
    client SuggestOtherEndpointTypoApi {
        base "https://values.example.test/";
        GET first(query tenant: String) -> Bytes via Binary;
        GET read() -> Bytes via Binary {
            headers { "X-Tenant" = tennat; }
        }
    }
}

// A named sibling stays invisible even if both parts are spelled exactly.
api! {
    client SuggestSiblingQualifiedExactApi {
        base "https://values.example.test/";
        scope hidden(query tenant: String) {
            GET first() -> Bytes via Binary;
        }
        GET read() -> Bytes via Binary {
            headers { "X-Tenant" = hidden.tenant; }
        }
    }
}

api! {
    client SuggestSiblingQualifiedTypoApi {
        base "https://values.example.test/";
        scope hidden(query tenant: String) {
            GET first() -> Bytes via Binary;
        }
        GET read() -> Bytes via Binary {
            headers { "X-Tenant" = hiden.tennat; }
        }
    }
}

// A known owner with a missing value can still explain an invisible match.
api! {
    client SuggestQualifiedInvisibleValueApi {
        base "https://values.example.test/";
        scope hidden(query tenant: String) {
            GET first() -> Bytes via Binary;
        }
        GET read() -> Bytes via Binary {
            headers { "X-Tenant" = client.tennat; }
        }
    }
}

// A visible correction wins even over an exact invisible declaration.
api! {
    client SuggestVisibleBeforeInvisibleExactApi {
        base "https://values.example.test/";
        scope hidden(query tennat: String) {
            GET first() -> Bytes via Binary;
        }
        GET read(query tenant: String) -> Bytes via Binary {
            headers { "X-Tenant" = tennat; }
        }
    }
}

// Normalized visible candidates also win over exact invisible declarations.
api! {
    client SuggestVisibleNormalizedApi {
        base "https://values.example.test/";
        scope hidden(query tenantid: String) {
            GET first() -> Bytes via Binary;
        }
        GET read(query tenant_id: String) -> Bytes via Binary {
            headers { "X-Tenant" = tenantid; }
        }
    }
}

// An ancestor correction remains valid while the sibling's exact name does not.
api! {
    client SuggestVisibleAncestorApi {
        base "https://values.example.test/";
        values { query tenant: String; }
        scope hidden(query tennat: String) {
            GET first() -> Bytes via Binary;
        }
        GET read() -> Bytes via Binary {
            headers { "X-Tenant" = tennat; }
        }
    }
}

api! {
    client SuggestUnrelatedInvisibleApi {
        base "https://values.example.test/";
        scope hidden(query tenant: String) {
            GET first() -> Bytes via Binary;
        }
        GET read() -> Bytes via Binary {
            headers { "X-Token" = astronaut; }
        }
    }
}

// Even when the written qualifier prevents a visible correction, nearby
// visible values suppress notes about an exact invisible declaration.
api! {
    client SuggestQualifiedVisibleSuppressesInvisibleApi {
        base "https://values.example.test/";
        scope hidden(query tennat: String) {
            GET first() -> Bytes via Binary;
        }
        GET read(query tenant: String) -> Bytes via Binary {
            headers { "X-Tenant" = client.tennat; }
        }
    }
}

api! {
    client SuggestUnknownQualifierVisibleSuppressesInvisibleApi {
        base "https://values.example.test/";
        scope hidden(query tennat: String) {
            GET first() -> Bytes via Binary;
        }
        GET read(query tenant: String) -> Bytes via Binary {
            headers { "X-Tenant" = moon.tennat; }
        }
    }
}

fn main() {}
