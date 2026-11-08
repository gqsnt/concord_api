// `operations Name;` with `send`: the policy receives each endpoint's variant through
// `SendOperation<T, Name>`; a policy that only implements `SendWith<T>` is reported at the
// `send` statement with Concord's message.
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
        operations StoreOperation;
        GET ping();
    }
}
fn main() {}
