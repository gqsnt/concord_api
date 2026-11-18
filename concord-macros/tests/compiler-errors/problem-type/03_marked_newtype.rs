use concord::read::{Incoming, ReadError, RecordStream};
#[derive(serde::Deserialize)] pub struct Problem;
pub struct Other;
pub struct Feed<P>(pub RecordStream<String, concord::Error<P>>);
impl<P> concord::error::CarriesProblem for Feed<P> { type Problem = P; }
pub struct Mine;
impl concord::Reader<Feed<Other>> for Mine {
    async fn read(&self, _: Incoming) -> Result<Feed<Other>, ReadError> { Err(ReadError::wrong_content_type()) }
}
concord::api! { client C { base "https://c.test"; problem Problem read concord::Json;
GET e() -> Feed<Other> { read Mine; } } }
fn main() {}
