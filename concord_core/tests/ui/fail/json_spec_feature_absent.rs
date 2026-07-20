use concord_core::prelude::Json;

fn main() {
    let _ = std::mem::size_of::<Json<Vec<u32>>>();
}
