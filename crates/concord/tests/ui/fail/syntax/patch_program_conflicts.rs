// PATCH-02 / §6.1-6.2: query/header patch programs are checked structurally.
// An introduction conflicts with any earlier potential writer of the same key
// (every source shape; the diagnostic names both writers); `replace`,
// `when_present` and `remove` need an earlier writer; `when_present` needs an
// outer-optional source; `query off`/`headers off` cannot coexist with direct
// placements on the same owner. Header names compare case-insensitively.
#![allow(unused_imports)]

use concord::{Binary, Bytes, api};

// Inherited literal writer vs. an endpoint introduction of the same key.
api! {
    client InheritedIntroduction {
        base "https://example.test/";
        query { "locale" = "fr"; }

        GET read(locale: String) -> Bytes via Binary {
            query { "locale" = locale; }
        }
    }
}

// Direct placement (step 2) vs. an explicit statement (step 3) on one owner.
api! {
    client DirectThenExplicit {
        base "https://example.test/";

        GET read(query page: u32) -> Bytes via Binary {
            query { "page" = 1; }
        }
    }
}

// Header names are one key under different casing.
api! {
    client HeaderCaseIntroduction {
        base "https://example.test/";
        headers { "X-Trace" = "client"; }

        GET read() -> Bytes via Binary {
            headers { "x-trace" = "endpoint"; }
        }
    }
}

// `replace` of a key nothing earlier writes (a misspelled override target).
api! {
    client ReplaceWithoutWriter {
        base "https://example.test/";
        query { "locale" = "fr"; }

        GET read() -> Bytes via Binary {
            query { replace "lcoale" = "de"; }
        }
    }
}

// `when_present` without fallback is not silently an introduction.
api! {
    client WhenPresentWithoutWriter {
        base "https://example.test/";

        GET read(locale?: String) -> Bytes via Binary {
            query { when_present "locale" = locale; }
        }
    }
}

// `remove` of a header nothing earlier writes.
api! {
    client RemoveWithoutWriter {
        base "https://example.test/";

        GET read() -> Bytes via Binary {
            headers { remove "x-trace"; }
        }
    }
}

// `when_present` with a total source would always replace.
api! {
    client WhenPresentTotalSource {
        base "https://example.test/";
        query { "locale" = "fr"; }

        GET read(locale: String) -> Bytes via Binary {
            query { when_present "locale" = locale; }
        }
    }
}

// `query off` with a direct query placement on the same owner.
api! {
    client QueryOffWithDirectPlacement {
        base "https://example.test/";

        GET read(query page: u32) -> Bytes via Binary {
            query off;
        }
    }
}

// `headers off` with a direct header placement on the same owner.
api! {
    client HeadersOffWithDirectPlacement {
        base "https://example.test/";

        scope traced(header("x-trace") trace: String) {
            headers off;

            GET read() -> Bytes via Binary;
        }
    }
}

fn main() {}
