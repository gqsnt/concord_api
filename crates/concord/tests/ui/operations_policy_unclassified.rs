// `operations Name;`: a send policy that classifies the operation with a `match` without a
// wildcard arm stops compiling when an endpoint is added and not classified (E0004, naming the
// endpoint's variant). Here `items.remove` was added after the policy was written.
use concord::{Endpoint, SendOperation};

#[derive(Default)]
pub struct Policy;

impl<T: Send> SendOperation<T, StoreOperation> for Policy {
    type Output = Result<T, concord::Error>;
    async fn send<E: Endpoint<T> + Send>(
        &self,
        operation: StoreOperation,
        endpoint: E,
    ) -> Self::Output {
        let _write = match operation {
            StoreOperation::ItemsList | StoreOperation::ItemsGet => false,
            StoreOperation::ItemsCreate => true,
        };
        endpoint.prepare()?.send().await
    }
}

concord::api! {
    pub client Store {
        base "http://x.test";
        send Policy;
        operations StoreOperation;
        scope items {
            path "items";
            GET list();
            GET get(id: u64) { path "{id}"; }
            POST create();
            DELETE remove(id: u64) { path "{id}"; }
        }
    }
}
fn main() {}
