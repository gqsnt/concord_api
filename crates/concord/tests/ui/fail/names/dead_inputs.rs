// V3-L13 / VALUE-06 / §3.5: every ordinary input needs a reachable semantic
// use. A required input used nowhere is rejected even when deprecated (a dead
// required input cannot be retained); an optional or initialized input used
// nowhere is rejected unless explicitly `#[deprecated]` as a no-op migration
// bridge, which is accepted.
#![allow(unused_imports)]

use concord::{Binary, Bytes, api};

api! {
    client DeadRequiredRoot {
        base "https://example.test/";
        values { tenant: String; }

        GET read() -> Bytes via Binary;
    }
}

api! {
    client DeadRequiredDeprecated {
        base "https://example.test/";
        values {
            #[deprecated]
            tenant: String;
        }

        GET read() -> Bytes via Binary;
    }
}

api! {
    client DeadOptionalEndpoint {
        base "https://example.test/";

        GET read(verbose?: bool) -> Bytes via Binary;
    }
}

api! {
    client DeadInitializedScope {
        base "https://example.test/";

        scope pages(page: u32 = 1) {
            GET read() -> Bytes via Binary;
        }
    }
}

// Accepted: an explicitly deprecated optional no-op bridge.
api! {
    client DeprecatedOptionalBridge {
        base "https://example.test/";
        values {
            #[deprecated(note = "no longer sent")]
            locale?: String;
        }

        GET read() -> Bytes via Binary;
    }
}

fn main() {}
