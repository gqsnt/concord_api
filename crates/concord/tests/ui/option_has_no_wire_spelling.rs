concord::api! {
    pub client Api {
        base "http://x.test";
        GET list(query filter: Option<String>);
    }
}
fn main() {}
