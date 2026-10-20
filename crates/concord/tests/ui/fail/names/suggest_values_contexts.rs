// Each declaration fails independently: references in every expression context
// must use the same spelling suggestions and preserve their original spans.
#![allow(unused_imports)]

use concord::pagination::{OffsetInit, OffsetPagination};
use concord::{Binary, Bytes, api};

api! {
    client SuggestClientPathApi {
        base "https://values.example.test/";
        values { tenant: String; }
        path "tenants/{tennat}";
        GET read() -> Bytes via Binary;
    }
}

api! {
    client SuggestScopePathApi {
        base "https://values.example.test/";
        scope regional(region: String) {
            path "regions/{regoin}";
            GET read() -> Bytes via Binary;
        }
    }
}

api! {
    client SuggestEndpointPathApi {
        base "https://values.example.test/";
        GET read(item_id: String) -> Bytes via Binary {
            path "items/prefix-{item_di}-suffix";
        }
    }
}

api! {
    client SuggestDnsAtomApi {
        base "https://values.example.test/";
        values { region: String; }
        subdomain [regoin];
        GET read() -> Bytes via Binary;
    }
}

api! {
    client SuggestDnsConcatApi {
        base "https://values.example.test/";
        values { query tenant: String; }
        scope regional() {
            subdomain [concat ["api-", client.tennat]];
            GET read() -> Bytes via Binary;
        }
    }
}

api! {
    client SuggestQueryConcatApi {
        base "https://values.example.test/";
        GET read(tenant: String) -> Bytes via Binary {
            query { "label" = concat ["tenant-", tennat]; }
        }
    }
}

api! {
    client SuggestHeaderConcatApi {
        base "https://values.example.test/";
        scope regional(query region: String) {
            GET read() -> Bytes via Binary {
                headers { "X-Region" = concat ["region-", regional.regoin]; }
            }
        }
    }
}

api! {
    client SuggestRateBindingApi {
        base "https://values.example.test/";
        rate_limits {
            policy per_tenant(tenant) {
                id "suggest.values.rate.binding";
                key [origin, "tenant" = tenant];
                allow 10 per 1s;
            }
        }
        GET read(query tenant: String) -> Bytes via Binary {
            rate_limit per_tenant(tenant = tennat);
        }
    }
}

api! {
    client SuggestInlineRateApi {
        base "https://values.example.test/";
        values { query tenant: String; }
        GET read() -> Bytes via Binary {
            rate_limit {
                id "suggest.values.rate.inline";
                key [origin, "tenant" = client.tennat];
                allow 10 per 1s;
            }
        }
    }
}

api! {
    client SuggestPaginationLocalApi {
        base "https://values.example.test/";
        GET read(query offset: u64 = 0) -> Bytes via Binary {
            paginate via OffsetPagination {
                init OffsetInit { offset: ofset, page_size: std::num::NonZeroU32::MIN };
                cursor -> offset;
            }
        }
    }
}

api! {
    client SuggestPaginationAncestorApi {
        base "https://values.example.test/";
        values { query initial_offset: u64 = 0; }
        GET read(query offset: u64 = 0) -> Bytes via Binary {
            paginate via OffsetPagination {
                init OffsetInit { offset: client.initial_ofset, page_size: std::num::NonZeroU32::MIN };
                cursor -> offset;
            }
        }
    }
}

fn main() {}
