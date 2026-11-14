#![allow(dead_code)]
struct Option; struct Result; struct Some; struct Ok; struct String; struct Vec; struct Box;
mod concord {} mod core {} mod std {}
::concord::api! {
    client Shadow {
        base "https://shadow.test";
        read ::concord::Json;
        GET get() -> u32;
        HEAD head() -> bool { status { 204 => true } }
        POST post();
        GET page(start?: u32) -> ::std::vec::Vec<u32> { query { start; } paginate ::concord::by_offset(start); }
        GET stream() -> ::concord::ByteStream { read ::concord::read::Stream; }
    }
}
fn main() { let _ = Shadow::new().page().pages(); }
