//! C8.6: a `Paging` decides in one fixed order, whatever the order in which its methods are
//! written. For generated rules, pages and inputs, every order of the methods sends the same
//! requests, gives the same pages and items, ends the same way, and calls the same functions of
//! the author's the same number of times. The one exception is the edits of `map_next`, which
//! run in the order written: an order is compared with the written order that has its edits in
//! the same order.
#![allow(clippy::unwrap_used, clippy::panic, clippy::indexing_slicing)]

mod walk;

use proptest::prelude::*;
use walk::{Base, Input, Inputs, Page, Reply, Rule, Spec, Walked};

/// Every rule, in the order in which a written description is taken to list them.
fn every_rule() -> Vec<Rule> {
    vec![
        Rule::Gate(Input::Gate),
        Rule::Gate(Input::Gate2),
        Rule::MoreA,
        Rule::MoreB,
        Rule::Short(2),
        Rule::ShortOf(3),
        Rule::Or(Input::Before),
        Rule::Or(Input::Back),
        Rule::Edit(1),
        Rule::Edit(2),
    ]
}

fn base() -> impl Strategy<Value = Base> {
    prop_oneof![
        3 => Just(Base::Cursor),
        2 => Just(Base::Offset),
        2 => Just(Base::Total),
        2 => Just(Base::Link),
        1 => Just(Base::Failing),
    ]
}

fn statement() -> impl Strategy<Value = Option<bool>> {
    prop_oneof![2 => Just(None), 3 => Just(Some(true)), 1 => Just(Some(false))]
}

/// One page, the `index`th of the walk, with items that no other page holds.
fn page(index: usize) -> impl Strategy<Value = Reply> {
    (
        0..5usize,
        statement(),
        statement(),
        proptest::bool::weighted(0.8),
        proptest::bool::weighted(0.8),
        prop_oneof![Just(None), (0..14i64).prop_map(Some)],
        prop_oneof![1 => Just(0u8), 4 => Just(1u8), 1 => Just(2u8)],
    )
        .prop_map(move |(length, a, b, next, prev, total, link)| {
            let first = index as u64 * 10;
            let page = Page {
                items: (first..first + length as u64).collect(),
                a,
                b,
                next: next.then(|| format!("n{index}")),
                prev: prev.then(|| format!("p{index}")),
                total,
            };
            match link {
                0 => page.reply(),
                1 => page.with_link(&format!(
                    r#"<https://walk.test/items?l={index}>; rel="next""#
                )),
                _ => page.with_link(r#"<https://walk.test/items?l=x>; rel="next"#),
            }
        })
}

fn replies() -> impl Strategy<Value = Vec<Reply>> {
    (page(0), page(1), page(2), page(3), page(4), 1..=5usize).prop_map(
        |(one, two, three, four, five, length)| {
            let mut replies = vec![one, two, three, four, five];
            replies.truncate(length);
            replies
        },
    )
}

fn inputs() -> impl Strategy<Value = Inputs> {
    (
        proptest::bool::weighted(0.85),
        proptest::bool::weighted(0.85),
        proptest::bool::weighted(0.3),
        proptest::bool::weighted(0.3),
        prop_oneof![Just(None), (1..4u32).prop_map(Some)],
        prop_oneof![Just(None), (0..3u64).prop_map(Some)],
    )
        .prop_map(|(gate, gate2, before, back, count, start)| Inputs {
            gate: gate.then_some(1),
            gate2: gate2.then_some(2),
            before: before.then(|| "p".to_owned()),
            back: back.then(|| "q".to_owned()),
            count,
            start,
            ..Inputs::default()
        })
}

/// A subset of the rules in their written order, and the same subset in an order of its own.
fn orders() -> impl Strategy<Value = (Vec<Rule>, Vec<Rule>)> {
    proptest::sample::subsequence(every_rule(), 0..=every_rule().len())
        .prop_flat_map(|written| (Just(written.clone()), Just(written).prop_shuffle()))
}

/// The written order, with the edits moved to the order in which `shuffled` has them.
fn written_with_edits_of(written: &[Rule], shuffled: &[Rule]) -> Vec<Rule> {
    let mut rules: Vec<Rule> = written
        .iter()
        .copied()
        .filter(|rule| !rule.is_edit())
        .collect();
    rules.extend(shuffled.iter().copied().filter(|rule| rule.is_edit()));
    rules
}

/// The URL with the digits 1 and 2 of its `tag` pair exchanged.
fn swap_tag(url: &str) -> String {
    match url.split_once("tag=") {
        None => url.to_owned(),
        Some((head, tag)) => {
            let (tag, rest) = tag
                .split_once('&')
                .map_or((tag, None), |(tag, rest)| (tag, Some(rest)));
            let tag: String = tag
                .chars()
                .map(|digit| match digit {
                    '1' => '2',
                    '2' => '1',
                    other => other,
                })
                .collect();
            match rest {
                None => format!("{head}tag={tag}"),
                Some(rest) => format!("{head}tag={tag}&{rest}"),
            }
        }
    }
}

fn run(spec: &Spec, inputs: &Inputs, replies: &[Reply]) -> Walked {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .unwrap();
    let mut walked = runtime.block_on(walk::walk(spec, inputs, replies));
    // Statements are read in the order written: only how often each function ran is compared.
    walked.events.sort();
    walked
}

proptest! {
    #![proptest_config(ProptestConfig { failure_persistence: None, ..ProptestConfig::default() })]

    #[test]
    fn c8_6_the_order_of_the_methods_changes_nothing_but_the_order_of_the_edits(
        base in base(),
        (written, shuffled) in orders(),
        inputs in inputs(),
        replies in replies(),
    ) {
        let expected = run(
            &Spec { base, rules: written_with_edits_of(&written, &shuffled) },
            &inputs,
            &replies,
        );
        let got = run(&Spec { base, rules: shuffled.clone() }, &inputs, &replies);
        prop_assert_eq!(got, expected, "written {:?}, then {:?}", written, shuffled);
    }

    #[test]
    fn c8_6_two_edits_written_the_other_way_round_change_only_the_edited_input(
        base in base(),
        (written, _) in orders(),
        inputs in inputs(),
        replies in replies(),
    ) {
        let mut one_two: Vec<Rule> = written.iter().copied().filter(|rule| !rule.is_edit()).collect();
        let mut two_one = one_two.clone();
        one_two.extend([Rule::Edit(1), Rule::Edit(2)]);
        two_one.extend([Rule::Edit(2), Rule::Edit(1)]);
        let first = run(&Spec { base, rules: one_two }, &inputs, &replies);
        let second = run(&Spec { base, rules: two_one }, &inputs, &replies);
        let swapped: Vec<String> = first.urls.iter().map(|url| swap_tag(url)).collect();
        prop_assert_eq!(&second.urls, &swapped);
        prop_assert_eq!(&second.pages, &first.pages);
        prop_assert_eq!(&second.end, &first.end);
        prop_assert_eq!(&second.events, &first.events);
    }
}
