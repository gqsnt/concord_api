// Owner qualifiers and qualified value names use their own visible namespaces.
#![allow(unused_imports)]

use concord::{Binary, Bytes, api};

api! {
    client SuggestClientQualifierApi {
        base "https://values.example.test/";
        values { query tenant: String; }
        GET read() -> Bytes via Binary {
            headers { "X-Tenant" = clinet.tenant; }
        }
    }
}

api! {
    client SuggestScopeQualifierApi {
        base "https://values.example.test/";
        scope regions(query region: String) {
            GET read() -> Bytes via Binary {
                headers { "X-Region" = regons.region; }
            }
        }
    }
}

api! {
    client SuggestNormalizedQualifierApi {
        base "https://values.example.test/";
        scope regional_zone(query region: String) {
            GET read() -> Bytes via Binary {
                headers { "X-Region" = RegionalZone.region; }
            }
        }
    }
}

api! {
    client SuggestBothPartsApi {
        base "https://values.example.test/";
        scope regions(query region: String) {
            GET read() -> Bytes via Binary {
                headers { "X-Region" = regons.regoin; }
            }
        }
    }
}

// A correct qualifier limits value candidates to that owner's declarations.
api! {
    client SuggestQualifiedNamespaceApi {
        base "https://values.example.test/";
        values { query account_id: String; }
        GET read(query account_key: String) -> Bytes via Binary {
            headers { "X-Account" = client.account_ke; }
        }
    }
}

// A far-away owner spelling cannot be repaired by matching its value alone.
api! {
    client SuggestUnrelatedQualifierApi {
        base "https://values.example.test/";
        values { query tenant: String; }
        GET read() -> Bytes via Binary {
            headers { "X-Tenant" = moon.tenant; }
        }
    }
}

// Redundant current-owner qualification retains its migration diagnostic,
// including when the qualified value also has a spelling error.
api! {
    client SuggestCurrentOwnerApi {
        base "https://values.example.test/";
        scope regional(query region: String) {
            headers { "X-Region" = regional.regoin; }
            GET read() -> Bytes via Binary;
        }
    }
}

api! {
    client SuggestEndpointQualifierApi {
        base "https://values.example.test/";
        GET read(query tenant: String) -> Bytes via Binary {
            headers { "X-Tenant" = endpoint.tennat; }
        }
    }
}

// Corrections preserve raw value names so the proposed reference is valid DSL.
api! {
    client SuggestRawValueApi {
        base "https://values.example.test/";
        values { query r#type: String; }
        GET read() -> Bytes via Binary {
            headers { "X-Type" = client.typ; }
        }
    }
}

// A corrected scope qualifier also needs its raw marker when it is a keyword.
api! {
    client SuggestRawScopeApi {
        base "https://values.example.test/";
        scope r#type(query region: String) {
            GET read() -> Bytes via Binary {
                headers { "X-Region" = tyep.region; }
            }
        }
    }
}

// These names select legacy/private-expression branches unless escaped.
api! {
    client SuggestDslQualifierApi {
        base "https://values.example.test/";
        scope secret(query tenant: String) {
            GET read() -> Bytes via Binary {
                headers { "X-Tenant" = secrte.tennat; }
            }
        }
    }
}

api! {
    client SuggestDslValueApi {
        base "https://values.example.test/";
        GET read(query concat: String) -> Bytes via Binary {
            headers { "X-Tenant" = conat; }
        }
    }
}

fn main() {}
