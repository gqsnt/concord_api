// CALL-03 / §16.2: a bound runtime request has no page terminals (`pages`,
// `collect_prefix`, `collect_all`); runtime operations are one-response or
// stream. The v2 `paginate(PageLimit)` terminal is removed everywhere.
use concord::endpoint::{EndpointId, RuntimeEndpoint};
use concord::http::Method;
use concord::{Binary, Bytes, api};

api! {
    client RuntimePaginationApi {
        timeout 30s;
        base "https://example.test";

        GET ordinary() -> Bytes via Binary;
    }
}

fn main() {
    // §16.2: typed generated endpoints provide the page surface; bound
    // runtime requests have no page terminals (CALL-03).
    let client = RuntimePaginationApi::new().unwrap();
    let runtime = |id: &str| {
        RuntimeEndpoint::builder(EndpointId::new(id.to_owned()).unwrap(), Method::GET)
            .response::<Binary, Bytes>()
            .build()
            .unwrap()
    };
    let _ = client.bind(runtime("runtime.pages")).unwrap().pages(1);
    let _ = client.bind(runtime("runtime.prefix")).unwrap().collect_prefix(1);
    let _ = client.bind(runtime("runtime.all")).unwrap().collect_all(std::num::NonZeroU32::MIN);
}
