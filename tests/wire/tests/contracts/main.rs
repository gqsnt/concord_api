//! Generated clients and the shipped values against a local axum server, over HTTP/1.1 and
//! HTTP/2 (see the documentation of this crate for the groups).
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod server;

mod bodies;
mod compression;
mod http2;
mod limits;
mod pages;
mod records;
mod redirects;
mod retries;

use axum::extract::{ConnectInfo, State};
use axum::response::{IntoResponse, Response};
use axum::Router;
use concord::http::StatusCode;
use server::{after_head, Ending, Peer};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

/// What a scripted route answers to one request.
#[derive(Clone, Debug)]
pub enum Answer {
    /// This status, with these headers and an empty body.
    Status(u16, &'static [(&'static str, &'static str)]),
    /// `200` with this text.
    Text(&'static str),
    /// No answer: the connection is cut before a byte of the response is written.
    Drop,
    /// `200` that announces 12 bytes of text, sends 4, and breaks after its head.
    CutBody,
}

/// A router whose route `path`, for any method, gives the answers in order, then `200` with
/// the text `spare`.
pub fn scripted(path: &str, answers: impl IntoIterator<Item = Answer>) -> Router {
    let queue: Arc<Mutex<VecDeque<Answer>>> = Arc::new(Mutex::new(answers.into_iter().collect()));
    Router::new()
        .route(path, axum::routing::any(answer))
        .with_state(queue)
}

async fn answer(
    State(queue): State<Arc<Mutex<VecDeque<Answer>>>>,
    ConnectInfo(peer): ConnectInfo<Peer>,
) -> Response {
    let next = queue.lock().unwrap().pop_front();
    match next.unwrap_or(Answer::Text("spare")) {
        Answer::Status(status, headers) => {
            let mut response = StatusCode::from_u16(status).unwrap().into_response();
            for (name, value) in headers {
                response.headers_mut().insert(*name, value.parse().unwrap());
            }
            response
        }
        Answer::Text(text) => text.into_response(),
        Answer::Drop => {
            peer.link.cut();
            StatusCode::OK.into_response()
        }
        Answer::CutBody => Response::builder()
            .header("content-type", "text/plain")
            .header("content-length", "12")
            .body(after_head(
                peer.link,
                bytes::Bytes::from_static(b"half"),
                Ending::Break,
            ))
            .unwrap(),
    }
}
