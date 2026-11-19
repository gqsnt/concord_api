//! Concord against a real HTTP server.
//!
//! The other tests of Concord's behaviour on the wire run against a scripted transport, the
//! small HTTP/1.1 server of `concord-testkit`, or HTTP/2 frames written by hand. The tests of
//! this crate put generated clients and the shipped values against a server stack that real
//! services use: axum over hyper and h2, started on `127.0.0.1` for each test, serving
//! HTTP/1.1 and HTTP/2 without TLS. They do not stand for any real API's own semantics.
//!
//! Each test name starts with the contract it checks, in `docs/reference/contracts.md`.
//!
//! | Module of `tests/contracts` | What it puts against the server |
//! |---|---|
//! | `retries` | `Retry::standard()`, an unmarked `POST`, `Retry-After`, a connection dropped before the head and a body cut after it (C4, C6.2, C9.5). |
//! | `http2` | A graceful shutdown while calls are in flight, and many calls on one connection (C4.1, C4.4, C9.5). |
//! | `redirects` | `301`, `302`, `307` and `308`, which `concord::Http` does not follow (C7.1, C7.5). |
//! | `compression` | A gzip response from `tower-http`'s compression layer, with decoding on and off (C7.1, C5.4). |
//! | `bodies` | A body cut short, slow chunks and the idle timeout, a large download, a multipart upload and a repeatable body sent again (C5.4, C7.6, C2.16, C12.12). |
//! | `records` | Server-sent events from axum's `Sse`, and JSON lines, raw and mapped, ended and cut (C11.8 to C11.13). |
//! | `pages` | `Link` headers, next URLs in the body, an empty query and reported page numbers (C8.4, C8.6). |
//! | `limits` | A server that counts the calls in flight for each key and reports its windows (C11.4, C11.5, C11.7). |
//!
//! **The clock.** A test that waits runs on Tokio's paused clock, with the server on the same
//! runtime, so that the server's clock is the client's. The clock is held still while sockets
//! are in use and moved by the test, by steps, only when no exchange is on the wire.
//!
//! Every test opens a socket on `127.0.0.1`: each is listed in `tools/socket-tests.txt`, so that
//! `CONCORD_SKIP_SOCKETS=1` skips them.
