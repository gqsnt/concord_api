// §16.2 / RUNTIME-02: the runtime builder has no automatic pagination or
// controller DSL (typed generated endpoints own the page surface) and no
// runtime `absent on` builder (use the explicit buffered representation
// pattern). The v2 `PageLimit` type is removed.
use concord::endpoint::{EndpointId, RuntimeEndpoint};
use concord::http::Method;

fn main() {
    // §16.2: no runtime pagination DSL and no runtime `absent on`.
    let builder = RuntimeEndpoint::builder(EndpointId::new("runtime.page").unwrap(), Method::GET);
    let _ = builder.paginate(1);

    let builder = RuntimeEndpoint::builder(EndpointId::new("runtime.absent").unwrap(), Method::GET);
    let _ = builder.absent_on(404);
}
