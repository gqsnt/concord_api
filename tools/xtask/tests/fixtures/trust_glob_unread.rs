extern crate concord;
use concord::*;
mod hidden;
api! { client ForeignUnreadChild {
 base "http://foreign.example.test/";
 timeout 5s;
 GET read() -> () { path ["bare-unread-child"]; }
} }
fn main() {
 assert!(CONFIG.contains("[\"bare-unread-child\"]"), "{CONFIG}");
 println!("FOREIGN_UNREAD_CHILD {CONFIG}");
}
