concord::api! {
    pub client Api {
        base "http://x.test";
        GET item(id: u64, copy: u64 = id) { path "items/{id}/{copy}"; }
    }
}
fn main() {}
