//! Bounded page and job traversal for Concord SDKs, over plain request factories.
//!
//! A traversal never knows Concord: an SDK adapter supplies a *factory* that turns a key into an
//! owned future (typically a generated operation with the key set, then `send()` and a projection
//! of the typed response into a [`Page`]). The crate schedules those futures, bounds them, orders
//! their outcomes and reports what is left. It spawns no task, needs no runtime and never
//! retries.
//!
//! # Ordinary use: reading a cursor listing
//!
//! | Type | What it is |
//! |---|---|
//! | [`Pages`] ([`Pages::new`]) | an owned dependent chain an SDK can name and return; [`next_page`](Pages::next_page) is `Ok(None)` only on provider exhaustion |
//! | [`PageItems`] ([`Pages::items`]) | the same chain item by item, with item-level checkpoints |
//! | [`ItemStream`] ([`PageItems::into_stream`]) | a `futures_core::Stream` of `Result<item, PagesError>` |
//! | [`PageItemEvents`] ([`Pages::item_events`]) | items and page metadata, including final completion metadata |
//!
//! The end is explicit: a cap (opt-in, [`Pages::max_pages`]; there is none by default) reached
//! while the provider has a next page, and a failed page, are [`PagesError`]s carrying the page
//! to continue from, and no call after them returns `Ok(None)`. `collect_complete` returns the
//! whole listing or an [`Incomplete`] prefix with its error, never a silent prefix. The facade
//! erases the factory and future types, so they must be `Send + 'static` (and the items `Send`);
//! `finish` returns the engine's reports below.
//!
//! # The engine: every traversal shape
//!
//! | Entry point | Shape | Typical provider |
//! |---|---|---|
//! | [`sequential`] → [`Sequential`] | dependent chain: the next key comes from the previous page | opaque body cursor, `Link: rel="next"`, next-page URL |
//! | [`Sequential::items`], [`resume_items`] → [`Items`] | the same chain flattened into items, with item-level checkpoints | |
//! | [`independent`] → [`Independent`] | a known plan of independent keys, bounded window | page numbers or offsets the provider lets you address independently |
//! | [`first_page_then`] → [`Independent`] | the first page fixes the plan (a total), then independent keys | `total_pages` / `total_count` envelopes |
//! | [`manifest_jobs`] → [`Jobs`] | a sequential manifest chain discovers job descriptors; each job runs a whole streamed download | result manifests listing result URLs |
//!
//! Each engine traversal has an `async fn next(&mut self)` that returns `None` when it ended for
//! *any* reason (exhaustion, budget, failure), cheap `progress()` counters, a `checkpoint()`
//! snapshot and `finish(self)`, which drops what is in flight and returns a report whose
//! [`Outcome`] says how it ended. Engine traversals require neither `Send` nor `'static`, and
//! their types name the factory and future types. [`JobsReport::into_resume`] and
//! [`IndependentReport::into_resume`] decompose a report without loss for a rerun.
//!
//! Keys (cursors, page numbers, job descriptors) must be `Clone`: the traversal keeps a copy of
//! each admitted key until its outcome is emitted, to report failed and restart identities. Items,
//! metadata, results and errors are moved, never cloned, and need no `Clone`; in the engine they
//! need no `Send` or `'static` either.
//!
//! # Budgets and bounds
//!
//! - **Logical page (or job) budget** ([`Budget`]): factory invocations, charged when a future is
//!   admitted, so failures cannot bypass it; it includes the first page of [`first_page_then`].
//!   It is not an HTTP-attempt budget: a factory that retries or sends subrequests internally makes
//!   more requests than its budget. Retry and account-wide rate limits are policies you compose in
//!   the factory; this crate performs none.
//! - **Discovery budget** ([`JobOptions::new`]): manifest calls, empty manifests included.
//! - **Execution window** ([`Options::window`]): futures in flight plus completed results
//!   retained for ordered emission.
//! - **Descriptor backlog** ([`JobOptions::backlog`]): discovered descriptors waiting for a job
//!   slot.
//! - **Discovery concurrency**: one manifest call at a time, beside the job window (`window + 1`
//!   futures at most).
//! - **Bytes and records**: not here. The window is not a byte bound; body and record limits belong
//!   to the reader at the codec boundary (for example the `concord-extras` stream readers).
//!
//! Constructing a traversal performs no call; polling `next` admits bounded work; a slow consumer
//! only stops admission. These bounds cover what the traversal retains; storage a caller keeps
//! (every yielded page, a collected stream) is the caller's.
//!
//! # Outcomes, failures and restart points
//!
//! A report's [`Outcome`] distinguishes provider exhaustion, a configured bound, an observed
//! failure, the caller's stop and incomplete discovery, and a report keeps the *failed* identity
//! apart from the *restart* identity. Errors are the factory's own typed values, returned by
//! `next`, never stringified. A dependent chain cannot continue after a failure (there is no next
//! key, and none is guessed); independent plans and jobs can ([`OnError::Continue`]), with keyed
//! outcomes in plan order or completion order ([`Order`]).
//!
//! # Progress and storage
//!
//! Progress is exposed while consuming, not only at the end: `checkpoint()` on every traversal,
//! item-level [`ItemCheckpoint`]s for [`Items`], sparse [`Checkpoint`]s (admitted count plus
//! unfinished keys) for independent plans and jobs. What the library retains is explicit: the
//! window's futures and retained results, the key of each failed page (bounded by
//! [`Options::failure_limit`] or the budget), and for jobs the backlog. Successful outcomes go to
//! the caller and are not remembered; durable ledgers are the caller's.
//!
//! A checkpoint belongs to the plan it came from: the same tenant, query, filter, sort, page size
//! and snapshot. Resuming it against another is the caller's error and is not detected (keys are
//! the provider's opaque values). An expired cursor is the factory's error; nothing silently
//! restarts from the first page.
//!
//! # Cancellation
//!
//! `next` (and [`Pages::next_page`], [`PageItems::next`], and a pending `next()` of an
//! [`ItemStream`]) is cancellation-safe: owned futures stay in the traversal between calls, so
//! the following call resumes the same request instead of making another. Dropping a
//! traversal (or `finish`) drops every owned future; nothing detached keeps fetching. Requests
//! already transmitted may still execute on the server, and sink records already written stay
//! written: a job cleans up its own partial output.
//!
//! # Diagnostics
//!
//! No `Debug` or `Display` of this crate prints a key, cursor, URL, item or metadata value: they
//! print counts, indices and outcomes. Underlying errors keep their own types and formatting
//! policy, reachable through the public fields.

mod independent;
mod jobs;
#[cfg(feature = "link")]
pub mod link;
mod pages;
mod sequential;
mod types;
mod window;

pub use independent::{
    Independent, IndependentReport, IndependentResume, first_page_then, independent,
};
pub use jobs::{
    DiscoveryReport, JobOptions, Jobs, JobsCheckpoint, JobsError, JobsProgress, JobsReport,
    JobsResume, manifest_jobs,
};
pub use pages::{
    Incomplete, ItemStream, PageItemEvent, PageItemEvents, PageItems, Pages, PagesError,
};
pub use sequential::{
    ItemCheckpoint, Items, ItemsProgress, ItemsReport, Sequential, SequentialReport, resume_items,
    sequential,
};
pub use types::{
    Budget, Checkpoint, Keyed, Manifest, OnError, Options, Order, Outcome, Page, Progress, Resume,
    Unfinished,
};

/// Compiles the README's example.
#[cfg(doctest)]
#[doc = include_str!("../README.md")]
pub struct ReadmeDoctests;
