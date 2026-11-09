// A1 seeded mistake 1: the error document's format cannot decode it (`Problem` is not
// `Deserialize`). Reported at the statement's format.
use concord::Json;
pub struct Problem {
    pub title: String,
}
concord::api! {
    pub client Api {
        base "http://x.test";
        error Problem in Json;
        GET ping();
    }
}
fn main() {}
