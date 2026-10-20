// V3-E08 / OUTCOME-01 (§10.2): head-only `absent on` requires the present
// representation's `Driver = Buffered`. `Binary` produces a `ByteStream`
// through the `Streaming` driver, so the declaration is rejected. The macro
// never recognizes the result-type spelling (§3.1, REP-01): the rejection is
// the sealed head-absence obligation on the selected representation pair.
use concord::{Binary, ByteStream, api};

api! {
    client StreamingAbsence {
        base "https://absence.example.test/";

        GET download(id: String) -> Option<ByteStream> via Binary {
            path "files/{id}";
            absent on 404;
        }
    }
}

fn main() {}
