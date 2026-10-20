// LANG-03: a reference names a declared value; a missing name is an error.
use concord::{Binary, Bytes, api};

api! {
    client ReferenceApi {
        base "https://reference.example.test";

        GET item() -> Bytes via Binary {
            path "{missing}";
        }
    }
}

// An owner-qualified reference must name the client or a visible ancestor.
api! {
    client UnknownOwnerApi {
        base "https://reference.example.test";

        scope regional(region: String) {
            path "{region}";
        }

        GET item() -> Bytes via Binary {
            headers { "X-Region" = regional.region; }
        }
    }
}

// The named ancestor exists but does not declare the value.
api! {
    client UnknownAncestorValueApi {
        base "https://reference.example.test";
        values { query tenant: String; }

        GET item() -> Bytes via Binary {
            headers { "X-Region" = client.region; }
        }
    }
}

fn main() {}
