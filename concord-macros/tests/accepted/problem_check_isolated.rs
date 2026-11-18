//! The check of a declared type meets none of the author's names: an application's blanket
//! method named like the check's never runs, and a correct typed stream compiles beside it.
use concord::read::{RecordFail, RecordStream, Sse, SseEvent, Step};
use std::sync::atomic::{AtomicUsize, Ordering};

static ASKED: AtomicUsize = AtomicUsize::new(0);

pub trait AnyExt {
    fn problem<P>(&self) {
        ASKED.fetch_add(1, Ordering::SeqCst);
    }
    fn must_name_the_clients_problem_type<P>(&self) {
        ASKED.fetch_add(1, Ordering::SeqCst);
    }
}
impl<T> AnyExt for T {}

#[derive(Debug, serde::Deserialize)]
pub struct Problem {
    pub code: u16,
}

fn event(event: SseEvent) -> Result<Step<String>, RecordFail<Problem>> {
    Ok(Step::Next(event.data))
}

concord::api! {
    client Feed {
        base "https://feed.test";
        problem Problem read concord::Json;
        GET plain() -> String { read concord::Text; }
        GET events() -> RecordStream<String, feed::Error> {
            path "events";
            read Sse::new(1024).try_map(event).until_end_of_body();
        }
    }
}

fn main() {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            let client = Feed::new().with_transport(concord::fake(|request| {
                Ok(if request.url().path().ends_with("/events") {
                    concord::reply(200, "data: one\n\n")
                } else {
                    concord::reply(200, "yes")
                })
            }));
            assert_eq!(client.plain().await.unwrap(), "yes");
            let mut events = client.events().await.unwrap();
            assert_eq!(events.try_next().await.unwrap().as_deref(), Some("one"));
            assert!(events.try_next().await.unwrap().is_none());
            assert_eq!(ASKED.load(Ordering::SeqCst), 0, "the application's method ran");
        });
}
