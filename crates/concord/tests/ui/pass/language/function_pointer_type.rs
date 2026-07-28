use concord::{Bytes, api};

api! {
    client FunctionPointerApi {
        base "http://example.test";
    }

    GET process(callback: fn(u32) -> bool) -> Bytes;
}

fn accepts_nonzero(value: u32) -> bool {
    value != 0
}

fn main() {
    let endpoint = function_pointer_api::Process::new(accepts_nonzero);
    let client = FunctionPointerApi::new().expect("function-pointer client");
    let _: function_pointer_api::ProcessRequest = client.request(endpoint);
    let _: function_pointer_api::ProcessRequest = client.process(accepts_nonzero);
}
