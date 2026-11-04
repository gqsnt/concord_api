concord::api! {
    client Api {
        base "http://x.test";
        scope left(value: String = "left") { GET leaf(); }
        scope right { GET leaf() { query { "value" = left.value; } } }
    }
}
fn main() {}
