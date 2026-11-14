#![deny(deprecated)]
#[deprecated]
fn old_key() -> u32 { 1 }
concord::api! {
    client Deprecation {
        base "https://deprecated.test";
        #[deprecated]
        GET get() { query { "key" = old_key(); } }
    }
}
fn main() {}
