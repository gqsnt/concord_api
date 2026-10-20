// LANG-03: a visible scope identifier is unique along one ancestor chain so
// an owner-qualified reference can never be ambiguous. Siblings may repeat.
use concord::{Binary, Bytes, api};

api! {
    client ReferenceApi {
        base "https://reference.example.test";

        scope regional(id: String) {
            path "regions/{id}";

            scope regional(code: String) {
                path "codes/{code}";

                GET item() -> Bytes via Binary;
            }
        }
    }
}

// `client` and `endpoint` are reserved owner names.
api! {
    client ReservedScopeApi {
        base "https://reference.example.test";

        scope client() {
            GET item() -> Bytes via Binary;
        }
    }
}

fn main() {}
