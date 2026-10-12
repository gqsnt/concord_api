use concord;
concord::api! {
    client ForeignChild {
        base "http://foreign.example.test/";
        timeout 5s;
        GET read() -> () { path ["child-foreign"]; }
    }
}
pub fn verify() {
    assert!(CONFIG.contains("[\"child-foreign\"]"), "{CONFIG}");
    println!("FOREIGN_USE_CRATE_CHILD {CONFIG}");
}
