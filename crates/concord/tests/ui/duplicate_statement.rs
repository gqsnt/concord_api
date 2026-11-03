concord::api! {
    pub client Api {
        base "http://x.test";
        path "a";
        path "b";
        GET list();
    }
}
fn main() {}
