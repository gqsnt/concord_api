//! C8.4 and C8.6: the decision for one page, including its next call and any deferred error.

// PANIC-SAFETY: Generated indices and test helpers belong to a test executable; failures report a case.
#![allow(clippy::unwrap_used, clippy::indexing_slicing)]

use concord::error::Kind;
use concord::page::{by_offset_total, Call, InputId, IntoItems, Pager, Paging, Slot};
use concord::{by_cursor, by_link, by_next_url, by_offset, by_offset_limit, by_page, Answer, Fail};
use concord_testkit::Script;
use proptest::{prelude::*, test_runner::TestRunner};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Request {
    inputs: [Option<i16>; 2],
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
struct Forwarded(Field);

impl Slot<Request> for Forwarded {
    type Value = i16;
    fn get<'a>(&self, call: &'a Request) -> Option<&'a i16> {
        self.0.get(call)
    }
    fn set(&self, call: Request, value: i16) -> Request {
        self.0.set(call, value)
    }
    fn input(&self) -> InputId {
        self.0.input()
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
        InputId::new(2)
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
    let [start, count] = call.inputs;
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
) -> Decision {
    let set = |field: Field| call.inputs[field as usize].is_some();
    let choices: Vec<_> = rules
        .iter()
        .filter_map(|rule| match *rule {
            Rule::Choose(field, forwarded, inner) if set(field) => Some((field, forwarded, inner)),
            _ => None,
        })
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
        None => successor(base, call.clone(), page),
        Some(&(_, forwarded, inner)) => {
            let alternative = if forwarded { Move::Url } else { Move::Cursor };
            match (inner, successor(alternative, call.clone(), page)) {
                (Some(false), _) => Decision::End,
                (Some(true), Decision::End) => mismatch(NO_NEXT),
                (_, decision) => decision,
            }
        }
    };
    match moved {
        Decision::Fault(..) => moved,
        Decision::End if more => mismatch(NO_NEXT),
        Decision::End => moved,
        Decision::Next(mut next) => {
            let bare_limit = matches!(base, Move::Limited)
                .then_some(call.inputs[1])
                .flatten();
            let sizes = rules
                .iter()
                .filter_map(|rule| match *rule {
                    Rule::Short(size) => Some(size as i16),
                    Rule::Sized(default) => Some(call.inputs[1].unwrap_or(default)),
                    _ => None,
                })
                .chain(bare_limit);
            if !more
                && sizes
                    .filter_map(|n| usize::try_from(n).ok())
                    .any(|n| page.items < n)
            {
                return Decision::End;
            }
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
    Total,
    Statement(usize),
    Edit(u8),
}

type Visits = Arc<Mutex<Vec<Visit>>>;

fn alternative(forwarded: bool, inner: Option<bool>) -> Paging<Request> {
    let pager = if forwarded {
        by_next_url(|sheet: &Sheet| sheet.cursor.map(|_| format!("{URL}#fragment")))
    } else {
        by_cursor(Field::Count, |sheet: &Sheet| sheet.cursor)
    };
    match inner {
        None => pager,
        Some(more) => Paging::from_fn(pager.has_more(move |_| more).into_fn()),
    }
}

fn assemble(base: Move, rules: &[Rule], visits: &Visits) -> Paging<Request> {
    use Field::{Count, Start};
    let mut paging = match base {
        Move::Offset => by_offset(Start),
        Move::Limited => by_offset_limit(Start, Count),
        Move::Total => {
            let seen = visits.clone();
            by_offset_total(Wide, move |sheet: &Sheet| {
                seen.lock().unwrap().push(Visit::Total);
                sheet.total
            })
        }
        Move::Numbered => by_page(Start, 3),
        Move::Cursor => by_cursor(Count, |sheet: &Sheet| sheet.cursor),
        Move::Link => by_link(),
        Move::Url => alternative(true, None),
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
            Rule::Says(which) => {
                let seen = visits.clone();
                paging.has_more(move |sheet| {
                    seen.lock().unwrap().push(Visit::Statement(which));
                    sheet.says[which]
                })
            }
            Rule::Short(size) => paging.stop_on_short_page_of(size),
            Rule::Sized(default) => paging.stop_on_short_page(Count, default),
            Rule::Choose(field, true, inner) => {
                paging.or_when_set(Forwarded(field), alternative(true, inner))
            }
            Rule::Choose(field, false, inner) => {
                paging.or_when_set(field, alternative(false, inner))
            }
            Rule::Edit(digit) => {
                let seen = visits.clone();
                paging.map_next(move |mut next, _| {
                    seen.lock().unwrap().push(Visit::Edit(digit));
                    next.edits.push(digit);
                    next
                })
            }
        };
    }
    paging
}

type Decide = dyn FnMut(&Answer<Sheet>) -> Result<(), Fail<()>> + Send;

#[derive(Clone)]
struct Job(Arc<Mutex<Box<Decide>>>);

concord::api! {
    client Store {
        base "https://store.test";
        GET scan(job: Job) -> Sheet {
            read concord::Json;
            paginate move |call, answer| {
                // Use Pages to turn the opaque failure into the public client error.
                (job.get(&call).unwrap().0.lock().unwrap())(answer)?;
                Ok(None)
            };
        }
    }
}

fn rule() -> impl Strategy<Value = Rule> {
    let field = || prop_oneof![Just(Field::Start), Just(Field::Count)];
    prop_oneof![
        field().prop_map(Rule::Gate),
        (0..2usize).prop_map(Rule::Says),
        (0..5usize).prop_map(Rule::Short),
        (-1..5i16).prop_map(Rule::Sized),
        (field(), any::<bool>(), prop::option::of(any::<bool>()))
            .prop_map(|(f, a, b)| Rule::Choose(f, a, b)),
        (0..3u8).prop_map(Rule::Edit),
    ]
}

#[test]
fn c8_6_one_page_follows_the_decision_order() {
    // Keep the decision sample size even when the workspace runs other properties with fewer cases.
    let mut runner = TestRunner::new(ProptestConfig {
        cases: 6_804,
        failure_persistence: None,
        ..ProptestConfig::default()
    });
    let cases = (
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
        prop::option::of(prop_oneof![(0..6u64), Just(u64::MAX)]),
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
    );
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .start_paused(true)
        .build()
        .unwrap();
    let result = runner.run(
        &cases,
        |(base, rules, start, count, wide, items, says, total, cursor, link)| {
            let call = Request {
                inputs: [start, count],
                wide,
                ..Request::default()
            };
            let sheet = Sheet {
                items,
                says,
                total,
                cursor,
                link,
            };
            let mut expected_visits = Vec::new();
            let wanted = expected(base, &rules, &call, &sheet, &mut expected_visits);

            let script = Script::new();
            let mut reply = script.answer(200).body(serde_json::to_vec(&sheet).unwrap());
            for field in link.fields() {
                reply = reply.header("link", field);
            }
            drop(reply);

            let visits = Visits::default();
            let mut pager = assemble(base, &rules, &visits);
            let next = Arc::new(Mutex::new(None));
            let result = next.clone();
            let job = Job(Arc::new(Mutex::new(Box::new(move |page| {
                *result.lock().unwrap() = pager.next_call(call.clone(), page)?;
                Ok(())
            }))));
            let mut pages = Store::new()
                .with_transport(script.clone())
                .scan(job)
                .pages();
            runtime.block_on(async {
                prop_assert_eq!(pages.try_next().await.unwrap(), Some(sheet));
                let actual = match pages.try_next().await {
                    Ok(page) => {
                        prop_assert_eq!(page, None);
                        next.lock()
                            .unwrap()
                            .take()
                            .map_or(Decision::End, Decision::Next)
                    }
                    Err(error) => {
                        let reason = match &wanted {
                            Decision::Fault(_, expected) => *expected,
                            _ => None,
                        };
                        prop_assert_eq!(error.reason(), reason, "{:?}", error);
                        Decision::Fault(error.kind(), reason)
                    }
                };
                prop_assert_eq!(actual, wanted);
                prop_assert_eq!(&*visits.lock().unwrap(), &expected_visits);
                prop_assert_eq!(pages.try_next().await.unwrap(), None);
                prop_assert_eq!(script.requests().len(), 1);
                Ok(())
            })
        },
    );
    if let Err(failure) = result {
        panic!("{failure}");
    }
}
