// PAGE-01 / V3-P02: cursor destinations are unique ordinary endpoint-owned
// scalar slots — never the body, a client/scope value, an `each` source, or
// a value in an effective rate key. Page eligibility also requires `effect
// safe`, and `absent on` cannot combine with pagination.
#![allow(unused_imports)]

use std::num::NonZeroU32;

use concord::pagination::{OffsetInit, OffsetPagination};
use concord::{Binary, Bytes, api};

pub const PAGE_SIZE: NonZeroU32 = NonZeroU32::MIN;

api! {
    client BodyDestination {
        base "https://example.test";

        POST read(body offset: Bytes via Binary) -> Bytes via Binary {
            effect safe;
            paginate via OffsetPagination {
                init OffsetInit { offset: 0, page_size: crate::PAGE_SIZE };
                cursor -> offset;
            }
        }
    }
}

api! {
    client ScopeDestination {
        base "https://example.test";

        scope pages(query offset: u64) {
            GET read() -> Bytes via Binary {
                paginate via OffsetPagination {
                    init OffsetInit { offset: pages.offset, page_size: crate::PAGE_SIZE };
                    cursor -> offset;
                }
            }
        }
    }
}

api! {
    client ClientDestination {
        base "https://example.test";
        values { query offset: u64 = 0; }

        GET read() -> Bytes via Binary {
            paginate via OffsetPagination {
                init OffsetInit { offset: client.offset, page_size: crate::PAGE_SIZE };
                cursor -> client.offset;
            }
        }
    }
}

api! {
    client EachDestination {
        base "https://example.test";

        GET read(query each offset: Vec<u64>) -> Bytes via Binary {
            paginate via OffsetPagination {
                init OffsetInit { offset: 0, page_size: crate::PAGE_SIZE };
                cursor -> offset;
            }
        }
    }
}

api! {
    client RepeatedDestination {
        base "https://example.test";

        GET read(query offset: u64 = 0) -> Bytes via Binary {
            paginate via OffsetPagination {
                init OffsetInit { offset: offset, page_size: crate::PAGE_SIZE };
                cursor -> (offset, offset);
            }
        }
    }
}

api! {
    client RateDestination {
        base "https://example.test";
        rate_limits {
            policy per_offset(offset) {
                id "pages.offset";
                key ["offset" = offset];
                allow 1 per 1s;
            }
        }

        GET read(query offset: u64 = 0) -> Bytes via Binary {
            rate_limit per_offset(offset = offset);
            paginate via OffsetPagination {
                init OffsetInit { offset: offset, page_size: crate::PAGE_SIZE };
                cursor -> offset;
            }
        }
    }
}

api! {
    client UnsafeEffect {
        base "https://example.test";

        POST read(query offset: u64 = 0) -> Bytes via Binary {
            paginate via OffsetPagination {
                init OffsetInit { offset: offset, page_size: crate::PAGE_SIZE };
                cursor -> offset;
            }
        }
    }
}

api! {
    client AbsentPagination {
        base "https://example.test";

        GET read(query offset: u64 = 0) -> Option<Bytes> via Binary {
            absent on 404;
            paginate via OffsetPagination {
                init OffsetInit { offset: offset, page_size: crate::PAGE_SIZE };
                cursor -> offset;
            }
        }
    }
}

fn main() {}
