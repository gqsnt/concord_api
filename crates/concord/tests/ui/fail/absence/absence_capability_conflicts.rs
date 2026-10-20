// V3-E08 / OUTCOME-01 (§10.1, §10.2): the expected-absence status set is
// endpoint-owned and disjoint from the accepted payload statuses, and
// head-only absence cannot combine with `expose accepted_bytes` (the buffered
// `NotFound<R>` representation is the raw-bytes route, §10.3). Conflicts are
// `[C3-REP]` declaration errors. One invocation per conflict because each
// closed declaration reports its first violation.
#![allow(unused_imports)]

use concord::{Binary, Bytes, api};

api! {
    client AbsenceWithAcceptedBytes {
        base "https://absence.example.test/";

        GET find(id: String) -> Option<Bytes> via Binary {
            path "items/{id}";
            absent on 404;
            expose accepted_bytes;
        }
    }
}

api! {
    client AbsenceInAcceptedList {
        base "https://absence.example.test/";

        GET find(id: String) -> Option<Bytes> via Binary {
            path "items/{id}";
            status [200, 404];
            absent on 404;
        }
    }
}

api! {
    client AbsenceInAcceptedRange {
        base "https://absence.example.test/";

        GET find(id: String) -> Option<Bytes> via Binary {
            path "items/{id}";
            status 200..=499;
            absent on 404;
        }
    }
}

api! {
    client AbsenceInDefaultSuccess {
        base "https://absence.example.test/";

        // The default accepted set is 200..=299 (`status success`).
        GET find(id: String) -> Option<Bytes> via Binary {
            path "items/{id}";
            absent on 204;
        }
    }
}

fn main() {}
