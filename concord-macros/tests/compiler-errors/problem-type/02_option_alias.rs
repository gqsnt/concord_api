use concord::read::{Incoming, ReadError, RecordStream};
#[derive(serde::Deserialize)] pub struct Problem;
pub struct Other;
type Maybe = Option<RecordStream<String, concord::Error<Other>>>;
pub struct Mine;
impl concord::Reader<Maybe> for Mine {
    async fn read(&self, _: Incoming) -> Result<Maybe, ReadError> { Ok(None) }
}
concord::api! { client C { base "https://c.test"; problem Problem read concord::Json;
GET e() -> Maybe { read Mine; } } }
fn main() {}
