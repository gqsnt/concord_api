//! A type macro that projects from `Self` is expanded once, in the endpoint, where `Self` is
//! the endpoint: it means what it meant before the check existed.
use concord::read::{RecordFail, RecordStream, Sse, SseEvent, Step};
#[derive(Debug, serde::Deserialize)]
pub struct Local;
pub trait Select {
    type Problem;
}
impl Select for sample::Events {
    type Problem = Local;
}
type R<P> = RecordStream<u8, concord::Error<P>>;
fn reject(_: SseEvent) -> Result<Step<u8>, RecordFail<Local>> {
    Err(RecordFail::problem(Local))
}
macro_rules! selected {
    () => { R<<Self as Select>::Problem> };
}
concord::api! {
    client Sample {
        base "https://unused.invalid";
        problem Local read concord::Json;
        GET events() -> selected!() {
            read Sse::new(100).try_map(reject).until_end_of_body();
        }
    }
}
fn main() {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            let client = Sample::new().with_transport(concord::fake(|_| Ok(concord::reply(200, "data: a\n\n"))));
            let mut rows: R<Local> = client.events().await.unwrap();
            let error = rows.try_next().await.unwrap_err();
            assert!(matches!(error.problem(), Some(Local)));
        });
}
