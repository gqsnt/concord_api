// A runtime lookup policy that classifies the operation with a `match` without a
// wildcard arm stops compiling when an endpoint is added and not classified (E0004, naming the
// endpoint's variant). Here `items.remove` was added after the policy was written.
use concord::{Endpoint, SendWith};

#[derive(Default)]
pub struct Policy;

impl<T: Send> SendWith<T> for Policy {
    type Output = Result<T, concord::Error>;
    async fn send<E: Endpoint<T> + Send>(
        &self,
        endpoint: E,
    ) -> Self::Output {
        let operation = __StoreOperation::from_operation(E::operation()).unwrap();
        let _write = match operation {
            __StoreOperation::ItemsList | __StoreOperation::ItemsGet => false,
            __StoreOperation::ItemsCreate => true,
        };
        endpoint.prepare()?.send().await
    }
}

concord::api! {
    pub client Store {
        base "http://x.test";
        send Policy;
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
