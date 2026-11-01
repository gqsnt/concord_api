concord::api! {
    pub client Api {
        base "http://x.test";
        GET e() { path "{r#self}"; }
        GET f() { path "{_}"; }
    }
}
fn main() {}
