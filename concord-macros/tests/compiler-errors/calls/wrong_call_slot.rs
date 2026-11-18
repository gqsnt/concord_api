use concord::{page::Slot, Json};
concord::api! {
    pub client Api {
        base "https://calls.test/";
        GET other();
        GET list(start?: u32) -> Vec<u32> {
            query { start; }
            read Json;
            paginate move |call, _| {
                let other = Api::new().other();
                let _ = start.get(&other);
                let _ = start.set(other, 10);
                Some(call)
            };
        }
    }
}
fn main() {}
