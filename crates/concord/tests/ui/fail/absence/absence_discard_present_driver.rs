// V3-E08 / OUTCOME-01 (§10.2): head-only `absent on` requires the present
// representation's `Driver = Buffered`. `Discard` produces `()` through the
// `DiscardDriver` (the accepted body is dropped unpolled), so a head-only
// absence over it is rejected by the sealed head-absence obligation rather
// than by any recognition of the `Option<()>` spelling (§3.1, REP-01).
use concord::{Discard, api};

api! {
    client DiscardAbsence {
        base "https://absence.example.test/";

        DELETE remove(id: String) -> Option<()> via Discard {
            path "items/{id}";
            absent on 404;
        }
    }
}

fn main() {}
