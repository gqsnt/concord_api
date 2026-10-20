// PAGE-01 / V3-P02: a cursor destination cannot be a direct rate dimension.
// (An optional cursor slot, e.g. for CursorPagination, is already rejected as
// an outer-optional dimension; this covers a total cursor slot.)
#![allow(unused_imports)]

use concord::{Binary, Bytes, api};

use std::num::NonZeroU32;

use concord::pagination::{OffsetInit, OffsetPagination};

pub const PAGE_SIZE: NonZeroU32 = NonZeroU32::MIN;

api! {
    client TotalCursorApi {
        base "https://example.test";

        GET list(query offset: u64 = 0) -> Bytes via Binary {
            rate_limit {
                id "concord.rate_limit_pagination_dimension.2";
                key ["offset" = offset];
                allow 1 per 1s;
            }
            paginate via OffsetPagination {
                init OffsetInit { offset: offset, page_size: crate::PAGE_SIZE };
                cursor -> offset;
            }
        }
    }
}

fn main() {}
