//! `HeaderPlan` (the run-time composition of chains with native header names) against an
//! independent sequential reference: every chain of up to four contributions over three names,
//! every operation and presence. The reference applies the declaration rules step by step, the
//! way the macro composes literal chains; the plan uses a closed form over name equality.

use concord_core::__private::{EndpointMeta, HeaderEntry, HeaderOp, HeaderPlan};
use concord_core::reqwest::header::HeaderName;

static META: EndpointMeta = EndpointMeta {
    name: "Plan.endpoint",
    method: "GET",
    labels: 0,
    sensitive_labels: &[],
    segments: 0,
    sensitive_segments: &[],
    sensitive_query: &[],
    sensitive_content_type: false,
};

const OPS: [HeaderOp; 5] = [
    HeaderOp::Set,
    HeaderOp::Append,
    HeaderOp::Override,
    HeaderOp::OverrideElseInherit,
    HeaderOp::Remove,
];

/// The sequential rule: per header, a list of values, set or not. Returns, per contribution,
/// whether its value is sent, or `None` when the declaration is an error.
fn reference(names: &[usize], ops: &[HeaderOp], present: &[bool]) -> Option<Vec<bool>> {
    // Per name: the contributions currently holding values; whether the name is set.
    let mut values: Vec<Vec<usize>> = vec![Vec::new(); 3];
    let mut set = [false; 3];
    for (i, (&n, &op)) in names.iter().zip(ops).enumerate() {
        match op {
            HeaderOp::Set => {
                if set[n] {
                    return None;
                }
                values[n] = vec![i];
                set[n] = true;
            }
            HeaderOp::Append => {
                values[n].push(i);
                set[n] = true;
            }
            HeaderOp::Override | HeaderOp::OverrideElseInherit => {
                if !set[n] {
                    return None;
                }
                if op == HeaderOp::Override || present[i] {
                    values[n] = vec![i];
                }
            }
            HeaderOp::Remove => {
                if !set[n] {
                    return None;
                }
                values[n].clear();
                set[n] = false;
            }
        }
    }
    let mut live = vec![false; names.len()];
    for list in values {
        for i in list {
            live[i] = true;
        }
    }
    Some(live)
}

fn plan<const N: usize>(
    names: &[HeaderName; 3],
    chain: &[usize],
    ops: &[HeaderOp],
    present: &[bool],
) -> Option<Vec<bool>> {
    let refs: [&HeaderName; N] = std::array::from_fn(|i| &names[chain[i]]);
    let entries: [HeaderEntry; N] = std::array::from_fn(|i| HeaderEntry {
        op: ops[i],
        present: present[i],
        marked: false,
        label: "entry",
    });
    // What is sent: a live contribution sends its value when it has one (an absent
    // `override ... else inherit` source sends nothing; the generated code does not call it).
    HeaderPlan::<N>::compose(refs, entries, false, &META)
        .ok()
        .map(|p| {
            (0..N)
                .map(|i| p.live[i] && (ops[i] != HeaderOp::OverrideElseInherit || present[i]))
                .collect()
        })
}

fn check<const N: usize>(names: &[HeaderName; 3]) -> usize {
    let mut cases = 0;
    let total = 3usize.pow(N as u32) * 5usize.pow(N as u32) * 2usize.pow(N as u32);
    for code in 0..total {
        let mut c = code;
        let mut chain = [0usize; N];
        let mut ops = [HeaderOp::Set; N];
        let mut present = [true; N];
        for i in 0..N {
            chain[i] = c % 3;
            c /= 3;
            ops[i] = OPS[c % 5];
            c /= 5;
            present[i] = c % 2 == 0;
            c /= 2;
        }
        let expected = reference(&chain, &ops, &present);
        let got = plan::<N>(names, &chain, &ops, &present);
        assert_eq!(
            got,
            expected,
            "chain {chain:?} ops {:?} present {present:?}",
            ops.map(|o| o as u8)
        );
        cases += 1;
    }
    cases
}

#[test]
fn plan_equals_the_sequential_rule() {
    let names = [
        HeaderName::from_static("authorization"),
        HeaderName::from_static("x-riot-token"),
        HeaderName::from_static("accept"),
    ];
    let cases = check::<1>(&names) + check::<2>(&names) + check::<3>(&names) + check::<4>(&names);
    assert!(cases > 100_000, "{cases}");
}

#[test]
fn sensitivity_is_sticky_per_header_and_by_name() {
    let auth = HeaderName::from_static("authorization");
    let other = HeaderName::from_static("x-other");
    let entry = |op, marked| HeaderEntry {
        op,
        present: true,
        marked,
        label: "e",
    };
    // A marked contribution makes every contribution to the same header sensitive, before or after.
    let p = HeaderPlan::<3>::compose(
        [&other, &other, &auth],
        [
            entry(HeaderOp::Set, false),
            entry(HeaderOp::Append, true),
            entry(HeaderOp::Set, false),
        ],
        false,
        &META,
    )
    .unwrap();
    assert_eq!(p.sensitive, [true, true, true]);
    let p = HeaderPlan::<2>::compose(
        [&other, &auth],
        [entry(HeaderOp::Set, false), entry(HeaderOp::Set, false)],
        false,
        &META,
    )
    .unwrap();
    assert_eq!(p.sensitive, [false, true]);
}
