// A1 seeded mistake 5: caller code that propagates a declared endpoint's `send()` error with `?`
// into the plain `concord::Error`: the error now carries the document type.
use concord::Json;
#[derive(serde::Deserialize)]
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
async fn call(api: Api) -> Result<(), concord::Error> {
    api.ping().send().await?;
    Ok(())
}
fn main() {
    let _ = call;
}
