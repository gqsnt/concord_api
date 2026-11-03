concord::api! {
    pub client Api {
        base "http://x.test";
        scope s { path "items/"; GET list(); }
    }
}
fn main() {}
