// A send policy that does not implement `SendWith<T>` for an endpoint's output. It derives
// `Default` (the type form's other requirement) so that only the `SendWith` diagnostic is
// checked: rustc spells a missing `Default` as `Default` or `std::default::Default` depending
// on the crate graph, which a fixed expectation cannot follow.
#[derive(Default)]
struct NotAPolicy;
concord::api! {
    pub client Store {
        base "http://x.test";
        send NotAPolicy;
        GET one();
    }
}
fn main() {}
