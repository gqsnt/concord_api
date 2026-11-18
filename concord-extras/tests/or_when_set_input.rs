//! C8.6: `or_when_set` knows an input by the input of the call, not by the Rust type of the slot
//! that names it. Two inputs behind one slot type are two inputs; one input reached directly and
//! through a slot of your own that forwards to it is one input; of two registrations of one
//! input, the first written moves.
#![allow(clippy::unwrap_used)]

use concord::page::{Call, Paging, Slot};
use concord::{by_cursor, Json};
use concord_testkit::Script;

/// A slot of your own that forwards to another.
#[derive(Clone, Copy)]
struct Forward<S>(S);
impl<C, S: Slot<C>> Slot<C> for Forward<S> {
    type Value = S::Value;
    fn get<'a>(&self, call: &'a C) -> Option<&'a S::Value> {
        self.0.get(call)
    }
    fn set(&self, call: C, value: S::Value) -> C {
        self.0.set(call, value)
    }
}

/// One slot type for either of two inputs.
#[derive(Clone, Copy)]
enum Either<X, Y> {
    X(X),
    Y(Y),
}
impl<C, X: Slot<C, Value = u8>, Y: Slot<C, Value = u8>> Slot<C> for Either<X, Y> {
    type Value = u8;
    fn get<'a>(&self, call: &'a C) -> Option<&'a u8> {
        match self {
            Self::X(slot) => slot.get(call),
            Self::Y(slot) => slot.get(call),
        }
    }
    fn set(&self, call: C, value: u8) -> C {
        match self {
            Self::X(slot) => slot.set(call, value),
            Self::Y(slot) => slot.set(call, value),
        }
    }
}

fn either<C, X, Y>(x: X, y: Y) -> Paging<C>
where
    C: Call<Value = Vec<u8>>,
    X: Slot<C, Value = u8>,
    Y: Slot<C, Value = u8>,
{
    let (a, b): (Either<X, Y>, Either<X, Y>) = (Either::X(x), Either::Y(y));
    by_cursor(x, |_| Some(99))
        .or_when_set(a, by_cursor(x, |_| Some(1)))
        .or_when_set(b, by_cursor(y, |_| Some(2)))
        .has_more(|page| !page.is_empty())
}

concord::api! {
    client Api {
        base "https://api.test";
        read Json;
        part second(y?: u8) { query { y; } }
        GET two(x?: u8, ..second) -> Vec<u8> {
            query { x; }
            paginate either(x, y);
        }
        GET forwarded(x?: u8) -> Vec<u8> {
            query { x; }
            paginate by_cursor(x, |_| Some(0))
                .or_when_set(x, by_cursor(x, |_| Some(1)))
                .or_when_set(Forward(x), by_cursor(x, |_| Some(2)))
                .has_more(|page| !page.is_empty());
        }
        GET twice(x?: u8) -> Vec<u8> {
            query { x; }
            paginate by_cursor(x, |_| Some(0))
                .or_when_set(x, by_cursor(x, |_| Some(1)))
                .or_when_set(x, by_cursor(x, |_| Some(2)))
                .has_more(|page| !page.is_empty());
        }
    }
}

/// Walks `pages` over a first page `first` and an empty second one: the URLs sent, and the end
/// (`Ok` or the reason of the error).
async fn walk(which: &str, first: &'static str) -> (Vec<String>, Result<(), Option<String>>) {
    let script = Script::new();
    script.answer(200).body(first);
    script.answer(200).body("[]");
    let api = Api::new().with_transport(script.clone());
    let mut pages = match which {
        "two" => api.two().x(7).y(8).pages(),
        "forwarded" => api.forwarded().x(7).pages(),
        _ => api.twice().x(7).pages(),
    };
    let end = loop {
        match pages.try_next().await {
            Ok(Some(_)) => {}
            Ok(None) => break Ok(()),
            Err(error) => break Err(error.reason().map(str::to_owned)),
        }
    };
    let urls = script
        .requests()
        .iter()
        .map(|request| request.url().to_string())
        .collect();
    (urls, end)
}

const TWO_MOVES: &str = "the call chooses two moves: it sets two inputs that each choose one";

#[tokio::test]
async fn c8_6_two_inputs_behind_one_slot_type_are_two_inputs() {
    for first in ["[4]", "[]"] {
        let (urls, end) = walk("two", first).await;
        assert_eq!(urls, ["https://api.test/?y=8&x=7"], "{first}");
        assert_eq!(end, Err(Some(TWO_MOVES.to_owned())), "{first}");
    }
}

#[tokio::test]
async fn c8_6_one_input_reached_through_a_slot_of_your_own_is_one_input() {
    let (urls, end) = walk("forwarded", "[4]").await;
    assert_eq!(urls, ["https://api.test/?x=7", "https://api.test/?x=1"]);
    assert_eq!(end, Ok(()));
    let (urls, end) = walk("forwarded", "[]").await;
    assert_eq!(urls, ["https://api.test/?x=7"]);
    assert_eq!(end, Ok(()));
}

#[tokio::test]
async fn c8_6_of_two_registrations_of_one_input_the_first_written_moves() {
    let (urls, end) = walk("twice", "[4]").await;
    assert_eq!(urls, ["https://api.test/?x=7", "https://api.test/?x=1"]);
    assert_eq!(end, Ok(()));
}
