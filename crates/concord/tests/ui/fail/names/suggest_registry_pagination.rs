// Cursor suggestions exclude ancestor values, bodies, each sources, rate
// dimensions and destinations already present in the mapping.
#![allow(unused_imports)]

use concord::pagination::{OffsetInit, OffsetPagination};
use concord::{Binary, Bytes, api};

api! {
    client CursorSpelling {
        base "https://example.test";
        GET read(query page_offset: u64 = 0) -> Bytes via Binary {
            paginate via OffsetPagination {
                init OffsetInit { offset: page_offset, page_size: std::num::NonZeroU32::MIN };
                cursor -> PageOffset;
            }
        }
    }
}

api! {
    client CursorNamespace {
        base "https://example.test";
        values { query offset: u64 = 0; }
        GET read(query distant: u64 = 0) -> Bytes via Binary {
            paginate via OffsetPagination {
                init OffsetInit { offset: distant, page_size: std::num::NonZeroU32::MIN };
                cursor -> offest;
            }
        }
    }
}

api! {
    client CursorScalarCandidates {
        base "https://example.test";
        GET read(query each offset: Vec<u64>, query offsets: u64 = 0) -> Bytes via Binary {
            paginate via OffsetPagination {
                init OffsetInit { offset: offsets, page_size: std::num::NonZeroU32::MIN };
                cursor -> offest;
            }
        }
    }
}

api! {
    client CursorEachEmissionCandidates {
        base "https://example.test";
        GET read(offset: Vec<u64>, query offsets: u64 = 0) -> Bytes via Binary {
            query { "offset" = each offset; }
            paginate via OffsetPagination {
                init OffsetInit { offset: offsets, page_size: std::num::NonZeroU32::MIN };
                cursor -> offest;
            }
        }
    }
}

api! {
    client CursorRateCandidates {
        base "https://example.test";
        GET read(query offset: u64 = 0, query offsets: u64 = 0) -> Bytes via Binary {
            rate_limit {
                id "suggest.cursor.rate";
                key ["offset" = offset];
                allow 1 per 1s;
            }
            paginate via OffsetPagination {
                init OffsetInit { offset: offsets, page_size: std::num::NonZeroU32::MIN };
                cursor -> offest;
            }
        }
    }
}

api! {
    client CursorAlreadyMapped {
        base "https://example.test";
        GET read(query offset: u64 = 0, query offsets: u64 = 0) -> Bytes via Binary {
            paginate via OffsetPagination {
                init OffsetInit { offset: offset, page_size: std::num::NonZeroU32::MIN };
                cursor -> (offest, offset);
            }
        }
    }
}

api! {
    client CursorBodyCandidates {
        base "https://example.test";
        POST read(body offset: Bytes via Binary, query offsets: u64 = 0) -> Bytes via Binary {
            effect safe;
            paginate via OffsetPagination {
                init OffsetInit { offset: offsets, page_size: std::num::NonZeroU32::MIN };
                cursor -> offest;
            }
        }
    }
}

fn main() {}
