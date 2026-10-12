extern crate sol_origin_attr;

#[sol_origin_attr::replace_macro_use]
#[macro_use]
extern crate concord;

api! {
    client ForeignBareAttribute {
        base "https://foreign.example.test/";
        timeout 5s;
        GET read() -> () { path ["bare-foreign"]; }
    }
}

fn main() {
    assert!(CONFIG.contains("[\"bare-foreign\"]"), "{CONFIG}");
    println!("FOREIGN_BARE_REPLACING_ATTRIBUTE {CONFIG}");
}
