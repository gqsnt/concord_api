//! The tour clients, generated from their original descriptions.
use concord::page::Slot;
use concord::{
    by_cursor, by_link, by_next_url, by_offset, by_offset_limit, by_page, form, json, read::Stream,
    Json, Secret, Text,
};
#[doc(inline)]
pub use tour_data::{
    checked, fresh_stream, next_batch, page_cursor, unpack, unseal, Account, AccountRoute,
    Americas, Asia, Batch, Chatty, Conflict, Count, DocId, EmptyTitle, Envelope, Europe, FileEntry,
    FileId, Filter, Item, Match, MatchId, MatchRoute, NewTask, Page, Puuid, Receipt, RiotProblem,
    RiotStatus, Sea, ShapesProblem, Signed, Tally, Task, Trace,
};
include!("tasks.rs");
include!("feed.rs");
include!("riot.rs");
include!("files.rs");
include!("shapes.rs");
