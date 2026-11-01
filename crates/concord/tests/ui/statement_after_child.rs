concord::api! {
    pub client Api {
        base "http://x.test";
        GET list();
        path "late";
    }
}
fn main() {}
