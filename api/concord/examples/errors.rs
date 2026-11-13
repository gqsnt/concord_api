//! Matching on the error: a typed problem, a refused answer, a timeout.
//! Run it with `cargo run --example errors`.
#![allow(clippy::print_stdout)] // An example prints what it shows.

use concord::reqwest::{Request, Response};
use concord::transport::{TransportError, TryInfo};
use concord::{error::Kind, fake, reply_with, Json, Transport};
use serde::Deserialize;

// The error body of this API. `problem Problem;` reads a refusal into it.
#[derive(Debug, Deserialize)]
struct Problem {
    code: String,
}

concord::api! {
    client Shop {
        base "https://shop.example.com";
        read Json;
        problem Problem;
        GET item(id: u64) -> String { path "items/{id}"; }
    }
}

// Every endpoint of `Shop` fails with one type, `shop::Error`. `kind()` says what failed.
fn explain(error: &shop::Error) -> String {
    match error.kind() {
        // The API said no. `problem()` is its body, read into `Problem` when it reads.
        Kind::Refused => match error.problem() {
            Some(problem) => format!("the API said no: {}", problem.code),
            None => "refused; the status, the headers and the body are kept".into(),
        },
        // No answer: a clock ended, or the network failed.
        Kind::Transport if error.is_timeout() => "no answer in time".into(),
        Kind::Transport => "the network failed".into(),
        Kind::Mismatch => "an answer of another shape".into(), // not what was promised
        Kind::Build => "nothing was sent".into(),              // the request could not be built
        _ => "another failure".into(),                         // `Kind` is `#[non_exhaustive]`.
    }
}

// A server in a function: item 9 does not exist; item 5 meets a busy proxy.
fn server(request: &Request) -> Result<Response, TransportError> {
    let json = [("content-type", "application/json")];
    Ok(match request.url().path() {
        "/items/9" => reply_with(404, &json, r#"{ "code": "no_such_item" }"#),
        _ => reply_with(503, &[("content-type", "text/html")], "<html>busy</html>"),
    })
}

// A transport of your own is one method. This one never answers.
struct Silent;
impl Transport for Silent {
    async fn send(&self, _: Request, _: TryInfo) -> Result<Response, TransportError> {
        std::future::pending().await
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let shop = Shop::new().with_transport(fake(server));
    // The clock for silence is 30 seconds by default; this client sets 100 milliseconds.
    let wait = std::time::Duration::from_millis(100);
    let silent = Shop::new().with_transport(Silent).with_idle_timeout(wait);
    for call in [shop.item(9), shop.item(5), silent.item(1)] {
        if let Err(error) = call.await {
            // `Display` is one line: the kind, the status, the endpoint. Never a value.
            println!("{error}\n  {}", explain(&error));
        }
    }
}

// It prints:
// refused: status 404, in shop.item
//   the API said no: no_such_item
// refused: status 503, in shop.item
//   refused; the status, the headers and the body are kept
// transport error: no answer for 0.1 s (with_idle_timeout changes it), in shop.item
//   no answer in time
