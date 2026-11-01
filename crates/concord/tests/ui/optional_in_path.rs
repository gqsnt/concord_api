concord::api! {
    pub client Api {
        base "http://x.test";
        GET item(id?: String) { path "items/{id}"; }
    }
}
fn main() {}
