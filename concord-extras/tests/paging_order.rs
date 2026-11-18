//! C8.6: the order in which a `Paging` decides each page, row by row, as the rustdoc of
//! `concord::page::Paging` numbers it: the gate, what the page states, the move, the short-page
//! rule when nothing is stated, the edits. For each row: the requests sent, what `.pages()` and
//! `.pages().items()` give, and how the walk ends (the kind, the line and the reason of the
//! error). Then C8.4: what the caller receives, and a dropped `Pages`.
#![allow(clippy::unwrap_used, clippy::panic, clippy::indexing_slicing)]

mod walk;

use concord::error::Kind;
use std::time::Duration;
use walk::{
    items, url, Base, End, Event, Input, Inputs, Rule, Spec, DISAGREE, MORE_WITHOUT_NEXT,
    MOVE_FAILED, TOTAL, TWO_MOVES,
};

fn spec(base: Base, rules: &[Rule]) -> Spec {
    Spec {
        base,
        rules: rules.to_vec(),
    }
}

fn none() -> Inputs {
    Inputs::default()
}

use Event::{Count, Edit, Move, StatementA as A, StatementB as B};

// 1. The gate --------------------------------------------------------------------------

#[tokio::test]
async fn c8_6_a_gate_whose_input_is_not_set_ends_the_list_and_nothing_else_is_read() {
    let walked = walk::walk(
        &spec(
            Base::Cursor,
            &[Rule::Gate(Input::Gate), Rule::MoreA, Rule::ShortOf(5)],
        ),
        &none(),
        &[
            items(&[1, 2]).a(true).next("n1").reply(),
            items(&[3]).reply(),
        ],
    )
    .await;
    assert_eq!(walked.urls, [url("")]);
    assert_eq!(walked.pages, [vec![1, 2]]);
    assert_eq!(walked.end, End::Done);
    assert_eq!(
        walked.events,
        [],
        "a closed gate read a statement, a move or a count"
    );
}

#[tokio::test]
async fn c8_6_a_gate_whose_input_is_set_lets_the_rest_decide() {
    let walked = walk::walk(
        &spec(Base::Cursor, &[Rule::Gate(Input::Gate), Rule::MoreA]),
        &Inputs {
            gate: Some(1),
            ..none()
        },
        &[
            items(&[1, 2]).a(true).next("n1").reply(),
            items(&[3]).a(false).next("n2").reply(),
        ],
    )
    .await;
    assert_eq!(walked.urls, [url("gate=1"), url("after=n1&gate=1")]);
    assert_eq!(walked.pages, [vec![1, 2], vec![3]]);
    assert_eq!(walked.end, End::Done);
    assert_eq!(walked.events, [A, Move("after"), A]);
}

#[tokio::test]
async fn c8_6_one_gate_of_two_that_is_not_set_ends_the_list() {
    let walked = walk::walk(
        &spec(
            Base::Cursor,
            &[
                Rule::Gate(Input::Gate),
                Rule::Gate(Input::Gate2),
                Rule::MoreA,
            ],
        ),
        &Inputs {
            gate: Some(1),
            ..none()
        },
        &[items(&[1]).a(true).next("n1").reply(), items(&[2]).reply()],
    )
    .await;
    assert_eq!(walked.urls, [url("gate=1")]);
    assert_eq!(walked.end, End::Done);
    assert_eq!(walked.events, []);
}

#[tokio::test]
async fn c8_6_a_gate_ends_the_list_before_a_move_that_would_fail() {
    let walked = walk::walk(
        &spec(Base::Failing, &[Rule::Gate(Input::Gate)]),
        &none(),
        &[items(&[1]).reply()],
    )
    .await;
    assert_eq!(walked.pages, [vec![1]]);
    assert_eq!(walked.end, End::Done);
    assert_eq!(walked.events, []);
}

// 2. What the page states ----------------------------------------------------------------

#[tokio::test]
async fn c8_6_a_statement_alone_says_more_or_ends_the_list_though_a_cursor_is_there() {
    let walked = walk::walk(
        &spec(Base::Cursor, &[Rule::MoreA]),
        &none(),
        &[
            items(&[1]).a(true).next("n1").reply(),
            items(&[2]).a(false).next("n2").reply(),
            items(&[3]).reply(),
        ],
    )
    .await;
    assert_eq!(walked.urls, [url(""), url("after=n1")]);
    assert_eq!(walked.pages, [vec![1], vec![2]]);
    assert_eq!(walked.end, End::Done);
    assert_eq!(
        walked.events,
        [A, Move("after"), A],
        "the move ran after a page said no more"
    );
}

#[tokio::test]
async fn c8_6_a_total_alone_moves_below_it_and_ends_when_reached() {
    let walked = walk::walk(
        &spec(Base::Total, &[]),
        &none(),
        &[
            items(&[1, 2]).total(5).reply(),
            items(&[3, 4]).total(5).reply(),
            items(&[5]).total(5).reply(),
            items(&[6]).total(5).reply(),
        ],
    )
    .await;
    assert_eq!(walked.urls, [url(""), url("start=2"), url("start=4")]);
    assert_eq!(walked.pages, [vec![1, 2], vec![3, 4], vec![5]]);
    assert_eq!(walked.end, End::Done);
    let totals = walked.events.iter().filter(|e| **e == Event::Total).count();
    assert_eq!(totals, 3, "the total was not read once for each page");
}

#[tokio::test]
async fn c8_6_two_statements_that_say_more_go_on_and_two_that_say_no_more_end() {
    let walked = walk::walk(
        &spec(Base::Cursor, &[Rule::MoreA, Rule::MoreB]),
        &none(),
        &[
            items(&[1]).a(true).b(true).next("n1").reply(),
            items(&[2]).a(false).b(false).next("n2").reply(),
            items(&[3]).reply(),
        ],
    )
    .await;
    assert_eq!(walked.urls, [url(""), url("after=n1")]);
    assert_eq!(walked.pages, [vec![1], vec![2]]);
    assert_eq!(walked.end, End::Done);
    assert_eq!(walked.events, [A, B, Move("after"), A, B]);
}

#[tokio::test]
async fn c8_6_the_statements_are_read_in_the_order_written() {
    let walked = walk::walk(
        &spec(Base::Cursor, &[Rule::MoreB, Rule::MoreA]),
        &none(),
        &[
            items(&[1]).a(true).b(true).next("n1").reply(),
            items(&[2]).reply(),
        ],
    )
    .await;
    assert_eq!(walked.events, [B, A, Move("after"), B, A]);
}

#[tokio::test]
async fn c8_6_two_statements_that_disagree_end_the_list_with_a_mismatch_before_the_move() {
    for (a, b) in [(true, false), (false, true)] {
        let walked = walk::walk(
            &spec(Base::Cursor, &[Rule::MoreA, Rule::MoreB]),
            &none(),
            &[
                items(&[1]).a(a).b(b).next("n1").reply(),
                items(&[2]).reply(),
            ],
        )
        .await;
        assert_eq!(walked.urls, [url("")]);
        assert_eq!(walked.pages, [vec![1]]);
        assert_eq!(walked.end, End::mismatch(DISAGREE));
        assert_eq!(walked.events, [A, B], "a: {a}, b: {b}");
    }
}

#[tokio::test]
async fn c8_6_a_total_reached_and_a_statement_that_says_more_disagree() {
    let walked = walk::walk(
        &spec(Base::Total, &[Rule::MoreA]),
        &none(),
        &[items(&[1, 2]).total(2).a(true).reply(), items(&[3]).reply()],
    )
    .await;
    assert_eq!(walked.urls, [url("")]);
    assert_eq!(walked.end, End::mismatch(DISAGREE));
}

#[tokio::test]
async fn c8_6_a_statement_that_cannot_be_read_is_the_result_and_every_statement_is_read() {
    let walked = walk::walk(
        &spec(Base::Total, &[Rule::MoreA]),
        &none(),
        &[items(&[1]).a(true).reply(), items(&[2]).reply()],
    )
    .await;
    assert_eq!(walked.urls, [url("")]);
    assert_eq!(walked.pages, [vec![1]]);
    assert_eq!(walked.end, End::mismatch(TOTAL));
    assert_eq!(
        walked.events.iter().filter(|e| **e == Event::Total).count(),
        1
    );
    assert_eq!(
        walked.events.iter().filter(|e| **e == A).count(),
        1,
        "the other statement was not read"
    );
}

#[tokio::test]
async fn c8_6_a_statement_that_cannot_be_read_wins_over_two_that_disagree() {
    let walked = walk::walk(
        &spec(Base::Total, &[Rule::MoreA, Rule::MoreB]),
        &none(),
        &[items(&[1]).total(-3).a(true).b(false).reply()],
    )
    .await;
    assert_eq!(walked.end, End::mismatch(TOTAL));
}

#[tokio::test]
async fn c8_6_more_with_a_next_call_moves_and_more_without_one_is_a_mismatch() {
    let walked = walk::walk(
        &spec(Base::Cursor, &[Rule::MoreA]),
        &none(),
        &[
            items(&[1]).a(true).next("n1").reply(),
            items(&[2]).a(true).reply(),
            items(&[3]).reply(),
        ],
    )
    .await;
    assert_eq!(walked.urls, [url(""), url("after=n1")]);
    assert_eq!(walked.pages, [vec![1], vec![2]]);
    assert_eq!(walked.end, End::mismatch(MORE_WITHOUT_NEXT));
    assert_eq!(walked.events, [A, Move("after"), A, Move("after")]);
}

// 3 and 4. The move, and the short-page rule ------------------------------------------------

#[tokio::test]
async fn c8_6_with_no_statement_the_move_decides_and_no_next_call_is_the_end() {
    let walked = walk::walk(
        &spec(Base::Cursor, &[]),
        &none(),
        &[items(&[]).next("n1").reply(), items(&[2]).reply()],
    )
    .await;
    assert_eq!(walked.urls, [url(""), url("after=n1")]);
    assert_eq!(walked.pages, [vec![], vec![2]]);
    assert_eq!(walked.end, End::Done);
}

#[tokio::test]
async fn c8_6_a_fixed_short_page_rule_ends_on_a_short_page_counted_after_the_move() {
    let walked = walk::walk(
        &spec(Base::Cursor, &[Rule::ShortOf(3)]),
        &none(),
        &[
            items(&[1, 2, 3]).next("n1").reply(),
            items(&[4, 5]).next("n2").reply(),
            items(&[6]).reply(),
        ],
    )
    .await;
    assert_eq!(walked.urls, [url(""), url("after=n1")]);
    assert_eq!(walked.pages, [vec![1, 2, 3], vec![4, 5]]);
    assert_eq!(walked.end, End::Done);
    assert_eq!(walked.events, [Move("after"), Count, Move("after"), Count]);
}

#[tokio::test]
async fn c8_6_a_short_page_rule_takes_the_count_the_caller_set_or_its_default() {
    let replies = [
        items(&[1, 2]).next("n1").reply(),
        items(&[3, 4, 5]).next("n2").reply(),
        items(&[6]).next("n3").reply(),
        items(&[7]).reply(),
    ];
    let by_default = walk::walk(&spec(Base::Cursor, &[Rule::Short(3)]), &none(), &replies).await;
    assert_eq!(by_default.urls, [url("")]);
    assert_eq!(by_default.end, End::Done);

    let by_count = walk::walk(
        &spec(Base::Cursor, &[Rule::Short(3)]),
        &Inputs {
            count: Some(2),
            ..none()
        },
        &replies,
    )
    .await;
    assert_eq!(
        by_count.urls,
        [
            url("count=2"),
            url("after=n1&count=2"),
            url("after=n2&count=2")
        ]
    );
    assert_eq!(by_count.pages, [vec![1, 2], vec![3, 4, 5], vec![6]]);
    assert_eq!(by_count.end, End::Done);
}

#[tokio::test]
async fn c8_6_a_statement_has_authority_over_the_short_page_rule_which_is_not_read() {
    let walked = walk::walk(
        &spec(Base::Cursor, &[Rule::ShortOf(3), Rule::MoreA]),
        &none(),
        &[
            items(&[1]).a(true).next("n1").reply(),
            items(&[2, 3, 4]).a(false).next("n2").reply(),
            items(&[5]).reply(),
        ],
    )
    .await;
    assert_eq!(walked.urls, [url(""), url("after=n1")]);
    assert_eq!(walked.pages, [vec![1], vec![2, 3, 4]]);
    assert_eq!(walked.end, End::Done);
    assert_eq!(
        walked.events,
        [A, Move("after"), A],
        "the short-page rule was read"
    );
}

#[tokio::test]
async fn c8_6_on_a_short_page_the_failure_of_the_move_is_the_result() {
    let walked = walk::walk(
        &spec(Base::Failing, &[Rule::ShortOf(3)]),
        &none(),
        &[items(&[1]).reply(), items(&[2]).reply()],
    )
    .await;
    assert_eq!(walked.urls, [url("")]);
    assert_eq!(walked.pages, [vec![1]]);
    assert_eq!(walked.end, End::mismatch(MOVE_FAILED));
}

#[tokio::test]
async fn c8_6_on_a_short_page_an_unreadable_link_is_the_result() {
    let walked = walk::walk(
        &spec(Base::Link, &[Rule::ShortOf(3)]),
        &none(),
        &[
            items(&[1]).with_link(r#"<https://walk.test/items?p=2>; rel="next"#),
            items(&[2]).reply(),
        ],
    )
    .await;
    assert_eq!(walked.urls, [url("")]);
    assert_eq!(walked.pages, [vec![1]]);
    match walked.end {
        End::Failed { kind, line, .. } => {
            assert_eq!(kind, Kind::Mismatch);
            assert_eq!(line, walk::CANNOT_BE_REQUESTED);
        }
        End::Done => panic!("an unreadable `Link` on a short page ended the list in silence"),
    }
}

#[tokio::test]
async fn c8_6_an_input_chooses_the_move_and_with_it_unset_the_default_moves() {
    let rules = [Rule::Or(Input::Before)];
    let replies = [
        items(&[1]).next("n1").prev("p1").reply(),
        items(&[2]).reply(),
    ];
    let forward = walk::walk(&spec(Base::Cursor, &rules), &none(), &replies).await;
    assert_eq!(forward.urls, [url(""), url("after=n1")]);
    assert_eq!(forward.events, [Move("after"), Move("after")]);

    let backward = walk::walk(
        &spec(Base::Cursor, &rules),
        &Inputs {
            before: Some("p0".into()),
            ..none()
        },
        &replies,
    )
    .await;
    assert_eq!(backward.urls, [url("before=p0"), url("before=p1")]);
    assert_eq!(backward.pages, [vec![1], vec![2]]);
    assert_eq!(backward.end, End::Done);
    assert_eq!(backward.events, [Move("before"), Move("before")]);
}

#[tokio::test]
async fn c8_6_the_default_input_set_beside_one_that_chooses_is_no_error() {
    let walked = walk::walk(
        &spec(Base::Cursor, &[Rule::Or(Input::Before)]),
        &Inputs {
            after: Some("n0".into()),
            before: Some("p0".into()),
            ..none()
        },
        &[
            items(&[1]).next("n1").prev("p1").reply(),
            items(&[2]).reply(),
        ],
    )
    .await;
    assert_eq!(
        walked.urls,
        [url("after=n0&before=p0"), url("after=n0&before=p1")]
    );
    assert_eq!(walked.end, End::Done);
}

#[tokio::test]
async fn c8_6_two_inputs_that_each_choose_the_move_end_the_list_at_the_first_decision() {
    let walked = walk::walk(
        &spec(
            Base::Cursor,
            &[Rule::Or(Input::Before), Rule::Or(Input::Back)],
        ),
        &Inputs {
            before: Some("p0".into()),
            back: Some("q0".into()),
            ..none()
        },
        &[
            items(&[1]).next("n1").prev("p1").reply(),
            items(&[2]).reply(),
        ],
    )
    .await;
    assert_eq!(walked.urls, [url("before=p0&back=q0")]);
    assert_eq!(walked.pages, [vec![1]]);
    assert_eq!(walked.end, End::mismatch(TWO_MOVES));
}

#[tokio::test]
async fn c8_6_two_inputs_that_each_choose_are_not_read_when_the_page_says_no_more() {
    let walked = walk::walk(
        &spec(
            Base::Cursor,
            &[Rule::Or(Input::Before), Rule::Or(Input::Back), Rule::MoreA],
        ),
        &Inputs {
            before: Some("p0".into()),
            back: Some("q0".into()),
            ..none()
        },
        &[items(&[1]).a(false).next("n1").prev("p1").reply()],
    )
    .await;
    assert_eq!(walked.urls, [url("before=p0&back=q0")]);
    assert_eq!(walked.end, End::Done);
    assert_eq!(walked.events, [A]);
}

#[tokio::test]
async fn c8_6_the_rules_of_the_value_apply_to_a_move_chosen_by_an_input() {
    let walked = walk::walk(
        &spec(
            Base::Cursor,
            &[Rule::Or(Input::Before), Rule::MoreA, Rule::Edit(1)],
        ),
        &Inputs {
            before: Some("p0".into()),
            ..none()
        },
        &[
            items(&[1]).a(true).prev("p1").reply(),
            items(&[2]).a(true).reply(),
        ],
    )
    .await;
    assert_eq!(walked.urls, [url("before=p0"), url("before=p1&tag=1")]);
    assert_eq!(walked.end, End::mismatch(MORE_WITHOUT_NEXT));
}

// 5. The edits ----------------------------------------------------------------------------

#[tokio::test]
async fn c8_6_the_edits_run_on_each_next_call_in_the_order_written() {
    let replies = [
        items(&[1]).next("n1").reply(),
        items(&[2]).next("n2").reply(),
        items(&[3]).reply(),
    ];
    let one_two = walk::walk(
        &spec(Base::Cursor, &[Rule::Edit(1), Rule::Edit(2)]),
        &none(),
        &replies,
    )
    .await;
    assert_eq!(
        one_two.urls,
        [url(""), url("after=n1&tag=12"), url("after=n2&tag=1212")]
    );
    assert_eq!(
        one_two.events,
        [
            Move("after"),
            Edit(1),
            Edit(2),
            Move("after"),
            Edit(1),
            Edit(2),
            Move("after")
        ]
    );

    let two_one = walk::walk(
        &spec(Base::Cursor, &[Rule::Edit(2), Rule::Edit(1)]),
        &none(),
        &replies,
    )
    .await;
    assert_eq!(
        two_one.urls,
        [url(""), url("after=n1&tag=21"), url("after=n2&tag=2121")]
    );
}

#[tokio::test]
async fn c8_6_an_edit_runs_after_every_decision_and_only_when_there_is_a_next_call() {
    let walked = walk::walk(
        &spec(
            Base::Cursor,
            &[Rule::Edit(1), Rule::MoreA, Rule::ShortOf(1)],
        ),
        &none(),
        &[
            items(&[1]).a(true).next("n1").reply(),
            items(&[2]).a(false).next("n2").reply(),
        ],
    )
    .await;
    assert_eq!(walked.urls, [url(""), url("after=n1&tag=1")]);
    assert_eq!(walked.events, [A, Move("after"), Edit(1), A]);
}

// C8.4: what the caller receives ------------------------------------------------------------

#[tokio::test]
async fn c8_4_a_page_comes_first_and_the_decision_about_it_with_the_next_pull() {
    walk::set(&spec(Base::Cursor, &[Rule::MoreA, Rule::MoreB]));
    let script = concord_testkit::Script::new();
    script
        .answer(200)
        .header("content-type", "application/json")
        .body(r#"{"items":[1,2],"a":true,"b":false,"next":"n1"}"#);
    let walk = walk::Walk::new().with_transport(script.clone());
    let mut pages = walk.list().pages();
    assert_eq!(
        script.requests().len(),
        0,
        "a request was sent before the first pull"
    );
    let first = pages.try_next().await.unwrap().unwrap();
    assert_eq!(first.items, [1, 2]);
    let error = pages.try_next().await.unwrap_err();
    assert_eq!(error.kind(), Kind::Mismatch);
    assert_eq!(error.reason(), Some(DISAGREE));
    for _ in 0..3 {
        assert!(matches!(pages.try_next().await, Ok(None)));
    }
    assert_eq!(script.requests().len(), 1);
}

#[tokio::test]
async fn c8_4_the_items_of_a_page_come_before_the_error_about_the_next() {
    walk::set(&spec(Base::Cursor, &[Rule::MoreA]));
    let script = concord_testkit::Script::new();
    script
        .answer(200)
        .header("content-type", "application/json")
        .body(r#"{"items":[1,2],"a":true}"#);
    let walk = walk::Walk::new().with_transport(script.clone());
    let mut items = walk.list().pages().items();
    assert_eq!(items.try_next().await.unwrap(), Some(1));
    assert_eq!(items.try_next().await.unwrap(), Some(2));
    let error = items.try_next().await.unwrap_err();
    assert_eq!(error.reason(), Some(MORE_WITHOUT_NEXT));
    assert_eq!(items.try_next().await.unwrap(), None);
    assert_eq!(items.try_next().await.unwrap(), None);
    assert_eq!(script.requests().len(), 1);
}

#[tokio::test(start_paused = true)]
async fn c8_4_a_dropped_pages_sends_nothing_more() {
    walk::set(&spec(Base::Cursor, &[Rule::MoreA]));
    let script = concord_testkit::Script::new();
    script
        .answer(200)
        .header("content-type", "application/json")
        .body(r#"{"items":[1],"a":true,"next":"n1"}"#);
    script
        .answer(200)
        .after(Duration::from_secs(5))
        .header("content-type", "application/json")
        .body(r#"{"items":[2],"a":true,"next":"n2"}"#);
    script
        .answer(200)
        .header("content-type", "application/json")
        .body(r#"{"items":[3]}"#);
    let walk = walk::Walk::new().with_transport(script.clone());
    let mut pages = walk.list().pages();
    assert_eq!(pages.try_next().await.unwrap().unwrap().items, [1]);
    // The second page is on its way when the caller stops waiting, then drops the pages.
    let waited = tokio::time::timeout(Duration::from_secs(1), pages.try_next()).await;
    assert!(waited.is_err(), "the second page arrived before its time");
    drop(pages);
    tokio::time::advance(Duration::from_secs(60)).await;
    assert_eq!(script.requests().len(), 2);
    assert_eq!(script.unused(), 1);

    // And a `Pages` dropped after its first page sends nothing more either.
    let script = concord_testkit::Script::new();
    script
        .answer(200)
        .header("content-type", "application/json")
        .body(r#"{"items":[1],"a":true,"next":"n1"}"#);
    let walk = walk::Walk::new().with_transport(script.clone());
    let mut pages = walk.list().pages();
    pages.try_next().await.unwrap();
    drop(pages);
    tokio::time::advance(Duration::from_secs(60)).await;
    assert_eq!(script.requests().len(), 1);
}
