//! Tools for testing Concord clients with scripted responses, virtual time, or a local HTTP server.
//!
//! - [`Server`]: a real local HTTP/1.1 server that a test scripts, for what must be
//!   checked on the wire. It records every request exactly as it arrived.
//! - [`Script`]: a [`Transport`](concord_core::Transport) with virtual time, for what
//!   depends on time. No socket: it works under `#[tokio::test(start_paused = true)]`.
//!
//! A test needs these lines in `Cargo.toml`. `start_paused` needs Tokio's `test-util` feature,
//! and it works on the current-thread runtime only (the default flavor of `#[tokio::test]`).
//! A [`Script`] also works on a multi-thread runtime, with real time there: see its documentation.
//!
//! ```toml
//! [dev-dependencies]
//! concord-testkit = "5.0.0-rc.1"
//! tokio = { version = "1", features = ["macros", "rt", "test-util"] }
//! ```
//!
//! # A server
//!
//! ```
//! # use concord_core as concord;
//! use concord_testkit::Server;
//!
//! # #[tokio::main(flavor = "current_thread")]
//! # async fn main() {
//! let server = Server::start().await;
//! server.on("GET", "/tasks").reply(503);
//! server
//!     .on("GET", "/tasks")
//!     .reply(200)
//!     .header("content-type", "application/json")
//!     .body("[]");
//!
//! let client = concord::reqwest::Client::builder().no_proxy().build().unwrap();
//! let url = format!("{}/tasks?page=2", server.url());
//! assert_eq!(client.get(&url).send().await.unwrap().status(), 503);
//! let answer = client.get(&url).send().await.unwrap();
//! assert_eq!(answer.status(), 200);
//! assert_eq!(answer.text().await.unwrap(), "[]");
//!
//! let requests = server.requests();
//! assert_eq!(requests.len(), 2);
//! assert_eq!(requests[1].target(), "/tasks?page=2");
//! # }
//! ```
//!
//! # A script
//!
//! ```
//! # use concord_core as concord;
//! use concord::transport::{Transport, TransportError, TryInfo};
//! use concord_testkit::Script;
//! use std::time::Duration;
//! use tokio::time::Instant;
//!
//! # #[tokio::main(flavor = "current_thread", start_paused = true)]
//! # async fn main() {
//! let script = Script::new();
//! script.fail(TransportError::connect()).after(Duration::from_secs(1));
//! script.answer(200).after(Duration::from_secs(30)).body("ok");
//!
//! let start = Instant::now();
//! let request = || {
//!     let url = "http://api.test/tasks".parse().unwrap();
//!     concord::reqwest::Request::new(concord::reqwest::Method::GET, url)
//! };
//! assert!(script.send(request(), TryInfo::new("tasks.list", 1)).await.is_err());
//! let answer = script.send(request(), TryInfo::new("tasks.list", 2)).await.unwrap();
//! assert_eq!(answer.status(), 200);
//! assert_eq!(start.elapsed(), Duration::from_secs(31)); // virtual time
//!
//! let tries = script.requests();
//! assert_eq!(tries[1].info().number(), 2);
//! assert_eq!(tries[1].at() - start, Duration::from_secs(1));
//! # }
//! ```

#![forbid(unsafe_code)]
#![warn(missing_debug_implementations)]

pub mod script;
pub mod server;

pub use script::Script;
pub use server::{Gate, Server};
