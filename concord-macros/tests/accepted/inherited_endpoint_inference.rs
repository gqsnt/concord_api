//! A part's macro expands in each endpoint and uses that endpoint's inferred types.
use std::sync::{Arc, Mutex};
use type_macro::alternate;

struct Text(String);
struct Number(u32);

impl Text {
    fn finish(self) -> Self {
        Self(self.0 + "!")
    }
}
impl Number {
    fn finish(self) -> Self {
        Self(self.0 + 1)
    }
}

type FinishText = fn(Text) -> Text;
type FinishNumber = fn(Number) -> Number;

macro_rules! finish {
    () => {
        |value| value.finish()
    };
}

macro_rules! next {
    ($call:ident) => {
        $call.advance_page()
    };
}

concord::api! {
    client Probe {
        base "https://example.invalid";
        part finish() {
            status { 2xx => alternate!(arm (finish!()) (|value| value.finish().finish())) }
        }
        part paging(offset?: u32) {
            query {offset;}
            read concord::Json;
            paginate move |call, _| alternate!(pager (next!(call)) ({ let _ = call; None }));
        }
        GET text(..finish) -> FinishText { path "text"; }
        GET number(..finish) -> FinishNumber { path "number"; }
        GET first(..paging) -> Vec<u8> { path "first"; }
        GET second(..paging) -> Vec<u8> { path "second"; }
    }
}

impl probe::Call<probe::First> {
    fn advance_page(self) -> Option<Self> {
        Some(self.offset(1))
    }
}
impl probe::Call<probe::Second> {
    fn advance_page(self) -> Option<Self> {
        Some(self.offset(2))
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let requests = Arc::new(Mutex::new(Vec::new()));
    let log = Arc::clone(&requests);
    let api = Probe::new().with_transport(concord::fake(move |request| {
        log.lock().unwrap().push(request.url().to_string());
        Ok(concord::reply(200, "[7]"))
    }));

    let text: FinishText = api.text().await.unwrap();
    let number: FinishNumber = api.number().await.unwrap();
    assert_eq!(text(Text(String::from("text"))).0, "text!");
    assert_eq!(number(Number(7)).0, 9);

    let mut first = api.first().pages();
    assert_eq!(first.try_next().await.unwrap(), Some(vec![7]));
    assert_eq!(first.try_next().await.unwrap(), Some(vec![7]));
    let mut second = api.second().pages();
    assert_eq!(second.try_next().await.unwrap(), Some(vec![7]));
    assert_eq!(second.try_next().await.unwrap(), None);

    assert_eq!(
        *requests.lock().unwrap(),
        [
            "https://example.invalid/text",
            "https://example.invalid/number",
            "https://example.invalid/first",
            "https://example.invalid/first?offset=1",
            "https://example.invalid/second"
        ]
    );
}
