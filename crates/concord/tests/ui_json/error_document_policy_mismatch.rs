// A1 seeded mistake 4: a send policy written for endpoints without an error document
// (`SendWith<T>`, i.e. `SendWith<T, ()>`) named by a client that declares one.
use concord::{Endpoint, Json, SendWith};
#[derive(serde::Deserialize)]
pub struct Problem {
    pub title: String,
}
#[derive(Default)]
pub struct Policy;
impl<T: Send> SendWith<T> for Policy {
    type Output = Result<T, concord::Error>;
    async fn send<E: Endpoint<T> + Send>(&self, endpoint: E) -> Self::Output {
        endpoint.prepare()?.send().await
    }
}
concord::api! {
    pub client Api {
        base "http://x.test";
        format Json;
        send Policy;
        error Problem in Json;
        GET ping();
    }
}
fn main() {}
