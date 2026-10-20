// §14.1 pagination grammar: `paginate via Controller { init InitType { field:
// value, ... }; cursor -> slot; }`. Init values are stable values or direct
// references; no calls, closures, blocks, `..Default::default()` or v2 field
// bindings.
#![allow(unused_imports)]

use concord::pagination::{OffsetInit, OffsetPagination};
use concord::{Binary, Bytes, api};

api! {
    client MissingVia {
        base "https://example.test";

        GET read(query offset: u64 = 0) -> Bytes via Binary {
            paginate OffsetPagination { init OffsetInit { offset: offset }; cursor -> offset; }
        }
    }
}

api! {
    client LegacyFieldBinding {
        base "https://example.test";

        GET read(query offset: u64 = 0) -> Bytes via Binary {
            paginate via OffsetPagination { offset -> offset; }
        }
    }
}

api! {
    client LegacyStableAssignment {
        base "https://example.test";

        GET read(query offset: u64 = 0) -> Bytes via Binary {
            paginate via OffsetPagination { page_size = 10; }
        }
    }
}

api! {
    client MissingInit {
        base "https://example.test";

        GET read(query offset: u64 = 0) -> Bytes via Binary {
            paginate via OffsetPagination { cursor -> offset; }
        }
    }
}

api! {
    client RestConstruction {
        base "https://example.test";

        GET read(query offset: u64 = 0) -> Bytes via Binary {
            paginate via OffsetPagination { init OffsetInit { ..Default::default() }; cursor -> offset; }
        }
    }
}

api! {
    client DuplicateInitField {
        base "https://example.test";

        GET read(query offset: u64 = 0) -> Bytes via Binary {
            paginate via OffsetPagination { init OffsetInit { offset: offset, offset: 0 }; cursor -> offset; }
        }
    }
}

api! {
    client MissingCursor {
        base "https://example.test";

        GET read(query offset: u64 = 0) -> Bytes via Binary {
            paginate via OffsetPagination { init OffsetInit { offset: offset }; }
        }
    }
}

api! {
    client QualifiedCursor {
        base "https://example.test";

        GET read(query offset: u64 = 0) -> Bytes via Binary {
            paginate via OffsetPagination { init OffsetInit { offset: offset }; cursor -> endpoint.offset; }
        }
    }
}

api! {
    client SingleTupleCursor {
        base "https://example.test";

        GET read(query offset: u64 = 0) -> Bytes via Binary {
            paginate via OffsetPagination { init OffsetInit { offset: offset }; cursor -> (offset); }
        }
    }
}

api! {
    client TrailingTokens {
        base "https://example.test";

        GET read(query offset: u64 = 0) -> Bytes via Binary {
            paginate via OffsetPagination { init OffsetInit { offset: offset }; cursor -> offset; extra; }
        }
    }
}

api! {
    client CallInitializer {
        base "https://example.test";

        GET read(query offset: u64 = 0) -> Bytes via Binary {
            paginate via OffsetPagination { init OffsetInit { offset: crate::make() }; cursor -> offset; }
        }
    }
}

api! {
    client MethodInitializer {
        base "https://example.test";

        GET read(query offset: u64 = 0) -> Bytes via Binary {
            paginate via OffsetPagination { init OffsetInit { offset: offset.clone() }; cursor -> offset; }
        }
    }
}

api! {
    client ClosureInitializer {
        base "https://example.test";

        GET read(query offset: u64 = 0) -> Bytes via Binary {
            paginate via OffsetPagination { init OffsetInit { offset: || 1 }; cursor -> offset; }
        }
    }
}

api! {
    client BlockInitializer {
        base "https://example.test";

        GET read(query offset: u64 = 0) -> Bytes via Binary {
            paginate via OffsetPagination { init OffsetInit { offset: { 1 } }; cursor -> offset; }
        }
    }
}

api! {
    client MacroInitializer {
        base "https://example.test";

        GET read(query offset: u64 = 0) -> Bytes via Binary {
            paginate via OffsetPagination { init OffsetInit { offset: format_args!("1") }; cursor -> offset; }
        }
    }
}

api! {
    client DefaultKeywordInitializer {
        base "https://example.test";

        GET read(query offset: u64 = 0) -> Bytes via Binary {
            paginate via OffsetPagination { init OffsetInit { offset: default }; cursor -> offset; }
        }
    }
}

api! {
    client SecretInitializer {
        base "https://example.test";

        GET read(query offset: u64 = 0) -> Bytes via Binary {
            paginate via OffsetPagination { init OffsetInit { offset: secret.offset }; cursor -> offset; }
        }
    }
}

fn main() {}
