use concord::{Bytes, api};

api! {
    client FunctionPointerApi {
        timeout off;
        base "http://example.test";


    GET process(callback: fn(u32) -> bool) -> Bytes using Bytes;
}}

fn accepts_nonzero(value: u32) -> bool {
    value != 0
}

fn main() {
    let endpoint = FunctionPointerApi::unbound().process(accepts_nonzero);
    let client = FunctionPointerApi::new().expect("function-pointer client");
    let _: function_pointer_api::ProcessRequest = client.request(endpoint);
    let _: function_pointer_api::ProcessRequest = client.process(accepts_nonzero);
}
