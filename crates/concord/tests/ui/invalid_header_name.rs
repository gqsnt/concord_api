concord::api! {
    pub client Api {
        base "http://x.test";
        headers { "x tag" = "a"; }
        GET list();
    }
}
fn main() {}
