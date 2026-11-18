// An arrow written as `Option<T>` is read as `T`; the endpoint's type is still checked whole.
use concord::read::{RecordFail, RecordStream, Sse, SseEvent, Step};
#[derive(serde::Deserialize)] pub struct Problem;
#[derive(serde::Deserialize)] pub struct Other;
fn event(event: SseEvent) -> Result<Step<String>, RecordFail<Other>> { Ok(Step::Next(event.data)) }
concord::api! { client C { base "https://c.test"; problem Problem read concord::Json;
GET e() -> Option<RecordStream<String, concord::Error<Other>>> {
    read Sse::new(64).try_map(event).until_end_of_body();
    status { 404 => None }
} } }
fn main() {}
