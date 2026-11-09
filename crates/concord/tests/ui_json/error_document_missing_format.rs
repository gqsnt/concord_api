// A1 seeded mistakes 2 and 3: the format is written explicitly (`in F`), and an owner declares
// at most one error document.
#[derive(serde::Deserialize)]
pub struct Problem {
    pub title: String,
}
concord::api! {
    pub client Api {
        base "http://x.test";
        error Problem;
        GET ping();
    }
}
concord::api! {
    pub client Twice {
        base "http://x.test";
        error Problem in concord::Json;
        error Problem in concord::Json;
        GET ping();
    }
}
fn main() {}
