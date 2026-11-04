concord::api! {
    client Api {
        base "http://x.test";
        query { "k" = "root"; }
        scope outer {
            query { remove "k"; }
            scope inner { GET leaf() { query { override "k" = "bad"; } } }
        }
    }
}
fn main() {}
