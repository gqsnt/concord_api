// Normalized spelling wins before edit distance. Distance is measured on
// characters, counts transpositions, and admits more edits in longer names.
#![allow(unused_imports)]

use concord::{Binary, Bytes, api};

api! {
    client SuggestCaseApi {
        base "https://values.example.test/";
        GET read(query tenant_id: String, query tenant_ids: String) -> Bytes via Binary {
            headers { "X-Tenant" = TENANT_ID; }
        }
    }
}

api! {
    client SuggestUnderscoreApi {
        base "https://values.example.test/";
        GET read(query tenant_id: String, query tenant_ids: String) -> Bytes via Binary {
            headers { "X-Tenant" = tenantid; }
        }
    }
}

// The normalized match is further away lexically and belongs to an ancestor.
api! {
    client SuggestNormalizedPrecedenceApi {
        base "https://values.example.test/";
        values { query tenant_id: String; }
        GET read(query tenantIds: String) -> Bytes via Binary {
            headers { "X-Tenant" = tenantId; }
        }
    }
}

api! {
    client SuggestTranspositionApi {
        base "https://values.example.test/";
        GET read(query tenant: String) -> Bytes via Binary {
            headers { "X-Tenant" = tennat; }
        }
    }
}

// Three substitutions are allowed at nine characters (length / 3).
api! {
    client SuggestLongThresholdApi {
        base "https://values.example.test/";
        GET read(query abcyyyxxx: String) -> Bytes via Binary {
            headers { "X-Token" = abcxxxxxx; }
        }
    }
}

// The same three substitutions exceed the threshold at eight characters.
api! {
    client SuggestShortThresholdApi {
        base "https://values.example.test/";
        GET read(query abcyyyxx: String) -> Bytes via Binary {
            headers { "X-Token" = abcxxxxx; }
        }
    }
}

// Only the nearest edit-distance group is included, with stable tie order.
api! {
    client SuggestNearestTiesApi {
        base "https://values.example.test/";
        GET read(query cut: String, query cat: String, query cart: String) -> Bytes via Binary {
            headers { "X-Token" = cot; }
        }
    }
}

// Equal-distance candidates can be local or qualified ancestor values.
api! {
    client SuggestAncestorTiesApi {
        base "https://values.example.test/";
        values { query("root_tenant") tenant: String; }
        scope regional(query("scope_tenant") tenant: String) {
            GET read(query tenet: String) -> Bytes via Binary {
                headers { "X-Tenant" = tenent; }
            }
        }
    }
}

api! {
    client SuggestUnrelatedApi {
        base "https://values.example.test/";
        GET read(query tenant: String) -> Bytes via Binary {
            headers { "X-Token" = astronaut; }
        }
    }
}

// Existing exact-ancestor guidance takes precedence over a fuzzy local match.
api! {
    client SuggestExactAncestorApi {
        base "https://values.example.test/";
        values { query("root_tenant") tenant: String; }
        scope regional(query("scope_tenant") tenant: String) {
            GET read(query tenants: String) -> Bytes via Binary {
                headers { "X-Tenant" = tenant; }
            }
        }
    }
}

// Edit distance uses characters rather than counting UTF-8 bytes.
api! {
    client SuggestUnicodeApi {
        base "https://values.example.test/";
        GET read(query café: String) -> Bytes via Binary {
            headers { "X-Token" = cafè; }
        }
    }
}

fn main() {}
