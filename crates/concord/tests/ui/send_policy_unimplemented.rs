// A send policy that does not implement `SendWith<T>` for an endpoint's output.
struct NotAPolicy;
concord::api! {
    pub client Store {
        base "http://x.test";
        send NotAPolicy;
        GET one() -> String { response concord::Text; }
    }
}
fn main() {}
