//! Trailing slashes after splat path inputs.

#[test]
fn c2_3_a_slash_after_a_final_splat_reaches_the_wire() {
    concord::api! {
        client Splat {
            base "https://splat.test";
            GET bare(file: String) { path "{*file}/"; }
            GET prefixed(file: String) { path "before/{*file}/"; }
        }
    }
    let client = Splat::new();
    assert_eq!(client.bare("a/b").request().unwrap().url().path(), "/a/b/");
    assert_eq!(
        client.prefixed("a/b").request().unwrap().url().path(),
        "/before/a/b/"
    );
    assert_eq!(
        client.bare("a/b/").request().unwrap().url().path(),
        "/a/b//"
    );
}
