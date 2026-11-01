concord::api! {
    pub client Api {
        base "http://x.test";
        scope s { query { remove "page"; } GET list(); }
    }
}
fn main() {}
