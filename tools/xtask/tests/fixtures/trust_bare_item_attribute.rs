extern crate sol_origin_attr;
#[macro_use]
extern crate concord;

#[sol_origin_attr::capture_item]
api! {
    client ForeignBareItemAttribute {
        base "https://foreign.example.test/";
        timeout 5s;
        GET read() -> () { path ["bare-foreign-item"]; }
    }
}

fn main() {
    assert!(CONFIG.contains("[\"bare-foreign-item\"]"), "{CONFIG}");
    println!("FOREIGN_BARE_ITEM_ATTRIBUTE {CONFIG}");
}
