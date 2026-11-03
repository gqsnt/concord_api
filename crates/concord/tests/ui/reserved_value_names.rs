concord::api! {
    pub client Api(http: String, __x: String) {
        base "http://x.test";
        query { "h" = http; "x" = __x; }
        GET list();
    }
}
fn main() {}
