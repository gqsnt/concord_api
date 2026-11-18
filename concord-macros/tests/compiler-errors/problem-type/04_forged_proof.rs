use concord::read::{RecordFail, RecordStream, Sse, SseEvent, Step};
#[derive(serde::Deserialize)] pub struct Problem;
#[derive(serde::Deserialize)] pub struct Other;
// A proof of the author's own that `Other` is the client's problem type: the seal refuses it.
impl concord::__private::SameProblem<Problem> for Other {}
fn event(event: SseEvent) -> Result<Step<String>, RecordFail<Other>> { Ok(Step::Next(event.data)) }
concord::api! { client C { base "https://c.test"; problem Problem read concord::Json;
GET e() -> RecordStream<String, concord::Error<Other>> { read Sse::new(64).try_map(event).until_end_of_body(); } } }
fn main() {}
