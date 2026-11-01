concord::api! {
    pub client Api {
        base "http://x.test";
        headers { "Content-Length" = "0"; }
        GET list();
    }
}
fn main() {}
