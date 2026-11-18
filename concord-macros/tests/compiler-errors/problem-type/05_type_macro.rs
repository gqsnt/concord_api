// A return type written as a macro that expands to `R<Local>` the first time and `R<Other>`
// after. The endpoint and its check read one type, the endpoint's own `Value`: the client of
// `Local` returns a stream of `Local`, and a stream of `Other` cannot be taken from it.
use concord::read::{Incoming, ReadError, Reader, RecordFail, RecordStream, Sse, SseEvent, Step};
use std::marker::PhantomData;
#[derive(Debug, serde::Deserialize)] pub struct Local;
#[derive(Debug, serde::Deserialize)] pub struct Other;
type R<P> = RecordStream<u8, concord::Error<P>>;
fn map<P>(_: SseEvent) -> Result<Step<u8>, RecordFail<P>> { Ok(Step::Next(8)) }
/// Reads a stream of any problem type, so that only the declared type decides it.
pub struct Any<P>(PhantomData<fn() -> P>);
impl<P: Send + 'static> Reader<R<P>> for Any<P> {
    async fn read(&self, body: Incoming) -> Result<R<P>, ReadError> {
        Sse::new(100).try_map(map::<P>).until_end_of_body().read(body).await
    }
}
concord::api! { client Sample { base "https://unused.invalid"; problem Local read concord::Json;
GET events() -> type_macro::rows!() { read Any(PhantomData); } } }
async fn other(client: &Sample) {
    let _rows: R<Other> = client.events().await.unwrap();
}
fn main() {}
