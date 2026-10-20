// Value spelling hints stay within the visible owner chain and preserve the
// qualification required by LANG-03. Each macro isolates one failing reference.
#![allow(unused_imports)]

use concord::pagination::{OffsetInit, OffsetPagination};
use concord::{Binary, Bytes, api};

// A whole path segment and a mixed literal/hole segment both resolve values.
api! {
    client PathHoleSuggestion {
        base "https://example.test";
        GET read(tenant: String) -> Bytes via Binary {
            path "{tennat}";
        }
    }
}

api! {
    client PathComponentSuggestion {
        base "https://example.test";
        GET read(tenant: String) -> Bytes via Binary {
            path "tenant-{tennat}";
        }
    }
}

// DNS labels use list atoms, including values qualified by an ancestor owner.
api! {
    client SubdomainSuggestion {
        base "https://example.test";
        scope regional(tenant: String) {
            subdomain replace [tennat, "api"];
            GET read() -> Bytes via Binary;
        }
    }
}

api! {
    client SubdomainAncestorSuggestion {
        base "https://example.test";
        values { query tenant: String; }
        scope regional() {
            subdomain replace [client.tennat, "api"];
            GET read() -> Bytes via Binary;
        }
    }
}

api! {
    client QuerySuggestion {
        base "https://example.test";
        GET read(tenant: String) -> Bytes via Binary {
            query { "tenant" = tennat; }
        }
    }
}

api! {
    client HeaderSuggestion {
        base "https://example.test";
        GET read(tenant: String) -> Bytes via Binary {
            headers { "X-Tenant" = tennat; }
        }
    }
}

api! {
    client QueryConcatSuggestion {
        base "https://example.test";
        GET read(tenant: String) -> Bytes via Binary {
            query { "tenant" = concat ["prefix-", tennat]; }
        }
    }
}

api! {
    client HeaderConcatSuggestion {
        base "https://example.test";
        GET read(tenant: String) -> Bytes via Binary {
            headers { "X-Tenant" = concat ["prefix-", tennat]; }
        }
    }
}

// Correct the qualifier or leaf while retaining the complete legal spelling.
api! {
    client QualifiedOwnerSuggestion {
        base "https://example.test";
        scope regional(query tenant: String) {
            GET read() -> Bytes via Binary {
                query { "owner" = regonal.tenant; }
            }
        }
    }
}

api! {
    client QualifiedLeafSuggestion {
        base "https://example.test";
        scope regional(query tenant: String) {
            GET read() -> Bytes via Binary {
                headers { "X-Tenant" = regional.tennat; }
            }
        }
    }
}

// A bare typo for an ancestor must suggest `client.tenant`, not bare `tenant`.
api! {
    client BareAncestorSuggestion {
        base "https://example.test";
        values { query tenant: String; }
        GET read() -> Bytes via Binary {
            path "{tennat}";
        }
    }
}

// The closer spelling wins even when another visible ancestor is nearer.
api! {
    client ClosestAncestorSuggestion {
        base "https://example.test";
        values { query tenant: String; }
        scope regional(query tenant_name: String) {
            GET read() -> Bytes via Binary {
                headers { "X-Tenant" = tennat; }
            }
        }
    }
}

// Exact bare ancestor matches keep the existing explicit-qualification error.
api! {
    client ExactAncestorSuggestion {
        base "https://example.test";
        values { query tenant: String; }
        GET read(tenants: String) -> Bytes via Binary {
            path "{tenant}";
        }
    }
}

// Siblings and descendants cannot contribute correction candidates.
api! {
    client SiblingExcludedSuggestion {
        base "https://example.test";
        scope sibling(query tenant: String) {
            GET read() -> Bytes via Binary;
        }
        GET read() -> Bytes via Binary {
            headers { "X-Tenant" = tennat; }
        }
    }
}

api! {
    client DescendantExcludedSuggestion {
        base "https://example.test";
        headers { "X-Tenant" = tennat; }
        scope regional(query tenant: String) {
            GET read() -> Bytes via Binary;
        }
    }
}

// An unrelated visible spelling is not a useful correction.
api! {
    client UnrelatedValueSuggestion {
        base "https://example.test";
        GET read(tenant: String) -> Bytes via Binary {
            headers { "X-Trace" = ultraviolet; }
        }
    }
}

// Policy bindings use the same value resolver as request-head atoms.
api! {
    client RateValueSuggestion {
        base "https://example.test";
        rate_limits {
            policy per_tenant(tenant) {
                id "suggestion.tenant";
                key [origin, "tenant" = tenant];
                allow 10 per 1s;
            }
        }
        GET read(tenant: String) -> Bytes via Binary {
            rate_limit per_tenant(tenant = tennat);
        }
    }
}

// Pagination init references and cursor destinations both diagnose misspellings.
api! {
    client PaginationInitSuggestion {
        base "https://example.test";
        GET read(query offset: u64 = 0) -> Bytes via Binary {
            paginate via OffsetPagination {
                init OffsetInit { offset: offest, page_size: 1 };
                cursor -> offset;
            }
        }
    }
}

api! {
    client PaginationAncestorInitSuggestion {
        base "https://example.test";
        values { query initial_offset: u64 = 0; }
        GET read(query offset: u64 = 0) -> Bytes via Binary {
            paginate via OffsetPagination {
                init OffsetInit { offset: client.intial_offset, page_size: 1 };
                cursor -> offset;
            }
        }
    }
}

api! {
    client PaginationCursorSuggestion {
        base "https://example.test";
        GET read(query offset: u64 = 0) -> Bytes via Binary {
            paginate via OffsetPagination {
                init OffsetInit { offset: offset, page_size: 1 };
                cursor -> offest;
            }
        }
    }
}

// Equal-distance candidates produce a stable, sorted list of alternatives.
api! {
    client TiedValueSuggestion {
        base "https://example.test";
        GET read(query cot: String, query cat: String) -> Bytes via Binary {
            headers { "X-Pet" = cut; }
        }
    }
}

// Candidate spellings preserve the raw identifier required by Rust syntax.
api! {
    client RawValueSuggestion {
        base "https://example.test";
        GET read(query r#type: String) -> Bytes via Binary {
            headers { "X-Type" = tyep; }
        }
    }
}

api! {
    client ClientQualifierSuggestion {
        base "https://example.test";
        values { query tenant: String; }
        GET read() -> Bytes via Binary {
            headers { "X-Tenant" = clinet.tenant; }
        }
    }
}

// Cursor suggestions must exclude otherwise-near values that cannot receive
// a cursor: each sources, quota dimensions, previous destinations, ancestors.
api! {
    client PaginationEachExcluded {
        base "https://example.test";
        GET read(query each offset: Vec<u64>) -> Bytes via Binary {
            paginate via OffsetPagination {
                init OffsetInit { offset: 0, page_size: 1 };
                cursor -> offest;
            }
        }
    }
}

api! {
    client PaginationRateExcluded {
        base "https://example.test";
        rate_limits {
            policy per_offset(offset) {
                id "suggestion.offset";
                key [origin, "offset" = offset];
                allow 10 per 1s;
            }
        }
        GET read(query offset: u64 = 0) -> Bytes via Binary {
            rate_limit per_offset(offset = offset);
            paginate via OffsetPagination {
                init OffsetInit { offset: 0, page_size: 1 };
                cursor -> offest;
            }
        }
    }
}

api! {
    client PaginationPreviousDestinationExcluded {
        base "https://example.test";
        GET read(query offset: u64 = 0) -> Bytes via Binary {
            paginate via OffsetPagination {
                init OffsetInit { offset: 0, page_size: 1 };
                cursor -> (offset, offest);
            }
        }
    }
}

api! {
    client PaginationAncestorExcluded {
        base "https://example.test";
        values { query offset: u64 = 0; }
        GET read() -> Bytes via Binary {
            paginate via OffsetPagination {
                init OffsetInit { offset: 0, page_size: 1 };
                cursor -> offest;
            }
        }
    }
}

// Later destinations are also reserved: suggesting `offset` would duplicate
// the second tuple component after the proposed correction is applied.
api! {
    client PaginationLaterDestinationExcluded {
        base "https://example.test";
        GET read(query offset: u64 = 0) -> Bytes via Binary {
            paginate via OffsetPagination {
                init OffsetInit { offset: 0, page_size: 1 };
                cursor -> (offest, offset);
            }
        }
    }
}

// Visible optional values cannot repair references that require total atoms.
// Each typo has only one nearby spelling, which must yield no suggestion.
api! {
    client OptionalPathCandidateExcluded {
        base "https://example.test";
        GET read(query tenant?: String) -> Bytes via Binary {
            path "{tennat}";
        }
    }
}

api! {
    client OptionalSubdomainCandidateExcluded {
        base "https://example.test";
        scope regional(query tenant?: String) {
            subdomain replace [tennat, "api"];
            GET read() -> Bytes via Binary;
        }
    }
}

api! {
    client OptionalConcatCandidateExcluded {
        base "https://example.test";
        GET read(query tenant?: String) -> Bytes via Binary {
            headers { "X-Tenant" = concat ["prefix-", tennat]; }
        }
    }
}

api! {
    client QualifiedRawValueSuggestion {
        base "https://example.test";
        values { query r#type: String; }
        GET read() -> Bytes via Binary {
            headers { "X-Type" = client.tyep; }
        }
    }
}

fn main() {}
