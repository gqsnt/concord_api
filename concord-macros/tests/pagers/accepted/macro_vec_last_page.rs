use concord::Json;
macro_rules! last_page {
    () => {
        |call, page| {
            if page.value().is_empty() {
                None
            } else {
                Some(call)
            }
        }
    };
}
concord::api! {
    pub client Book {
        base "https://probe.test";
        read Json;
        GET entries() -> Vec<u32> { paginate last_page!(); }
    }
}
fn main() {}
