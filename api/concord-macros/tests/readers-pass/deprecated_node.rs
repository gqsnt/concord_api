#![deny(deprecated)]
::concord::api! {
    client Deprecation {
        base "https://deprecated.test";
        #[deprecated]
        scope old { GET get(); }
        #[deprecated]
        GET legacy();
    }
}
fn main() {}
