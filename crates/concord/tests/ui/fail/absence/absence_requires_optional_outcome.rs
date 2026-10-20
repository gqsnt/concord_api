// V3-E08 / OUTCOME-01 (§10.2): the result of an endpoint with `absent on`
// must implement the library-sealed `OptionalOutcome` relationship, which
// exists only for `Option<T>` (and aliases of it). A non-`Option` result is
// rejected through that trait, not through token matching.
use concord::{Binary, Bytes, api};

api! {
    client PlainAbsence {
        base "https://absence.example.test/";

        GET find(id: String) -> Bytes via Binary {
            path "items/{id}";
            absent on 404;
        }
    }
}

fn main() {}
