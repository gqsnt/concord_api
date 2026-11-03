// `remove` acts on what an owner inherits, before its own contributions: removing a key the
// owner itself introduces (and nothing above sets) is an error, not a silent no-op.
concord::api! {
    pub client Api {
        base "http://x.test";
        GET page(a: String) { query { "k" = a; remove "k"; } }
    }
}
fn main() {}
