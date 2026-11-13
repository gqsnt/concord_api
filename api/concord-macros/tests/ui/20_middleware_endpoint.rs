concord::api! {
    client Tasks {
        GET jobs() {
            middleware MyLayer::new(config);
        }
    }
}
fn main() {}
