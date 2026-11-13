concord::api! {
    client Tasks {
        POST submit(payload: Payload) {
            body make_first(payload);
            body encode_payload(payload)?;
        }
    }
}
fn main() {}
