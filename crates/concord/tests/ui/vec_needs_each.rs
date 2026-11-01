concord::api! {
    pub client Api {
        base "http://x.test";
        GET list(query tags: Vec<String>);
    }
}
fn main() {}
