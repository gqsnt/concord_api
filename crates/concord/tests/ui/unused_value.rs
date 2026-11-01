concord::api! {
    pub client Api(tenant: String) {
        base "http://x.test";
        GET list(limit?: u32);
    }
}
fn main() {}
