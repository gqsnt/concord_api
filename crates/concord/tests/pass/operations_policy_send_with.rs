// Every client dispatches through SendWith.
use concord::{Endpoint, SendWith};

#[derive(Default)]
pub struct Policy;

impl<T: Send> SendWith<T> for Policy {
    type Output = Result<T, concord::Error>;
    async fn send<E: Endpoint<T> + Send>(&self, endpoint: E) -> Self::Output {
        endpoint.prepare()?.send().await
    }
}

concord::api! {
    pub client Store {
        base "http://x.test";
        send Policy;
        GET ping();
    }
}
fn main() {}
