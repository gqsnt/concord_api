//! C8.4 and C8.6: the decision for one page, including its next call and any deferred error.

// PANIC-SAFETY: Generated indices and test helpers belong to a test executable; failures report a case.
#![allow(clippy::unwrap_used, clippy::indexing_slicing)]

use concord::error::Kind;
use concord::page::{by_offset_total, Call, InputId, IntoItems, Pager, Paging, Slot};
use concord::{by_cursor, by_link, by_next_url, by_offset, by_offset_limit, by_page, Answer, Fail};
use concord_testkit::Script;
use proptest::{
    prelude::*,
    test_runner::{TestCaseResult, TestRunner},
};
use serde::{Deserialize, Serialize};
use std::cell::{Cell, RefCell};
use std::sync::{Arc, Mutex};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Request {
    inputs: [Option<i16>; 3],
    wide: Option<u64>,
    url: Option<String>,
    edits: Vec<u8>,
}

impl Call for Request {
    type Value = Sheet;
    type Problem = ();

    fn at_url(mut self, url: &str) -> Self {
        self.url = Some(url.into());
        self
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Field {
    Start = 0,
    Count = 1,
    Gate = 2,
}

impl Slot<Request> for Field {
    type Value = i16;

    fn get<'a>(&self, call: &'a Request) -> Option<&'a i16> {
        call.inputs[*self as usize].as_ref()
    }

    fn set(&self, mut call: Request, value: i16) -> Request {
        call.inputs[*self as usize] = Some(value);
        call
    }

    fn input(&self) -> InputId {
        InputId::new(*self as usize)
    }
}

#[derive(Clone, Copy)]
struct Forwarded {
    first: Field,
    later: Field,
}

thread_local! {
    static LATER: Cell<bool> = const { Cell::new(false) };
    static VISITS: RefCell<Vec<Visit>> = const { RefCell::new(Vec::new()) };
}

impl Forwarded {
    fn field(self) -> Field {
        if LATER.get() {
            self.later
        } else {
            self.first
        }
    }
}

fn visit(event: Visit) {
    VISITS.with_borrow_mut(|visits| visits.push(event));
}

impl Slot<Request> for Forwarded {
    type Value = i16;
    fn get<'a>(&self, call: &'a Request) -> Option<&'a i16> {
        self.field().get(call)
    }
    fn set(&self, call: Request, value: i16) -> Request {
        self.field().set(call, value)
    }
    fn input(&self) -> InputId {
        self.field().input()
    }
}

#[derive(Clone, Copy)]
struct Wide;

impl Slot<Request> for Wide {
    type Value = u64;
    fn get<'a>(&self, call: &'a Request) -> Option<&'a u64> {
        call.wide.as_ref()
    }
    fn set(&self, mut call: Request, value: u64) -> Request {
        call.wide = Some(value);
        call
    }
    fn input(&self) -> InputId {
        InputId::new(3)
    }
}

const URL: &str = "/next?y=%2f&y=two";

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
enum Link {
    Missing,
    Anchored,
    Next,
    BrokenAfterNext,
}

impl Link {
    fn fields(self) -> &'static [&'static str] {
        match self {
            Self::Missing => &[],
            Self::Anchored => &[r#"</wrong>; rel=next; anchor="/elsewhere""#],
            Self::Next => &[
                r#"</wrong>; rel=next; anchor="/elsewhere""#,
                r#"</next?y=%2f&y=two#aside>; title="a,b;c"; rel="prev next""#,
            ],
            Self::BrokenAfterNext => &[r#"</next>; rel=next"#, r#"</bad>; title="unterminated"#],
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
struct Sheet {
    items: usize,
    says: [bool; 2],
    total: i64,
    cursor: Option<i16>,
    link: Link,
}

impl IntoItems for Sheet {
    type Item = ();
    fn into_items(self) -> Vec<()> {
        vec![(); self.items]
    }
    fn count(&self) -> usize {
        visit(Visit::Count);
        self.items
    }
}

#[derive(Clone, Copy, Debug)]
enum Move {
    Offset,
    Limited,
    Total,
    Numbered,
    Cursor,
    Link,
    Url,
    Refuse,
    Reject,
}

#[derive(Clone, Copy, Debug)]
enum Rule {
    Gate(Field),
    Says(usize),
    Short(usize),
    Sized(i16),
    // The forwarded slot and the original can appear together in any order.
    Choose(Field, bool, Option<bool>),
    Switch(Field, Field),
    Edit(u8),
}

#[derive(Debug, PartialEq, Eq)]
enum Decision {
    Next(Request),
    End,
    Fault(Kind, Option<&'static str>),
}

const TWO_MOVES: &str = "the call chooses two moves: it sets two inputs that each choose one";
const DISAGREE: &str = "the page's statements disagree on whether more items follow";
const NO_NEXT: &str = "the page says that more items follow, and gives no next page";
const BAD_OFFSET: &str = "the offset of the call is not a number of items";
const OVERFLOW: &str = "the offset cannot grow any further";
const BAD_TOTAL: &str = "the total of the list could not be read";

fn mismatch(reason: &'static str) -> Decision {
    Decision::Fault(Kind::Mismatch, Some(reason))
}

fn successor(base: Move, mut call: Request, page: &Sheet) -> Decision {
    let [start, count, _] = call.inputs;
    let n = page.items as i128;
    match base {
        Move::Offset | Move::Limited => {
            let start = i128::from(start.unwrap_or(0));
            if start < 0 {
                return mismatch(BAD_OFFSET);
            }
            if n == 0 {
                return Decision::End;
            }
            let Ok(next) = i16::try_from(start + n) else {
                return if matches!(base, Move::Limited) && count.is_some_and(|k| n < i128::from(k))
                {
                    Decision::End
                } else {
                    mismatch(OVERFLOW)
                };
            };
            call.inputs[0] = Some(next);
        }
        Move::Total => {
            if n == 0 {
                return Decision::End;
            }
            // The total statement has already checked the sum.
            call.wide = Some(call.wide.unwrap_or(0) + page.items as u64);
        }
        Move::Numbered => {
            if n == 0 {
                return Decision::End;
            }
            let at = start.unwrap_or(3);
            if !(0..i16::MAX).contains(&at) {
                return mismatch("the page number cannot grow any further");
            }
            call.inputs[0] = Some(at + 1);
        }
        Move::Cursor => {
            let Some(cursor) = page.cursor else {
                return Decision::End;
            };
            call.inputs[1] = Some(cursor);
        }
        Move::Link => match page.link {
            Link::Missing | Link::Anchored => return Decision::End,
            Link::Next => call.url = Some(URL.into()),
            Link::BrokenAfterNext => return mismatch("the Link field could not be read"),
        },
        Move::Url => {
            if page.cursor.is_none() {
                return Decision::End;
            }
            call.url = Some(URL.into());
        }
        Move::Refuse => return Decision::Fault(Kind::Refused, None),
        Move::Reject => return mismatch("the page is incomplete"),
    }
    Decision::Next(call)
}

fn expected(
    base: Move,
    rules: &[Rule],
    call: &Request,
    page: &Sheet,
    visits: &mut Vec<Visit>,
    later: bool,
) -> Decision {
    let set = |field: Field| call.inputs[field as usize].is_some();
    let choices: Vec<_> = rules
        .iter()
        .filter_map(|rule| match *rule {
            Rule::Choose(field, forwarded, inner) => Some((field, forwarded, inner)),
            Rule::Switch(first, second) => Some((if later { second } else { first }, true, None)),
            _ => None,
        })
        .filter(|&(field, _, _)| set(field))
        .collect();
    if let Some((first, _, _)) = choices.first() {
        if choices.iter().any(|(field, _, _)| field != first) {
            return mismatch(TWO_MOVES);
        }
    }
    if rules
        .iter()
        .any(|rule| matches!(rule, Rule::Gate(field) if !set(*field)))
    {
        return Decision::End;
    }

    let mut statements = Vec::new();
    if matches!(base, Move::Total) {
        visits.push(Visit::Count);
        let reached = i128::from(call.wide.unwrap_or(0)) + page.items as i128;
        statements.push(if reached > i128::from(u64::MAX) {
            Err(OVERFLOW)
        } else {
            visits.push(Visit::Total);
            if page.total < 0 {
                Err(BAD_TOTAL)
            } else {
                Ok(reached < i128::from(page.total))
            }
        });
    }
    for rule in rules {
        if let Rule::Says(which) = *rule {
            visits.push(Visit::Statement(which));
            statements.push(Ok(page.says[which]));
        }
    }
    let statements = match statements.into_iter().collect::<Result<Vec<_>, _>>() {
        Ok(statements) => statements,
        Err(first) => return mismatch(first),
    };
    let more = statements.contains(&true);
    if statements.contains(&false) {
        return if more {
            mismatch(DISAGREE)
        } else {
            Decision::End
        };
    }

    let moved = match choices.first() {
        None => {
            if matches!(
                base,
                Move::Offset | Move::Limited | Move::Total | Move::Numbered
            ) {
                visits.push(Visit::Count);
            }
            successor(base, call.clone(), page)
        }
        Some(&(_, forwarded, inner)) => {
            visits.push(Visit::Move);
            let alternative = if forwarded { Move::Url } else { Move::Cursor };
            match (inner, successor(alternative, call.clone(), page)) {
                (Some(false), _) => Decision::End,
                (Some(true), Decision::End) => mismatch(NO_NEXT),
                (_, decision) => decision,
            }
        }
    };
    if !more && !matches!(moved, Decision::Fault(..)) {
        let limit = matches!(base, Move::Limited)
            .then_some(call.inputs[1])
            .flatten();
        let largest = rules
            .iter()
            .filter_map(|rule| match *rule {
                Rule::Short(size) => Some(size as i16),
                Rule::Sized(default) => Some(call.inputs[1].unwrap_or(default)),
                _ => None,
            })
            .chain(limit)
            .filter_map(|size| usize::try_from(size).ok())
            .max();
        if let Some(size) = largest {
            visits.push(Visit::Count);
            if page.items < size {
                return Decision::End;
            }
        }
    }
    match moved {
        Decision::Fault(..) => moved,
        Decision::End if more => mismatch(NO_NEXT),
        Decision::End => moved,
        Decision::Next(mut next) => {
            next.edits
                .extend(rules.iter().filter_map(|rule| match *rule {
                    Rule::Edit(digit) => {
                        visits.push(Visit::Edit(digit));
                        Some(digit)
                    }
                    _ => None,
                }));
            Decision::Next(next)
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
enum Visit {
    Count,
    Move,
    Total,
    Statement(usize),
    Edit(u8),
}

fn alternative(forwarded: bool, inner: Option<bool>) -> Paging<Request> {
    let pager = if forwarded {
        by_next_url(|sheet: &Sheet| sheet.cursor.map(|_| format!("{URL}#fragment")))
    } else {
        by_cursor(Field::Count, |sheet: &Sheet| sheet.cursor)
    };
    let mut pager = match inner {
        None => pager,
        Some(more) => Paging::from_fn(pager.has_more(move |_| more).into_fn()),
    };
    Paging::from_fn(move |call, answer: &Answer<Sheet>| {
        visit(Visit::Move);
        pager.next_call(call, answer)
    })
}

fn assemble(base: Move, rules: &[Rule]) -> Paging<Request> {
    use Field::{Count, Start};
    let mut paging = match base {
        Move::Offset => by_offset(Start),
        Move::Limited => by_offset_limit(Start, Count),
        Move::Total => by_offset_total(Wide, |sheet: &Sheet| {
            visit(Visit::Total);
            sheet.total
        }),
        Move::Numbered => by_page(Start, 3),
        Move::Cursor => by_cursor(Count, |sheet: &Sheet| sheet.cursor),
        Move::Link => by_link(),
        Move::Url => by_next_url(|sheet: &Sheet| sheet.cursor.map(|_| format!("{URL}#fragment"))),
        Move::Refuse => Paging::from_fn(
            |_: Request, _: &Answer<Sheet>| -> Result<Option<Request>, Fail<()>> {
                Err(Fail::from(()))
            },
        ),
        Move::Reject => Paging::from_fn(
            |_: Request, _: &Answer<Sheet>| -> Result<Option<Request>, Fail<()>> {
                Err(concord::mismatch("the page is incomplete"))
            },
        ),
    };
    for rule in rules {
        paging = match *rule {
            Rule::Gate(field) => paging.only_when_set(field),
            Rule::Says(which) => paging.has_more(move |sheet| {
                visit(Visit::Statement(which));
                sheet.says[which]
            }),
            Rule::Short(size) => paging.stop_on_short_page_of(size),
            Rule::Sized(default) => paging.stop_on_short_page(Count, default),
            Rule::Choose(field, true, inner) => paging.or_when_set(
                Forwarded {
                    first: field,
                    later: field,
                },
                alternative(true, inner),
            ),
            Rule::Choose(field, false, inner) => {
                paging.or_when_set(field, alternative(false, inner))
            }
            Rule::Switch(first, later) => {
                paging.or_when_set(Forwarded { first, later }, alternative(true, None))
            }
            Rule::Edit(digit) => paging.map_next(move |mut next, _| {
                visit(Visit::Edit(digit));
                next.edits.push(digit);
                next
            }),
        };
    }
    paging
}

type Decide = dyn FnMut(&Answer<Sheet>) -> Result<Option<Request>, Fail<()>> + Send;

#[derive(Clone)]
struct Job(Arc<Mutex<Box<Decide>>>);

concord::api! {
    client Store {
        base "https://store.test";
        GET scan(job: Job, start?: i16, count?: i16, gate?: i16, wide?: u64, edits?: String) -> Sheet {
            path "scan";
            query { start; count; gate; wide; edits; }
            read concord::Json;
            paginate move |call, answer| {
                let next = (job.get(&call).unwrap().0.lock().unwrap())(answer)?;
                Ok(next.map(|next| {
                    let call = call.start(next.inputs[0]).count(next.inputs[1])
                        .gate(next.inputs[2]).wide(next.wide).edits(edit_text(&next.edits));
                    match next.url {
                        Some(url) => Call::at_url(call, &url),
                        None => call,
                    }
                }))
            };
        }
    }
}

fn rule() -> impl Strategy<Value = Rule> {
    let field = || prop_oneof![Just(Field::Start), Just(Field::Count)];
    prop_oneof![
        prop_oneof![field(), Just(Field::Gate)].prop_map(Rule::Gate),
        (0..2usize).prop_map(Rule::Says),
        (0..5usize).prop_map(Rule::Short),
        (-1..5i16).prop_map(Rule::Sized),
        (field(), any::<bool>(), prop::option::of(any::<bool>()))
            .prop_map(|(f, a, b)| Rule::Choose(f, a, b)),
        (0..3u8).prop_map(Rule::Edit),
    ]
}

fn edit_text(edits: &[u8]) -> Option<String> {
    (!edits.is_empty()).then(|| {
        edits
            .iter()
            .map(|&digit| char::from(b'0' + digit))
            .collect()
    })
}

fn target(call: &Request) -> String {
    if let Some(url) = &call.url {
        return format!("https://store.test{url}");
    }
    let [start, count, gate] = call.inputs;
    let pairs: Vec<_> = [
        ("start", start.map(|n| n.to_string())),
        ("count", count.map(|n| n.to_string())),
        ("gate", gate.map(|n| n.to_string())),
        ("wide", call.wide.map(|n| n.to_string())),
        ("edits", edit_text(&call.edits)),
    ]
    .into_iter()
    .filter_map(|(key, value)| value.map(|value| format!("{key}={value}")))
    .collect();
    if pairs.is_empty() {
        "https://store.test/scan".into()
    } else {
        format!("https://store.test/scan?{}", pairs.join("&"))
    }
}

#[derive(Debug)]
struct Walk {
    base: Move,
    rules: Vec<Rule>,
    call: Request,
    sheets: Vec<Sheet>,
}

async fn check_walk(walk: Walk) -> TestCaseResult {
    let Walk {
        base,
        rules,
        call,
        mut sheets,
    } = walk;

    let decisions = sheets.len();
    // A successor from the last decision is still sent and its response is returned.
    sheets.push(Sheet {
        items: 0,
        says: [false; 2],
        total: 0,
        cursor: None,
        link: Link::Missing,
    });
    let script = Script::new();
    let mut urls = vec![target(&call)];
    let mut visits = Vec::new();
    let mut want = Decision::End;
    let mut successors = Vec::new();
    let mut current = call.clone();
    for (step, sheet) in sheets.iter().enumerate() {
        let mut reply = script.answer(200).body(serde_json::to_vec(sheet).unwrap());
        for field in sheet.link.fields() {
            reply = reply.header("link", field);
        }
        drop(reply);
        if step == decisions {
            break;
        }
        match expected(base, &rules, &current, sheet, &mut visits, step > 0) {
            Decision::Next(next) => {
                successors.push(next.clone());
                let url = target(&next);
                if urls.last() == Some(&url) {
                    want = Decision::Fault(Kind::Mismatch, None);
                    break;
                }
                urls.push(url);
                current = next;
            }
            decision => {
                want = decision;
                break;
            }
        }
    }

    VISITS.with_borrow_mut(Vec::clear);
    LATER.set(false);
    let mut pager = assemble(base, &rules);
    let mut current = call.clone();
    let mut step = 0;
    let observed = Arc::new(Mutex::new(Vec::new()));
    let recorded = observed.clone();
    let job = Job(Arc::new(Mutex::new(Box::new(move |page| {
        if step == decisions {
            return Ok(None);
        }
        LATER.set(step > 0);
        step += 1;
        let next = pager.next_call(current.clone(), page)?;
        if let Some(next) = &next {
            recorded.lock().unwrap().push(next.clone());
            current = next.clone();
        }
        Ok(next)
    }))));
    let [start, count, gate] = call.inputs;
    let mut pages = Store::new()
        .with_transport(script.clone())
        .scan(job)
        .start(start)
        .count(count)
        .gate(gate)
        .wide(call.wide)
        .edits(edit_text(&call.edits))
        .pages();

    for sheet in sheets.into_iter().take(urls.len()) {
        prop_assert_eq!(pages.try_next().await.unwrap(), Some(sheet));
    }
    match (pages.try_next().await, want) {
        (Ok(None), Decision::End) => (),
        (Err(error), Decision::Fault(kind, reason)) => {
            prop_assert_eq!(error.kind(), kind);
            prop_assert_eq!(error.reason(), reason);
        }
        (got, want) => prop_assert!(false, "got {:?}, expected {:?}", got, want),
    }
    prop_assert_eq!(pages.try_next().await.unwrap(), None);
    let sent: Vec<_> = script
        .requests()
        .iter()
        .map(|request| request.url().to_string())
        .collect();
    prop_assert_eq!(sent, urls);
    prop_assert_eq!(VISITS.with_borrow_mut(std::mem::take), visits);
    Ok(())
}

fn sheet() -> impl Strategy<Value = Sheet> {
    (
        0..5usize,
        any::<[bool; 2]>(),
        -1..10i64,
        prop::option::of(0..5i16),
        prop_oneof![
            Just(Link::Missing),
            Just(Link::Anchored),
            Just(Link::Next),
            Just(Link::BrokenAfterNext)
        ],
    )
        .prop_map(|(items, says, total, cursor, link)| Sheet {
            items,
            says,
            total,
            cursor,
            link,
        })
}

fn walks() -> impl Strategy<Value = Walk> {
    use Field::{Count, Gate, Start};
    let any_walk = (
        prop_oneof![
            Just(Move::Offset),
            Just(Move::Limited),
            Just(Move::Total),
            Just(Move::Numbered),
            Just(Move::Cursor),
            Just(Move::Link),
            Just(Move::Url),
            Just(Move::Refuse),
            Just(Move::Reject)
        ],
        prop::collection::vec(rule(), 0..10),
        prop::option::of(prop_oneof![(-1..4i16), Just(i16::MAX)]),
        prop::option::of(-1..5i16),
        prop::option::of(-1..5i16),
        prop::option::of(prop_oneof![(0..6u64), Just(u64::MAX)]),
        sheet(),
    )
        .prop_map(|(base, rules, start, count, gate, wide, sheet)| Walk {
            base,
            rules,
            call: Request {
                inputs: [start, count, gate],
                wide,
                ..Request::default()
            },
            sheets: vec![sheet],
        });

    let closed_gate = (any::<i16>(), any::<i16>(), sheet()).prop_map(|(a, b, sheet)| Walk {
        base: Move::Offset,
        rules: vec![
            Rule::Choose(Start, false, None),
            Rule::Gate(Gate),
            Rule::Choose(Count, true, None),
        ],
        call: Request {
            inputs: [Some(a), Some(b), None],
            ..Request::default()
        },
        sheets: vec![sheet],
    });

    let changing_slot = (0..4i16, sheet(), sheet()).prop_map(|(cursor, mut first, second)| {
        first.cursor = Some(cursor);
        Walk {
            base: Move::Cursor,
            rules: vec![Rule::Choose(Start, false, None), Rule::Switch(Start, Count)],
            call: Request {
                inputs: [Some(0), Some(cursor + 1), None],
                ..Request::default()
            },
            sheets: vec![first, second],
        }
    });

    let counted = (0..5usize, sheet()).prop_map(|(size, mut sheet)| {
        sheet.cursor = Some(1);
        Walk {
            base: Move::Reject,
            rules: vec![
                Rule::Choose(Start, false, None),
                Rule::Short(size),
                Rule::Edit(2),
            ],
            call: Request {
                inputs: [Some(0), None, None],
                ..Request::default()
            },
            sheets: vec![sheet],
        }
    });

    prop_oneof![12 => any_walk, 1 => closed_gate, 1 => changing_slot, 1 => counted]
}

#[test]
fn c8_6_one_page_follows_the_decision_order() {
    // Keep the decision sample size even when the workspace runs other properties with fewer cases.
    let mut runner = TestRunner::new(ProptestConfig {
        cases: 6_804,
        failure_persistence: None,
        ..ProptestConfig::default()
    });
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .start_paused(true)
        .build()
        .unwrap();
    if let Err(failure) = runner.run(&walks(), |walk| runtime.block_on(check_walk(walk))) {
        panic!("{failure}");
    }
}
