concord::api! {
    client Api(value: String = "root") {
        base "http://x.test";
        scope outer { GET leaf() { query { "v" = value; } } }
    }
}
fn main() {}
