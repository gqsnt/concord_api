//! A paged endpoint whose pager is put together at run time from a list of rules, in the order
//! the list gives, and a walk through its pages with a script.
//!
//! The endpoint's `paginate` statement calls [`configured`], which reads the [`Spec`] set by
//! the test on the current thread when `.pages()` is called. Every function of the author's
//! that a `Paging` may call (a statement, a move, a count, an edit) writes an [`Event`] to the
//! log of the thread, so a test sees what was read, and in which order.
#![allow(dead_code)]

use concord::error::Kind;
use concord::page::{by_offset_total, Call, IntoItems, Paging, Slot};
use concord::{by_cursor, by_link, by_offset, mismatch, Answer, Fail, Json};
use concord_testkit::Script;
use serde::Deserialize;
use std::cell::RefCell;
use std::error::Error as _;

/// The text of the Mismatch when a page says that more items follow and nothing moves.
pub const MORE_WITHOUT_NEXT: &str = "the page says that more items follow, and gives no next page";
/// The text of the Mismatch when two statements of one page disagree.
pub const DISAGREE: &str = "the page's statements disagree on whether more items follow";
/// The text of the Mismatch when a total cannot be read.
pub const TOTAL: &str = "the total of the list could not be read";
/// The text of the Mismatch when an offset cannot grow.
pub const OFFSET: &str = "the offset cannot grow any further";
/// The text of the Mismatch when a call sets two inputs that each choose the move.
pub const TWO_MOVES: &str = "the call sets more than one input that chooses how the list moves";
/// What every Mismatch about the next page prints.
pub const CANNOT_BE_REQUESTED: &str =
    "mismatch: status 200, the next page cannot be requested, in walk.list";
/// What the failing move of [`Base::Failing`] says.
pub const MOVE_FAILED: &str = "the move failed";

/// One page: its items, two statements, a cursor forward and one backward, and a total.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct Page {
    /// The items.
    #[serde(default)]
    pub items: Vec<u64>,
    /// The first statement: `None` reads as "no more".
    pub a: Option<bool>,
    /// The second statement: `None` reads as "no more".
    pub b: Option<bool>,
    /// The cursor of the next page forward.
    pub next: Option<String>,
    /// The cursor of the next page backward.
    pub prev: Option<String>,
    /// The number of items of the list; absent reads as `-1`, which is not a number of items.
    pub total: Option<i64>,
}

impl IntoItems for Page {
    type Item = u64;
    fn into_items(self) -> Vec<u64> {
        self.items
    }
    fn count(&self) -> usize {
        log(Event::Count);
        self.items.len()
    }
}

/// What a function of the author's did, in the order it happened.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Event {
    /// The statement `a` was read.
    StatementA,
    /// The statement `b` was read.
    StatementB,
    /// The total was read.
    Total,
    /// A move ran: the name of what it reads.
    Move(&'static str),
    /// The page was counted.
    Count,
    /// An edit ran: its number.
    Edit(u8),
}

/// How the value moves before any rule.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Base {
    /// `by_cursor(after, |page| page.next)`.
    Cursor,
    /// `by_offset(start)`.
    Offset,
    /// `by_offset_total(start, |page| page.total)`.
    Total,
    /// `by_link()`.
    Link,
    /// `Paging::from_fn` of a move that always fails.
    Failing,
}

/// An input that a rule names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Input {
    /// `gate`, a number.
    Gate,
    /// `gate2`, a number.
    Gate2,
    /// `before`, a cursor that moves by `page.prev`.
    Before,
    /// `back`, a cursor that moves by `page.prev`.
    Back,
}

/// One method of `Paging`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rule {
    /// `.only_when_set(input)`.
    Gate(Input),
    /// `.has_more(|page| page.a.unwrap_or(false))`.
    MoreA,
    /// `.has_more(|page| page.b.unwrap_or(false))`.
    MoreB,
    /// `.stop_on_short_page(count, default)`.
    Short(u32),
    /// `.stop_on_short_page_of(size)`.
    ShortOf(usize),
    /// `.or_when_set(input, by_cursor(input, |page| page.prev))`.
    Or(Input),
    /// `.map_next(…)`: appends its number to `tag`.
    Edit(u8),
}

impl Rule {
    /// Whether this rule is an edit, the one kind whose order is meaning.
    pub fn is_edit(self) -> bool {
        matches!(self, Rule::Edit(_))
    }
}

/// What `configured` builds: a base, then the rules in this order.
#[derive(Clone, Debug)]
pub struct Spec {
    /// How the value moves.
    pub base: Base,
    /// The methods, in the order they are written.
    pub rules: Vec<Rule>,
}

thread_local! {
    static SPEC: RefCell<Spec> = const { RefCell::new(Spec { base: Base::Cursor, rules: Vec::new() }) };
    static LOG: RefCell<Vec<Event>> = const { RefCell::new(Vec::new()) };
}

/// Records what a function of the author's did.
pub fn log(event: Event) {
    LOG.with(|log| log.borrow_mut().push(event));
}

/// Sets the rules of the next `.pages()` of this thread.
pub fn set(spec: &Spec) {
    SPEC.with(|cell| *cell.borrow_mut() = spec.clone());
    take_log();
}

fn take_log() -> Vec<Event> {
    LOG.with(|log| std::mem::take(&mut *log.borrow_mut()))
}

/// The pager of `walk.list`: the base of the thread's [`Spec`], then each rule, in order.
#[allow(clippy::too_many_arguments)]
pub fn configured<C, A, B, K, S, N, G, H, T>(
    after: A,
    before: B,
    back: K,
    start: S,
    count: N,
    gate: G,
    gate2: H,
    tag: T,
) -> Paging<C>
where
    C: Call<Value = Page>,
    A: Slot<C, Value = String>,
    B: Slot<C, Value = String>,
    K: Slot<C, Value = String>,
    S: Slot<C, Value = u64>,
    N: Slot<C, Value = u32>,
    G: Slot<C, Value = u32>,
    H: Slot<C, Value = u32>,
    T: Slot<C, Value = String>,
{
    let spec = SPEC.with(|spec| spec.borrow().clone());
    let mut paging = match spec.base {
        Base::Cursor => by_cursor(after, |page: &Page| {
            log(Event::Move("after"));
            page.next.clone()
        }),
        Base::Offset => by_offset(start),
        Base::Total => by_offset_total(start, |page: &Page| {
            log(Event::Total);
            page.total.unwrap_or(-1)
        }),
        Base::Link => by_link(),
        Base::Failing => Paging::from_fn(
            |_: C, _: &Answer<Page>| -> Result<Option<C>, Fail<C::Problem>> {
                log(Event::Move("failing"));
                Err(mismatch(MOVE_FAILED))
            },
        ),
    };
    for rule in spec.rules {
        paging = match rule {
            Rule::Gate(Input::Gate) => paging.only_when_set(gate),
            Rule::Gate(Input::Gate2) => paging.only_when_set(gate2),
            Rule::Gate(Input::Before) => paging.only_when_set(before),
            Rule::Gate(Input::Back) => paging.only_when_set(back),
            Rule::MoreA => paging.has_more(|page: &Page| {
                log(Event::StatementA);
                page.a.unwrap_or(false)
            }),
            Rule::MoreB => paging.has_more(|page: &Page| {
                log(Event::StatementB);
                page.b.unwrap_or(false)
            }),
            Rule::Short(default) => paging.stop_on_short_page(count, default),
            Rule::ShortOf(size) => paging.stop_on_short_page_of(size),
            Rule::Or(Input::Before) => paging.or_when_set(
                before,
                by_cursor(before, |page: &Page| {
                    log(Event::Move("before"));
                    page.prev.clone()
                }),
            ),
            Rule::Or(Input::Back) => paging.or_when_set(
                back,
                by_cursor(back, |page: &Page| {
                    log(Event::Move("back"));
                    page.prev.clone()
                }),
            ),
            Rule::Or(Input::Gate) => paging.or_when_set(gate, by_offset(start)),
            Rule::Or(Input::Gate2) => paging.or_when_set(gate2, by_offset(start)),
            Rule::Edit(number) => paging.map_next(move |next: C, _: &Page| {
                log(Event::Edit(number));
                let written = tag.get(&next).cloned().unwrap_or_default();
                tag.set(next, format!("{written}{number}"))
            }),
        };
    }
    paging
}

concord::api! {
    /// A list whose pager is put together at run time.
    pub client Walk {
        base "https://walk.test";
        read Json;
        GET list(
            after?: String,
            before?: String,
            back?: String,
            start?: u64,
            count?: u32,
            gate?: u32,
            gate2?: u32,
            tag?: String,
        ) -> Page {
            path "items";
            query { after; before; back; start; count; gate; gate2; tag; }
            paginate configured(after, before, back, start, count, gate, gate2, tag);
        }
    }
}

/// What the caller sets on the first call.
#[derive(Clone, Debug, Default)]
pub struct Inputs {
    /// `after`.
    pub after: Option<String>,
    /// `before`.
    pub before: Option<String>,
    /// `back`.
    pub back: Option<String>,
    /// `start`.
    pub start: Option<u64>,
    /// `count`.
    pub count: Option<u32>,
    /// `gate`.
    pub gate: Option<u32>,
    /// `gate2`.
    pub gate2: Option<u32>,
}

/// One response of the script: a JSON body, and a `Link` field when there is one.
#[derive(Clone, Debug)]
pub struct Reply {
    /// The body.
    pub body: String,
    /// The value of a `Link` field.
    pub link: Option<String>,
}

/// A page with these items and nothing else.
pub fn items(items: &[u64]) -> Page {
    Page {
        items: items.to_vec(),
        ..Page::default()
    }
}

impl Page {
    /// The same page, with the statement `a`.
    pub fn a(mut self, more: bool) -> Page {
        self.a = Some(more);
        self
    }
    /// The same page, with the statement `b`.
    pub fn b(mut self, more: bool) -> Page {
        self.b = Some(more);
        self
    }
    /// The same page, with a cursor forward.
    pub fn next(mut self, next: &str) -> Page {
        self.next = Some(next.to_owned());
        self
    }
    /// The same page, with a cursor backward.
    pub fn prev(mut self, prev: &str) -> Page {
        self.prev = Some(prev.to_owned());
        self
    }
    /// The same page, with a total.
    pub fn total(mut self, total: i64) -> Page {
        self.total = Some(total);
        self
    }
    /// The response that carries this page.
    pub fn reply(&self) -> Reply {
        Reply {
            body: self.json(),
            link: None,
        }
    }
    /// The response that carries this page and a `Link` field.
    pub fn with_link(&self, link: &str) -> Reply {
        Reply {
            body: self.json(),
            link: Some(link.to_owned()),
        }
    }
    fn json(&self) -> String {
        let mut fields = vec![format!(
            r#""items":[{}]"#,
            self.items
                .iter()
                .map(u64::to_string)
                .collect::<Vec<_>>()
                .join(",")
        )];
        if let Some(a) = self.a {
            fields.push(format!(r#""a":{a}"#));
        }
        if let Some(b) = self.b {
            fields.push(format!(r#""b":{b}"#));
        }
        if let Some(next) = &self.next {
            fields.push(format!(r#""next":"{next}""#));
        }
        if let Some(prev) = &self.prev {
            fields.push(format!(r#""prev":"{prev}""#));
        }
        if let Some(total) = self.total {
            fields.push(format!(r#""total":{total}"#));
        }
        format!("{{{}}}", fields.join(","))
    }
}

/// How a walk ended.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum End {
    /// `Ok(None)`.
    Done,
    /// An error: its kind, its line, and its reason.
    Failed {
        /// `kind()`.
        kind: Kind,
        /// `to_string()`.
        line: String,
        /// `reason()`.
        reason: Option<String>,
    },
}

impl End {
    /// A Mismatch about the next page, with this reason.
    pub fn mismatch(reason: &str) -> End {
        End::Failed {
            kind: Kind::Mismatch,
            line: CANNOT_BE_REQUESTED.to_owned(),
            reason: Some(reason.to_owned()),
        }
    }
    /// The Build error of a next page at another origin.
    pub fn another_origin() -> End {
        End::Failed {
            kind: Kind::Build,
            line: "build error: the URL leaves the origin of the call, in walk.list".to_owned(),
            reason: None,
        }
    }
}

/// What the caller received, and what was sent.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Walked {
    /// The URL of every request, in order.
    pub urls: Vec<String>,
    /// The items of each page, in order (from `.pages()`), or every item (from `.items()`).
    pub pages: Vec<Vec<u64>>,
    /// How it ended.
    pub end: End,
    /// What the functions of the author's did.
    pub events: Vec<Event>,
}

fn script(replies: &[Reply]) -> Script {
    let script = Script::new();
    for reply in replies {
        let mut answer = script
            .answer(200)
            .header("content-type", "application/json");
        if let Some(link) = &reply.link {
            answer = answer.header("link", link);
        }
        answer.body(reply.body.clone());
    }
    script
}

fn call(walk: &Walk, inputs: &Inputs) -> walk::Call<walk::List> {
    let mut call = walk.list();
    if let Some(after) = &inputs.after {
        call = call.after(after.clone());
    }
    if let Some(before) = &inputs.before {
        call = call.before(before.clone());
    }
    if let Some(back) = &inputs.back {
        call = call.back(back.clone());
    }
    if let Some(start) = inputs.start {
        call = call.start(start);
    }
    if let Some(count) = inputs.count {
        call = call.count(count);
    }
    if let Some(gate) = inputs.gate {
        call = call.gate(gate);
    }
    if let Some(gate2) = inputs.gate2 {
        call = call.gate2(gate2);
    }
    call
}

fn ended<E>(error: concord::Error<E>) -> End {
    End::Failed {
        kind: error.kind(),
        line: error.to_string(),
        reason: error.reason().map(str::to_owned),
    }
}

fn urls(script: &Script) -> Vec<String> {
    script
        .requests()
        .iter()
        .map(|request| request.url().as_str().to_owned())
        .collect()
}

/// At most this many pages are pulled: a walk that does not end is a fault of the test.
const MOST: usize = 16;

/// Walks the pages with `.pages()`. After an error, two more pulls must give the end and send
/// nothing.
pub async fn pages(spec: &Spec, inputs: &Inputs, replies: &[Reply]) -> Walked {
    set(spec);
    let script = script(replies);
    let walk = Walk::new().with_transport(script.clone());
    let mut pages = call(&walk, inputs).pages();
    let mut got = Vec::new();
    let mut end = End::Done;
    for _ in 0..MOST {
        match pages.try_next().await {
            Ok(Some(page)) => got.push(page.items),
            Ok(None) => break,
            Err(error) => {
                end = ended(error);
                break;
            }
        }
    }
    if end != End::Done {
        let sent = script.requests().len();
        assert!(
            matches!(pages.try_next().await, Ok(None)),
            "a pull after an error gave more than the end"
        );
        assert!(
            matches!(pages.try_next().await, Ok(None)),
            "a second pull after an error gave more than the end"
        );
        assert_eq!(
            script.requests().len(),
            sent,
            "a pull after an error sent a request"
        );
    }
    Walked {
        urls: urls(&script),
        pages: got,
        end,
        events: take_log(),
    }
}

/// Walks the items with `.pages().items()`, with the same rules as [`pages`]: the items are in
/// one list.
pub async fn items_of(spec: &Spec, inputs: &Inputs, replies: &[Reply]) -> Walked {
    set(spec);
    let script = script(replies);
    let walk = Walk::new().with_transport(script.clone());
    let mut items = call(&walk, inputs).pages().items();
    let mut got = Vec::new();
    let mut end = End::Done;
    for _ in 0..MOST * 8 {
        match items.try_next().await {
            Ok(Some(item)) => got.push(item),
            Ok(None) => break,
            Err(error) => {
                end = ended(error);
                break;
            }
        }
    }
    if end != End::Done {
        let sent = script.requests().len();
        assert!(
            matches!(items.try_next().await, Ok(None)),
            "a pull after an error gave more than the end"
        );
        assert_eq!(
            script.requests().len(),
            sent,
            "a pull after an error sent a request"
        );
    }
    Walked {
        urls: urls(&script),
        pages: vec![got],
        end,
        events: take_log(),
    }
}

/// Walks with `.pages()` and with `.items()`, checks that both sent the same requests and gave
/// the same items and the same end, and returns the walk by pages.
pub async fn walk(spec: &Spec, inputs: &Inputs, replies: &[Reply]) -> Walked {
    let by_page = pages(spec, inputs, replies).await;
    let by_item = items_of(spec, inputs, replies).await;
    assert_eq!(
        by_item.urls, by_page.urls,
        "`.items()` sent other requests than `.pages()`"
    );
    assert_eq!(
        by_item.pages,
        vec![by_page.pages.concat()],
        "`.items()` gave other items than `.pages()`"
    );
    assert_eq!(
        by_item.end, by_page.end,
        "`.items()` ended otherwise than `.pages()`"
    );
    by_page
}

/// Every text an error shows: its line, its `Debug`, its reason and each `source()`.
pub fn texts<E: std::fmt::Debug>(error: &concord::Error<E>) -> Vec<String> {
    let mut texts = vec![error.to_string(), format!("{error:?}")];
    texts.extend(error.reason().map(str::to_owned));
    let mut source = error.source();
    while let Some(cause) = source {
        texts.push(cause.to_string());
        texts.push(format!("{cause:?}"));
        source = cause.source();
    }
    texts
}

/// The URL of `walk.list` with this query.
pub fn url(query: &str) -> String {
    if query.is_empty() {
        "https://walk.test/items".to_owned()
    } else {
        format!("https://walk.test/items?{query}")
    }
}
