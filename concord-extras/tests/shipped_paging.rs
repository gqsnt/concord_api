//! C8.6: the six shipped pagers, now values of `concord::page::Paging`, each on the pages the
//! contract names (an empty page, an unset input, the largest offset, an unreadable `Link`, a
//! next URL at another origin, the request just sent); `by_offset_total`; `Paging::into_fn`,
//! `Paging::from_fn` and `Paging::from_pager`, a pager of the author's that keeps state or
//! fails, and a box of a pager. Then what no error of a `Paging` shows: a value of the page.
#![allow(clippy::unwrap_used, clippy::panic, clippy::indexing_slicing)]

mod walk;

use concord::error::Kind;
use concord::page::{by_offset_total, Call, IntoItems, Pager, Paging, Slot};
use concord::{
    by_cursor, by_link, by_next_url, by_offset, by_offset_limit, by_page, mismatch, Answer, Fail,
    Json,
};
use concord_testkit::Script;
use serde::{Deserialize, Serialize};
use std::cell::Cell;

/// What every Mismatch about the next page prints.
const CANNOT_BE_REQUESTED: &str = "mismatch: status 200, the next page cannot be requested";

/// A page that may state whether more follow, with a cursor or a URL, and a total.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct Stated {
    /// The items.
    #[serde(default)]
    pub items: Vec<u64>,
    /// The statement of the page.
    pub more: Option<bool>,
    /// A second statement, read by a rule written around a converted pager.
    pub outer: Option<bool>,
    /// The cursor or the URL of the next page.
    pub next: Option<String>,
    /// The number of items in the list.
    pub total: Option<i64>,
}

impl IntoItems for Stated {
    type Item = u64;
    fn into_items(self) -> Vec<u64> {
        self.items
    }
    fn count(&self) -> usize {
        self.items.len()
    }
}

/// An offset whose type says that its default is seven.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(transparent)]
pub struct Seven(u64);

impl Default for Seven {
    fn default() -> Seven {
        Seven(7)
    }
}
impl From<u64> for Seven {
    fn from(at: u64) -> Seven {
        Seven(at)
    }
}
impl From<Seven> for u64 {
    fn from(at: Seven) -> u64 {
        at.0
    }
}

thread_local! {
    static INNER: Cell<usize> = const { Cell::new(0) };
    static OUTER: Cell<usize> = const { Cell::new(0) };
}

fn inner_read() -> usize {
    INNER.with(Cell::get)
}

/// Stops after `left` pages, whatever the pager inside says: a pager of the author's that keeps
/// state.
pub struct AtMost<P> {
    /// The pager inside.
    pub pager: P,
    /// How many pages are left.
    pub left: u32,
}

impl<P> AtMost<P> {
    /// At most `left` pages of `pager`.
    pub fn new(pager: P, left: u32) -> AtMost<P> {
        AtMost { pager, left }
    }
}

impl<C: Call, P: Pager<C>> Pager<C> for AtMost<P> {
    fn next_call(
        &mut self,
        call: C,
        page: &Answer<C::Value>,
    ) -> Result<Option<C>, Fail<C::Problem>> {
        self.left = self.left.saturating_sub(1);
        if self.left == 0 {
            return Ok(None);
        }
        self.pager.next_call(call, page)
    }
}

/// A pager of the author's that refuses every page.
pub fn refusing<C: Call>() -> Paging<C> {
    Paging::from_fn(
        |_: C, _: &Answer<C::Value>| -> Result<Option<C>, Fail<C::Problem>> {
            Err(mismatch("the page is not whole"))
        },
    )
}

concord::api! {
    /// One endpoint for each pager.
    pub client Shelf {
        base "https://shelf.test";
        read Json;
        GET offsets(start?: u8) -> Vec<u64> {
            path "offsets";
            query { start; }
            paginate by_offset(start);
        }
        GET seven_offsets(start?: Seven) -> Vec<u64> {
            path "seven";
            query { start; }
            paginate by_offset(start);
        }
        GET limited(start?: u8, count?: u32) -> Vec<u64> {
            path "limited";
            query { start; count; }
            paginate by_offset_limit(start, count);
        }
        GET default_count(start?: u32, count?: u32) -> Vec<u64> {
            path "default-count";
            query { start; count; }
            paginate by_offset(start).stop_on_short_page(count, 20);
        }
        GET seven_limited(start?: Seven, count?: u32) -> Vec<u64> {
            path "seven-limited";
            query { start; count; }
            paginate by_offset_limit(start, count);
        }
        GET limited_stated(start?: u8, count?: u32) -> Stated {
            path "limited-stated";
            query { start; count; }
            paginate by_offset_limit(start, count).has_more(|page| page.more.unwrap_or(false));
        }
        GET numbered(page?: u8) -> Vec<u64> {
            path "numbered";
            query { page; }
            paginate by_page(page, 1);
        }
        GET cursors(after?: String) -> Stated {
            path "cursors";
            query { after; }
            paginate by_cursor(after, |page| page.next.clone());
        }
        GET urls() -> Stated {
            path "urls";
            paginate by_next_url(|page: &Stated| page.next.clone());
        }
        GET links() -> Vec<u64> {
            path "links";
            paginate by_link();
        }
        GET totals(start?: u64) -> Stated {
            path "totals";
            query { start; }
            paginate by_offset_total(start, |page| page.total.unwrap_or(-1));
        }
        GET small_totals(start?: u8) -> Stated {
            path "small-totals";
            query { start; }
            paginate by_offset_total(start, |page| page.total.unwrap_or(-1));
        }
        GET signed_totals(start?: i64) -> Stated {
            path "signed-totals";
            query { start; }
            paginate by_offset_total(start, |page| page.total.unwrap_or(-1));
        }
        GET seven_totals(start?: Seven) -> Stated {
            path "seven-totals";
            query { start; }
            paginate by_offset_total(start, |page| page.total.unwrap_or(-1));
        }
        GET signed_short(after?: String, count?: i32) -> Stated {
            path "signed-short";
            query { after; count; }
            paginate by_cursor(after, |page| page.next.clone()).stop_on_short_page(count, -1);
        }
        GET gated(after?: String, limit?: u32) -> Stated {
            path "gated";
            query { after; limit; }
            paginate by_cursor(after, |page| page.next.clone())
                .map_next(|next, _| next.limit(None))
                .only_when_set(limit);
        }
        GET stated(after?: String) -> Stated {
            path "stated";
            query { after; }
            paginate by_cursor(after, |page| page.next.clone()).has_more(|page| page.more.unwrap_or(false));
        }
        GET stated_fn(after?: String) -> Stated {
            path "stated";
            query { after; }
            paginate by_cursor(after, |page| page.next.clone()).has_more(|page| page.more.unwrap_or(false)).into_fn();
        }
        GET own_fn(after?: String) -> Stated {
            path "stated";
            query { after; }
            paginate Paging::from_fn(move |call, page: &Answer<Stated>| {
                page.value().next.clone().map(|next| after.set(call, next))
            })
            .has_more(|page| page.more.unwrap_or(false));
        }
        GET outer_no_more(after?: String) -> Stated {
            path "stated";
            query { after; }
            paginate Paging::from_fn(
                by_cursor(after, |page: &Stated| page.next.clone())
                    .has_more(|page: &Stated| {
                        INNER.with(|read| read.set(read.get() + 1));
                        page.more.unwrap_or(false)
                    })
                    .into_fn(),
            )
            .has_more(|page| {
                OUTER.with(|read| read.set(read.get() + 1));
                page.outer.unwrap_or(false)
            });
        }
        GET outer_short(after?: String) -> Stated {
            path "stated";
            query { after; }
            paginate Paging::from_fn(
                by_cursor(after, |page: &Stated| page.next.clone())
                    .has_more(|page: &Stated| page.more.unwrap_or(false))
                    .into_fn(),
            )
            .stop_on_short_page_of(3);
        }
        GET at_most(after?: String) -> Stated {
            path "stated";
            query { after; }
            paginate AtMost::new(by_cursor(after, |page| page.next.clone()), 2);
        }
        GET at_most_ruled(after?: String) -> Stated {
            path "stated";
            query { after; }
            paginate Paging::from_pager(AtMost::new(by_cursor(after, |page| page.next.clone()), 2))
                .has_more(|page| page.more.unwrap_or(false));
        }
        GET refused(after?: String) -> Stated {
            path "stated";
            query { after; }
            paginate refusing();
        }
        GET boxed(after?: String) -> Stated {
            path "stated";
            query { after; }
            paginate {
                let pager: Box<dyn Pager<_>> = Box::new(by_cursor(after, |page: &Stated| page.next.clone()));
                pager
            };
        }
        GET boxed_paging(after?: String) -> Stated {
            path "stated";
            query { after; }
            paginate Box::new(by_cursor(after, |page| page.next.clone()).has_more(|page| page.more.unwrap_or(false)));
        }
    }
}

// What the tests share ----------------------------------------------------------------------

/// The next attempt answers `200` with this JSON body and, when given, a `Link` field.
fn give(script: &Script, body: serde_json::Value, link: Option<&str>) {
    let mut answer = script
        .answer(200)
        .header("content-type", "application/json");
    if let Some(link) = link {
        answer = answer.header("link", link);
    }
    answer.body(body.to_string());
}

fn shelf(script: &Script) -> Shelf {
    Shelf::new().with_transport(script.clone())
}

fn urls(script: &Script) -> Vec<String> {
    script
        .requests()
        .iter()
        .map(|request| request.url().as_str().to_owned())
        .collect()
}

fn at(path_and_query: &str) -> String {
    format!("https://shelf.test/{path_and_query}")
}

/// How a walk ended.
#[derive(Debug, PartialEq)]
enum End {
    Done,
    Failed {
        kind: Kind,
        line: String,
        reason: Option<String>,
    },
}

/// Every page of `.pages()`, then how it ended; after an error, the next pull gives the end and
/// sends nothing.
async fn every<T, E>(mut pages: concord::Pages<T, E>, script: &Script) -> (Vec<T>, End)
where
    T: Send,
    E: Send,
{
    let mut got = Vec::new();
    for _ in 0..16 {
        match pages.try_next().await {
            Ok(Some(page)) => got.push(page),
            Ok(None) => return (got, End::Done),
            Err(error) => {
                let sent = script.requests().len();
                assert!(matches!(pages.try_next().await, Ok(None)));
                assert_eq!(script.requests().len(), sent);
                return (
                    got,
                    End::Failed {
                        kind: error.kind(),
                        line: error.to_string(),
                        reason: error.reason().map(str::to_owned),
                    },
                );
            }
        }
    }
    panic!("the pages did not end");
}

fn mismatch_in(endpoint: &str, reason: Option<&str>) -> End {
    End::Failed {
        kind: Kind::Mismatch,
        line: format!("{CANNOT_BE_REQUESTED}, in shelf.{endpoint}"),
        reason: reason.map(str::to_owned),
    }
}

/// A Mismatch about the next page, whatever its reason.
#[track_caller]
fn assert_mismatch(end: &End, endpoint: &str) {
    match end {
        End::Failed { kind, line, .. } => {
            assert_eq!(*kind, Kind::Mismatch);
            assert_eq!(*line, format!("{CANNOT_BE_REQUESTED}, in shelf.{endpoint}"));
        }
        End::Done => panic!("the pages ended in silence, where a Mismatch was due"),
    }
}

fn another_origin(endpoint: &str) -> End {
    End::Failed {
        kind: Kind::Build,
        line: format!("build error: the URL leaves the origin of the call, in shelf.{endpoint}"),
        reason: None,
    }
}

fn ids(pages: &[Stated]) -> Vec<Vec<u64>> {
    pages.iter().map(|page| page.items.clone()).collect()
}

fn page(items: &[u64]) -> serde_json::Value {
    serde_json::json!({ "items": items })
}

fn list(items: &[u64]) -> serde_json::Value {
    serde_json::json!(items)
}

// by_offset ----------------------------------------------------------------------------------

#[tokio::test]
async fn c8_6_by_offset_starts_unset_moves_by_the_items_and_ends_on_an_empty_page() {
    let script = Script::new();
    give(&script, list(&[1, 2, 3]), None);
    give(&script, list(&[4]), None);
    give(&script, list(&[]), None);
    give(&script, list(&[9]), None);
    let (pages, end) = every(shelf(&script).offsets().pages(), &script).await;
    assert_eq!(pages, [vec![1, 2, 3], vec![4], vec![]]);
    assert_eq!(end, End::Done);
    assert_eq!(
        urls(&script),
        [at("offsets"), at("offsets?start=3"), at("offsets?start=4")]
    );
}

#[tokio::test]
async fn c8_6_by_offset_at_the_largest_offset_is_a_mismatch_and_an_empty_page_there_ends() {
    let script = Script::new();
    give(&script, list(&[1, 2, 3, 4, 5, 6]), None);
    let (pages, end) = every(shelf(&script).offsets().start(250).pages(), &script).await;
    assert_eq!(pages.len(), 1);
    assert_eq!(end, mismatch_in("offsets", Some(walk::OFFSET)));
    assert_eq!(urls(&script), [at("offsets?start=250")]);

    let script = Script::new();
    give(&script, list(&[]), None);
    let (_, end) = every(shelf(&script).offsets().start(255).pages(), &script).await;
    assert_eq!(end, End::Done);
}

#[tokio::test]
async fn c8_6_by_offset_counts_an_unset_offset_as_zero_whatever_the_default_of_its_type() {
    let script = Script::new();
    give(&script, list(&[1, 2]), None);
    give(&script, list(&[]), None);
    let (_, end) = every(shelf(&script).seven_offsets().pages(), &script).await;
    assert_eq!(end, End::Done);
    assert_eq!(urls(&script), [at("seven"), at("seven?start=2")]);
}

// by_offset_limit ------------------------------------------------------------------------

#[tokio::test]
async fn c8_6_by_offset_limit_ends_on_a_page_shorter_than_the_count() {
    let script = Script::new();
    give(&script, list(&[1, 2]), None);
    give(&script, list(&[3]), None);
    give(&script, list(&[4]), None);
    let (pages, end) = every(shelf(&script).limited().count(2).pages(), &script).await;
    assert_eq!(pages, [vec![1, 2], vec![3]]);
    assert_eq!(end, End::Done);
    assert_eq!(
        urls(&script),
        [at("limited?count=2"), at("limited?start=2&count=2")]
    );
}

#[tokio::test]
async fn c8_6_by_offset_limit_with_no_count_ends_only_on_an_empty_page() {
    let script = Script::new();
    give(&script, list(&[1, 2]), None);
    give(&script, list(&[3]), None);
    give(&script, list(&[]), None);
    let (pages, end) = every(shelf(&script).limited().pages(), &script).await;
    assert_eq!(pages, [vec![1, 2], vec![3], vec![]]);
    assert_eq!(end, End::Done);
    assert_eq!(
        urls(&script),
        [at("limited"), at("limited?start=2"), at("limited?start=3")]
    );
}

#[tokio::test]
async fn c8_6_by_offset_limit_ends_on_an_empty_page_and_fails_at_the_largest_offset() {
    let script = Script::new();
    give(&script, list(&[]), None);
    let (_, end) = every(shelf(&script).limited().count(5).pages(), &script).await;
    assert_eq!(end, End::Done);
    assert_eq!(script.requests().len(), 1);

    let script = Script::new();
    give(&script, list(&[1, 2, 3, 4, 5, 6]), None);
    let (_, end) = every(
        shelf(&script).limited().start(250).count(6).pages(),
        &script,
    )
    .await;
    assert_eq!(end, mismatch_in("limited", Some(walk::OFFSET)));
}

#[tokio::test]
async fn c8_6_by_offset_limit_counts_an_unset_offset_as_zero_whatever_the_default_of_its_type() {
    let script = Script::new();
    give(&script, list(&[1, 2]), None);
    give(&script, list(&[3]), None);
    let (_, end) = every(shelf(&script).seven_limited().count(2).pages(), &script).await;
    assert_eq!(end, End::Done);
    assert_eq!(
        urls(&script),
        [
            at("seven-limited?count=2"),
            at("seven-limited?start=2&count=2")
        ]
    );
}

#[tokio::test]
async fn c8_6_a_statement_has_authority_over_the_count_of_by_offset_limit() {
    let script = Script::new();
    give(
        &script,
        serde_json::json!({ "items": [1], "more": true }),
        None,
    );
    give(
        &script,
        serde_json::json!({ "items": [2, 3, 4], "more": false }),
        None,
    );
    give(&script, page(&[5]), None);
    let (pages, end) = every(shelf(&script).limited_stated().count(3).pages(), &script).await;
    assert_eq!(ids(&pages), [vec![1], vec![2, 3, 4]]);
    assert_eq!(end, End::Done);
    assert_eq!(
        urls(&script),
        [
            at("limited-stated?count=3"),
            at("limited-stated?start=1&count=3")
        ]
    );
}

#[tokio::test]
async fn c8_6_a_statement_that_says_more_on_an_empty_page_of_by_offset_limit_is_a_mismatch() {
    let script = Script::new();
    give(
        &script,
        serde_json::json!({ "items": [], "more": true }),
        None,
    );
    let (pages, end) = every(shelf(&script).limited_stated().count(3).pages(), &script).await;
    assert_eq!(ids(&pages), [Vec::<u64>::new()]);
    assert_eq!(
        end,
        mismatch_in("limited_stated", Some(walk::MORE_WITHOUT_NEXT))
    );
}

// by_page --------------------------------------------------------------------------------

#[tokio::test]
async fn c8_6_by_page_takes_an_unset_page_as_the_first_and_ends_on_an_empty_page() {
    let script = Script::new();
    give(&script, list(&[1]), None);
    give(&script, list(&[2]), None);
    give(&script, list(&[]), None);
    let (pages, end) = every(shelf(&script).numbered().pages(), &script).await;
    assert_eq!(pages, [vec![1], vec![2], vec![]]);
    assert_eq!(end, End::Done);
    assert_eq!(
        urls(&script),
        [at("numbered"), at("numbered?page=2"), at("numbered?page=3")]
    );
}

#[tokio::test]
async fn c8_6_by_page_at_the_largest_page_number_is_a_mismatch() {
    let script = Script::new();
    give(&script, list(&[1]), None);
    let (pages, end) = every(shelf(&script).numbered().page(255).pages(), &script).await;
    assert_eq!(pages, [vec![1]]);
    assert_mismatch(&end, "numbered");
    assert_eq!(script.requests().len(), 1);
}

// by_cursor --------------------------------------------------------------------------------

#[tokio::test]
async fn c8_6_by_cursor_follows_a_cursor_past_an_empty_page_and_ends_without_one() {
    let script = Script::new();
    give(
        &script,
        serde_json::json!({ "items": [], "next": "c1" }),
        None,
    );
    give(
        &script,
        serde_json::json!({ "items": [1], "next": "c2" }),
        None,
    );
    give(&script, page(&[2]), None);
    let (pages, end) = every(shelf(&script).cursors().pages(), &script).await;
    assert_eq!(ids(&pages), [vec![], vec![1], vec![2]]);
    assert_eq!(end, End::Done);
    assert_eq!(
        urls(&script),
        [
            at("cursors"),
            at("cursors?after=c1"),
            at("cursors?after=c2")
        ]
    );
}

#[tokio::test]
async fn c8_6_by_cursor_that_gives_the_cursor_just_sent_is_a_mismatch() {
    let script = Script::new();
    give(
        &script,
        serde_json::json!({ "items": [1], "next": "c1" }),
        None,
    );
    give(
        &script,
        serde_json::json!({ "items": [2], "next": "c1" }),
        None,
    );
    give(&script, page(&[3]), None);
    let (pages, end) = every(shelf(&script).cursors().pages(), &script).await;
    assert_eq!(ids(&pages), [vec![1], vec![2]]);
    assert_mismatch(&end, "cursors");
    assert_eq!(script.requests().len(), 2);
}

// by_next_url --------------------------------------------------------------------------------

#[tokio::test]
async fn c8_6_by_next_url_resolves_a_relative_url_follows_it_past_an_empty_page_and_ends_without_one(
) {
    let script = Script::new();
    give(
        &script,
        serde_json::json!({ "items": [], "next": "urls?p=2#top" }),
        None,
    );
    give(
        &script,
        serde_json::json!({ "items": [1], "next": "/urls?p=3" }),
        None,
    );
    give(&script, page(&[2]), None);
    let (pages, end) = every(shelf(&script).urls().pages(), &script).await;
    assert_eq!(ids(&pages), [vec![], vec![1], vec![2]]);
    assert_eq!(end, End::Done);
    assert_eq!(urls(&script), [at("urls"), at("urls?p=2"), at("urls?p=3")]);
}

#[tokio::test]
async fn c8_6_by_next_url_at_another_origin_is_a_build_error() {
    let script = Script::new();
    give(
        &script,
        serde_json::json!({ "items": [1], "next": "https://elsewhere.test/urls?p=2" }),
        None,
    );
    let (pages, end) = every(shelf(&script).urls().pages(), &script).await;
    assert_eq!(ids(&pages), [vec![1]]);
    assert_eq!(end, another_origin("urls"));
    assert_eq!(script.requests().len(), 1);
}

#[tokio::test]
async fn c8_6_by_next_url_that_gives_the_url_just_sent_is_a_mismatch() {
    let script = Script::new();
    give(
        &script,
        serde_json::json!({ "items": [1], "next": "/urls" }),
        None,
    );
    let (_, end) = every(shelf(&script).urls().pages(), &script).await;
    assert_mismatch(&end, "urls");
    assert_eq!(script.requests().len(), 1);
}

// by_link ----------------------------------------------------------------------------------

#[tokio::test]
async fn c8_6_by_link_follows_next_past_an_empty_page_skips_an_anchored_link_and_ends_without_one()
{
    let script = Script::new();
    give(&script, list(&[]), Some(r#"</links?p=2>; rel="next""#));
    give(
        &script,
        list(&[1]),
        Some(r##"</links?p=9>; rel="next"; anchor="#x", </links?p=3>; rel="prev next""##),
    );
    give(&script, list(&[2]), Some(r#"</links?p=1>; rel="prev""#));
    let (pages, end) = every(shelf(&script).links().pages(), &script).await;
    assert_eq!(pages, [vec![], vec![1], vec![2]]);
    assert_eq!(end, End::Done);
    assert_eq!(
        urls(&script),
        [at("links"), at("links?p=2"), at("links?p=3")]
    );
}

#[tokio::test]
async fn c8_6_by_link_with_an_unreadable_field_is_a_mismatch_not_the_end() {
    for field in [
        r#"</links?p=2>; rel="next"#,
        r#"</links?p=2; rel="next""#,
        r#"/links?p=2>; rel="next""#,
    ] {
        let script = Script::new();
        give(&script, list(&[1]), Some(field));
        let (pages, end) = every(shelf(&script).links().pages(), &script).await;
        assert_eq!(pages, [vec![1]], "{field}");
        assert!(
            end != End::Done,
            "the field {field} ended the pages in silence"
        );
        assert_mismatch(&end, "links");
        assert_eq!(script.requests().len(), 1);
    }
}

#[tokio::test]
async fn c8_6_by_link_at_another_origin_is_a_build_error_and_the_url_just_sent_a_mismatch() {
    let script = Script::new();
    give(
        &script,
        list(&[1]),
        Some(r#"<https://elsewhere.test/links?p=2>; rel="next""#),
    );
    let (_, end) = every(shelf(&script).links().pages(), &script).await;
    assert_eq!(end, another_origin("links"));

    let script = Script::new();
    give(
        &script,
        list(&[1]),
        Some(r#"<https://shelf.test/links>; rel="next""#),
    );
    let (_, end) = every(shelf(&script).links().pages(), &script).await;
    assert_mismatch(&end, "links");
    assert_eq!(script.requests().len(), 1);
}

// by_offset_total --------------------------------------------------------------------------

fn totalled(items: &[u64], total: i64) -> serde_json::Value {
    serde_json::json!({ "items": items, "total": total })
}

#[tokio::test]
async fn c8_6_by_offset_total_counts_an_unset_offset_as_zero_whatever_the_default_of_its_type() {
    let script = Script::new();
    give(&script, totalled(&[1, 2, 3], 5), None);
    give(&script, totalled(&[4, 5], 5), None);
    let (pages, end) = every(shelf(&script).seven_totals().pages(), &script).await;
    assert_eq!(ids(&pages), [vec![1, 2, 3], vec![4, 5]]);
    assert_eq!(end, End::Done);
    assert_eq!(
        urls(&script),
        [at("seven-totals"), at("seven-totals?start=3")]
    );
}

#[tokio::test]
async fn c8_6_by_offset_total_ends_when_the_total_is_reached_passed_or_below_the_offset() {
    for (start, items, total) in [
        (None, &[1, 2][..], 2),
        (None, &[1, 2, 3][..], 2),
        (Some(10), &[1][..], 5),
        (Some(10), &[][..], 5),
    ] {
        let script = Script::new();
        give(&script, totalled(items, total), None);
        let mut call = shelf(&script).totals();
        if let Some(start) = start {
            call = call.start(start);
        }
        let (pages, end) = every(call.pages(), &script).await;
        assert_eq!(pages.len(), 1);
        assert_eq!(
            end,
            End::Done,
            "start {start:?}, {} items, total {total}",
            items.len()
        );
        assert_eq!(script.requests().len(), 1);
    }
}

#[tokio::test]
async fn c8_6_by_offset_total_reads_a_total_that_changes_between_pages() {
    let script = Script::new();
    give(&script, totalled(&[1, 2], 10), None);
    give(&script, totalled(&[3, 4], 4), None);
    give(&script, totalled(&[5], 10), None);
    let (pages, end) = every(shelf(&script).totals().pages(), &script).await;
    assert_eq!(ids(&pages), [vec![1, 2], vec![3, 4]]);
    assert_eq!(end, End::Done);
    assert_eq!(urls(&script), [at("totals"), at("totals?start=2")]);
}

#[tokio::test]
async fn c8_6_by_offset_total_that_cannot_be_read_is_a_mismatch() {
    for body in [totalled(&[1], -4), page(&[1])] {
        let script = Script::new();
        give(&script, body, None);
        let (pages, end) = every(shelf(&script).totals().pages(), &script).await;
        assert_eq!(pages.len(), 1);
        assert_eq!(end, mismatch_in("totals", Some(walk::TOTAL)));
    }
}

#[tokio::test]
async fn c8_6_by_offset_total_with_an_offset_that_cannot_be_read_is_a_mismatch() {
    let script = Script::new();
    give(&script, totalled(&[1], 50), None);
    let (pages, end) = every(shelf(&script).signed_totals().start(-3).pages(), &script).await;
    assert_eq!(pages.len(), 1);
    assert_mismatch(&end, "signed_totals");
    assert_eq!(script.requests().len(), 1);
}

#[tokio::test]
async fn c8_6_by_offset_total_that_overflows_is_a_mismatch() {
    let script = Script::new();
    give(&script, totalled(&[1, 2, 3], i64::MAX), None);
    let (_, end) = every(shelf(&script).totals().start(u64::MAX - 1).pages(), &script).await;
    assert_mismatch(&end, "totals");
    assert_eq!(script.requests().len(), 1);

    let script = Script::new();
    give(&script, totalled(&[1, 2, 3, 4, 5, 6], 1000), None);
    let (_, end) = every(shelf(&script).small_totals().start(250).pages(), &script).await;
    assert_eq!(end, mismatch_in("small_totals", Some(walk::OFFSET)));
}

#[tokio::test]
async fn c8_6_by_offset_total_with_an_empty_page_below_the_total_is_a_mismatch() {
    let script = Script::new();
    give(&script, totalled(&[1, 2], 5), None);
    give(&script, totalled(&[], 5), None);
    let (pages, end) = every(shelf(&script).totals().pages(), &script).await;
    assert_eq!(ids(&pages), [vec![1, 2], vec![]]);
    assert_eq!(end, mismatch_in("totals", Some(walk::MORE_WITHOUT_NEXT)));
}

// only_when_set ------------------------------------------------------------------------

#[tokio::test]
async fn c8_6_a_gate_reads_the_call_that_fetched_each_page_after_its_edits() {
    let script = Script::new();
    give(
        &script,
        serde_json::json!({ "items": [1], "next": "c1" }),
        None,
    );
    give(
        &script,
        serde_json::json!({ "items": [2], "next": "c2" }),
        None,
    );
    give(&script, page(&[3]), None);
    let (pages, end) = every(shelf(&script).gated().limit(5).pages(), &script).await;
    assert_eq!(ids(&pages), [vec![1], vec![2]]);
    assert_eq!(end, End::Done);
    assert_eq!(urls(&script), [at("gated?limit=5"), at("gated?after=c1")]);
}

// stop_on_short_page ---------------------------------------------------------------------

#[tokio::test]
async fn c8_6_an_omitted_count_uses_the_default_to_stop_without_sending_it() {
    let script = Script::new();
    let first: Vec<_> = (1..=20).collect();
    give(&script, list(&first), None);
    give(&script, list(&[21]), None);
    let (pages, end) = every(shelf(&script).default_count().pages(), &script).await;
    assert_eq!(pages, [first, vec![21]]);
    assert_eq!(end, End::Done);
    assert_eq!(
        urls(&script),
        [at("default-count"), at("default-count?start=20")]
    );
}

#[tokio::test]
async fn c8_6_a_short_page_rule_whose_count_or_default_is_not_a_number_of_items_stops_nothing() {
    for count in [None, Some(-5)] {
        let script = Script::new();
        give(
            &script,
            serde_json::json!({ "items": [1], "next": "c1" }),
            None,
        );
        give(
            &script,
            serde_json::json!({ "items": [], "next": "c2" }),
            None,
        );
        give(&script, page(&[2]), None);
        let mut call = shelf(&script).signed_short();
        if let Some(count) = count {
            call = call.count(count);
        }
        let (pages, end) = every(call.pages(), &script).await;
        assert_eq!(ids(&pages), [vec![1], vec![], vec![2]], "count {count:?}");
        assert_eq!(end, End::Done);
    }
}

// into_fn, from_fn, from_pager ---------------------------------------------------------------

fn stated_pages(script: &Script) {
    give(
        script,
        serde_json::json!({ "items": [1], "more": true, "next": "c1" }),
        None,
    );
    give(
        script,
        serde_json::json!({ "items": [2], "more": true, "next": "c2" }),
        None,
    );
    give(
        script,
        serde_json::json!({ "items": [3], "more": true }),
        None,
    );
}

#[tokio::test]
async fn c8_6_a_paging_turned_into_a_function_decides_as_before() {
    let script = Script::new();
    stated_pages(&script);
    let (as_value, value_end) = every(shelf(&script).stated().pages(), &script).await;
    let value_urls = urls(&script);

    let script = Script::new();
    stated_pages(&script);
    let (as_fn, fn_end) = every(shelf(&script).stated_fn().pages(), &script).await;
    assert_eq!(ids(&as_fn), ids(&as_value));
    assert_eq!(urls(&script), value_urls);
    assert_eq!(
        value_end,
        mismatch_in("stated", Some(walk::MORE_WITHOUT_NEXT))
    );
    assert_eq!(
        fn_end,
        mismatch_in("stated_fn", Some(walk::MORE_WITHOUT_NEXT))
    );
}

#[tokio::test]
async fn c8_6_a_function_of_the_authors_made_a_paging_takes_its_rules() {
    let script = Script::new();
    stated_pages(&script);
    let (pages, end) = every(shelf(&script).own_fn().pages(), &script).await;
    assert_eq!(ids(&pages), [vec![1], vec![2], vec![3]]);
    assert_eq!(end, mismatch_in("own_fn", Some(walk::MORE_WITHOUT_NEXT)));
    assert_eq!(
        urls(&script),
        [at("stated"), at("stated?after=c1"), at("stated?after=c2")]
    );
}

#[tokio::test]
async fn c8_6_an_outer_no_more_around_a_converted_pager_skips_its_statements() {
    let before = inner_read();
    let script = Script::new();
    give(
        &script,
        serde_json::json!({ "items": [1], "more": true, "outer": false, "next": "c1" }),
        None,
    );
    let (pages, end) = every(shelf(&script).outer_no_more().pages(), &script).await;
    assert_eq!(ids(&pages), [vec![1]]);
    assert_eq!(end, End::Done);
    assert_eq!(inner_read() - before, 0, "the inner statement was read");
    assert_eq!(OUTER.with(Cell::get), 1);
}

#[tokio::test]
async fn c8_6_an_outer_short_page_rule_ends_a_list_that_a_converted_pager_says_goes_on() {
    let script = Script::new();
    give(
        &script,
        serde_json::json!({ "items": [1], "more": true, "next": "c1" }),
        None,
    );
    give(&script, page(&[2]), None);
    let (pages, end) = every(shelf(&script).outer_short().pages(), &script).await;
    assert_eq!(ids(&pages), [vec![1]]);
    assert_eq!(end, End::Done);
    assert_eq!(script.requests().len(), 1);
}

#[tokio::test]
async fn c8_6_a_pager_of_the_authors_keeps_its_state_for_one_walk() {
    for twice in 0..2 {
        let script = Script::new();
        give(
            &script,
            serde_json::json!({ "items": [1], "next": "c1" }),
            None,
        );
        give(
            &script,
            serde_json::json!({ "items": [2], "next": "c2" }),
            None,
        );
        give(
            &script,
            serde_json::json!({ "items": [3], "next": "c3" }),
            None,
        );
        let (pages, end) = every(shelf(&script).at_most().pages(), &script).await;
        assert_eq!(ids(&pages), [vec![1], vec![2]], "walk {twice}");
        assert_eq!(end, End::Done);
    }
}

#[tokio::test]
async fn c8_6_a_pager_of_the_authors_made_a_paging_takes_its_rules() {
    let script = Script::new();
    give(
        &script,
        serde_json::json!({ "items": [1], "more": true }),
        None,
    );
    let (_, end) = every(shelf(&script).at_most_ruled().pages(), &script).await;
    assert_eq!(
        end,
        mismatch_in("at_most_ruled", Some(walk::MORE_WITHOUT_NEXT))
    );

    let script = Script::new();
    give(
        &script,
        serde_json::json!({ "items": [1], "more": false, "next": "c1" }),
        None,
    );
    let (_, end) = every(shelf(&script).at_most_ruled().pages(), &script).await;
    assert_eq!(end, End::Done);
    assert_eq!(script.requests().len(), 1);
}

#[tokio::test]
async fn c8_6_a_pager_of_the_authors_that_fails_gives_the_page_then_its_error_then_the_end() {
    let script = Script::new();
    give(
        &script,
        serde_json::json!({ "items": [1, 2], "next": "c1" }),
        None,
    );
    let mut items = shelf(&script).refused().pages().items();
    assert_eq!(items.try_next().await.unwrap(), Some(1));
    assert_eq!(items.try_next().await.unwrap(), Some(2));
    let error = items.try_next().await.unwrap_err();
    assert_eq!(error.kind(), Kind::Mismatch);
    assert_eq!(error.reason(), Some("the page is not whole"));
    assert_eq!(
        error.to_string(),
        format!("{CANNOT_BE_REQUESTED}, in shelf.refused")
    );
    assert_eq!(items.try_next().await.unwrap(), None);
    assert_eq!(script.requests().len(), 1);
}

#[tokio::test]
async fn c8_6_a_box_of_a_pager_and_a_box_of_a_paging_are_pagers() {
    let script = Script::new();
    stated_pages(&script);
    let (pages, end) = every(shelf(&script).boxed().pages(), &script).await;
    assert_eq!(ids(&pages), [vec![1], vec![2], vec![3]]);
    assert_eq!(end, End::Done);

    let script = Script::new();
    stated_pages(&script);
    let (pages, end) = every(shelf(&script).boxed_paging().pages(), &script).await;
    assert_eq!(ids(&pages), [vec![1], vec![2], vec![3]]);
    assert_eq!(
        end,
        mismatch_in("boxed_paging", Some(walk::MORE_WITHOUT_NEXT))
    );
}

#[test]
fn c8_6_a_paging_is_send_and_sync() {
    fn send_sync<T: Send + Sync>() {}
    send_sync::<Paging<shelf::Call<shelf::Stated>>>();
    send_sync::<Paging<walk::walk::Call<walk::walk::List>>>();
}

// What an error never shows ------------------------------------------------------------------

/// A value that a page holds, and that no text of an error may show.
const VALUE: &str = "zq7731";

#[tokio::test]
async fn c8_6_no_error_of_a_paging_shows_a_value_of_the_page() {
    use walk::{items, Base, Input, Inputs, Rule, Spec};
    let cursor = format!("{VALUE}c");
    let cases: Vec<(Spec, Inputs, walk::Reply)> = vec![
        (
            Spec {
                base: Base::Cursor,
                rules: vec![Rule::MoreA, Rule::MoreB],
            },
            Inputs::default(),
            items(&[1]).a(true).b(false).next(&cursor).reply(),
        ),
        (
            Spec {
                base: Base::Cursor,
                rules: vec![Rule::MoreA],
            },
            Inputs::default(),
            items(&[1]).a(true).prev(&cursor).reply(),
        ),
        (
            Spec {
                base: Base::Total,
                rules: vec![],
            },
            Inputs::default(),
            items(&[1]).total(-7731).next(&cursor).reply(),
        ),
        (
            Spec {
                base: Base::Cursor,
                rules: vec![Rule::Or(Input::Before), Rule::Or(Input::Back)],
            },
            Inputs {
                before: Some(format!("{VALUE}b")),
                back: Some(format!("{VALUE}k")),
                ..Inputs::default()
            },
            items(&[1]).next(&cursor).prev(&cursor).reply(),
        ),
        (
            Spec {
                base: Base::Offset,
                rules: vec![],
            },
            Inputs {
                start: Some(u64::MAX - 1),
                ..Inputs::default()
            },
            items(&[1, 2, 3]).next(&cursor).reply(),
        ),
        (
            Spec {
                base: Base::Link,
                rules: vec![],
            },
            Inputs::default(),
            items(&[1]).with_link(&format!(r#"</items?{VALUE}>; rel="next"#)),
        ),
        (
            Spec {
                base: Base::Cursor,
                rules: vec![],
            },
            Inputs {
                after: Some(cursor.clone()),
                ..Inputs::default()
            },
            items(&[1]).next(&cursor).reply(),
        ),
        (
            Spec {
                base: Base::Link,
                rules: vec![],
            },
            Inputs::default(),
            items(&[1]).with_link(&format!(r#"<https://{VALUE}.test/items>; rel="next""#)),
        ),
    ];
    for (number, (spec, inputs, reply)) in cases.into_iter().enumerate() {
        walk::set(&spec);
        let script = Script::new();
        let mut answer = script
            .answer(200)
            .header("content-type", "application/json");
        if let Some(link) = &reply.link {
            answer = answer.header("link", link);
        }
        answer.body(reply.body.clone());
        let walk = walk::Walk::new().with_transport(script.clone());
        let mut call = walk.list();
        if let Some(after) = inputs.after {
            call = call.after(after);
        }
        if let Some(before) = inputs.before {
            call = call.before(before);
        }
        if let Some(back) = inputs.back {
            call = call.back(back);
        }
        if let Some(start) = inputs.start {
            call = call.start(start);
        }
        let mut pages = call.pages();
        pages.try_next().await.unwrap().unwrap();
        let error = pages.try_next().await.unwrap_err();
        for text in walk::texts(&error) {
            assert!(!text.contains(VALUE), "case {number}: {text}");
            assert!(!text.contains("7731"), "case {number}: {text}");
        }
    }
}
