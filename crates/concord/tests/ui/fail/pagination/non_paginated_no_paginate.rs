// CALL-03 / §14.2: page terminals exist only on a statically page-capable
// endpoint; the retired generic `.paginate(PageLimit)` is absent everywhere.
use concord::{Binary, Bytes, api};

api! {
    client PlainApi {
        base "http://plain.example.test/";

        GET read() -> Bytes via Binary;
    }
}

fn main() {
    let client = PlainApi::new().unwrap();
    let _ = client.read().pages(1);
    let _ = client.read().collect_prefix(1);
    let _ = client.read().collect_all(std::num::NonZeroU32::MIN);
    let _ = client.read().paginate(1);
}
