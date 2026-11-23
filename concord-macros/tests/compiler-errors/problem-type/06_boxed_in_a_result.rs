// The wrappers that a return type can have carry the problem of what they wrap, also nested.
use concord::read::{Incoming, ReadError, RecordStream};
#[derive(serde::Deserialize)] pub struct Problem;
pub struct Other;
type Boxed = Box<Result<Box<RecordStream<String, concord::Error<Other>>>, String>>;
pub struct Mine;
impl concord::Reader<Boxed> for Mine {
    async fn read(&self, _: Incoming) -> Result<Boxed, ReadError> { Ok(Box::new(Err(String::new()))) }
}
concord::api! { client C { base "https://c.test"; problem Problem read concord::Json;
GET e() -> Boxed { read Mine; } } }
fn main() {}
