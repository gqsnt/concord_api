::concord::api! {
    client Capture {
        base "https://capture.test";
        GET get() { query { "value" = { __draft.query("injected", &[]); 1 }; } }
    }
}
fn main() {}
