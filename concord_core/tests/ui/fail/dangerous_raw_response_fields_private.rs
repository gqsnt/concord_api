use concord_core::dangerous::RawResponse;

fn inspect(response: RawResponse) {
    let RawResponse { body, .. } = response;
    let _ = body;
}

fn main() {}
