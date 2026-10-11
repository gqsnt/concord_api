// DIAG-01 (§19.1): every coded diagnostic names its owning client, scope or
// endpoint (by its full owner path) and carries a correction valid in the v3
// grammar. One invocation per family, because each closed declaration
// reports its first violation: PLACE, ATOM, AUTH, RATE, PAGE, NAME and REP.
#![allow(unused_imports)]

use concord::{Binary, Bytes, api};

// C3-PLACE: a reserved public header in a nested scope's block.
api! {
    client PlaceOwnerApi {
        base "https://owner.example.test/";

        scope tenants() {
            scope users() {
                headers { "Content-Type" = "text/plain"; }

                GET list() -> Bytes via Binary;
            }
        }
    }
}

// C3-ATOM: a suffixed integer atom in an endpoint query block.
api! {
    client AtomOwnerApi {
        base "https://owner.example.test/";

        scope catalog() {
            GET items() -> Bytes via Binary {
                query { "limit" = 10u32; }
            }
        }
    }
}

// C3-AUTH: two challenge-attributing placements on one endpoint.
api! {
    client AuthOwnerApi {
        base "https://owner.example.test/";
        credentials {
            secret first_key;
            secret second_key;
            credential first = api_key(secret.first_key);
            credential second = api_key(secret.second_key);
        }

        scope admin() {
            GET audit() -> Bytes via Binary {
                auth {
                    header "X-First" = first challenge unauthorized;
                    header "X-Second" = second challenge unauthorized;
                }
            }
        }
    }
}

// C3-RATE: a zero cost in a scope's inline rate policy.
api! {
    client RateOwnerApi {
        base "https://owner.example.test/";

        scope reports() {
            rate_limit { id "owner.reports"; key [origin]; cost 0; allow 10 per 1s; }

            GET daily() -> Bytes via Binary;
        }
    }
}

// C3-PAGE: a duplicate Init field in an endpoint's pagination.
api! {
    client PageOwnerApi {
        base "https://owner.example.test/";

        scope feeds() {
            GET list(query offset: u64 = 0) -> Vec<String> via Binary {
                paginate via OffsetPagination {
                    init OffsetInit { offset: offset, offset: offset };
                    cursor -> offset;
                }
            }
        }
    }
}

// C3-NAME: an unknown bare name with no ancestor candidate.
api! {
    client NameOwnerApi {
        base "https://owner.example.test/";

        scope regional(region: String) {
            path "{region}";

            GET item() -> Bytes via Binary {
                path "items/{item_id}";
            }
        }
    }
}

// C3-REP: a body with no request representation in a fenced scope.
api! {
    client RepOwnerApi {
        base "https://owner.example.test/";
        request Binary;

        scope uploads() {
            request explicit;

            POST create(body input: Bytes) -> Bytes via Binary;
        }
    }
}

fn main() {}
