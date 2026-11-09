//! `HeaderPlan` (the run-time composition of chains with native header names) against an
//! independent sequential reference: every chain of up to four contributions over three names,
//! every operation and presence. The reference applies the declaration rules step by step, the
//! way the macro composes literal chains; the plan uses a closed form over name equality.
//!
//! H13: the client's `validate_header_names()` (`NameCheck`) takes no presence at all; its verdict
//! must be the one `prepare()` (`HeaderPlan::compose`) reaches under **every** presence
//! assignment, with the same declaration reported, including the `content-type` rule against a
//! declared body media type.

use concord_core::__private::{EndpointMeta, HeaderOp, HeaderPlan, HeaderShape, NameCheck};
use concord_core::reqwest::header::HeaderName;

static META: EndpointMeta = EndpointMeta {
    name: "Plan.endpoint",
    method: "GET",
    route: concord_core::__private::route(""),
    query_pairs: false,
    labels: 0,
    sensitive_labels: &[],
    segments: 0,
    sensitive_segments: &[],
    sensitive_query: &[],
    sensitive_content_type: false,
    sensitive_accept: false,
    accept_removed: false,
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
    reference_with_media(names, ops, present, None)
}

/// The sequential rule, plus the media-type rule: with a declared body media type, an error when
/// any value of `content_type` (a name index) would be sent under this presence assignment.
fn reference_with_media(
    names: &[usize],
    ops: &[HeaderOp],
    present: &[bool],
    content_type: Option<usize>,
) -> Option<Vec<bool>> {
    // Per name: the contributions currently holding values; whether the name is set.
    let mut values: Vec<Vec<usize>> = vec![Vec::new(); 4];
    let mut set = [false; 4];
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
    if let Some(ct) = content_type
        && !values[ct].is_empty()
    {
        return None;
    }
    let mut live = vec![false; names.len()];
    for list in values {
        for i in list {
            live[i] = true;
        }
    }
    Some(live)
}

fn shapes<const N: usize>(ops: &[HeaderOp]) -> [HeaderShape; N] {
    const LABELS: [&str; 4] = ["c0", "c1", "c2", "c3"];
    std::array::from_fn(|i| HeaderShape {
        op: ops[i],
        label: LABELS[i],
    })
}

fn plan<const N: usize>(
    names: &[HeaderName; 3],
    chain: &[usize],
    ops: &[HeaderOp],
    present: &[bool],
) -> Option<Vec<bool>> {
    let refs: [&HeaderName; N] = std::array::from_fn(|i| &names[chain[i]]);
    let present: [bool; N] = std::array::from_fn(|i| present[i]);
    // What is sent: a live contribution sends its value when it has one (an absent
    // `override ... else inherit` source sends nothing; the generated code does not call it).
    HeaderPlan::<N>::compose(refs, &shapes(ops), present, [false; N], false, &META)
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
    // A marked contribution makes every contribution to the same header sensitive, before or after.
    let p = HeaderPlan::<3>::compose(
        [&other, &other, &auth],
        &shapes(&[HeaderOp::Set, HeaderOp::Append, HeaderOp::Set]),
        [true; 3],
        [false, true, false],
        false,
        &META,
    )
    .unwrap();
    assert_eq!(p.sensitive, [true, true, true]);
    let p = HeaderPlan::<2>::compose(
        [&other, &auth],
        &shapes(&[HeaderOp::Set, HeaderOp::Set]),
        [true; 2],
        [false; 2],
        false,
        &META,
    )
    .unwrap();
    assert_eq!(p.sensitive, [false, true]);
}

/// H13: over chains of up to four contributions on `authorization`, `content-type`, `accept` and
/// (for chains up to three) `host`, every operation, both media settings: `NameCheck` (no
/// presence) reports a problem exactly when `compose` fails, under every presence assignment, on
/// the same declaration; and both agree with the sequential reference with the media rule.
#[test]
fn the_validator_and_prepare_share_one_verdict() {
    fn run<const N: usize>(names: &[HeaderName], tally: &mut [usize; 2]) {
        let k = names.len();
        let content_type = names.iter().position(|n| n == "content-type");
        for code in 0..k.pow(N as u32) * 5usize.pow(N as u32) {
            let mut c = code;
            let mut chain = [0usize; N];
            let mut ops = [HeaderOp::Set; N];
            for i in 0..N {
                chain[i] = c % k;
                c /= k;
                ops[i] = OPS[c % 5];
                c /= 5;
            }
            let refs: [&HeaderName; N] = std::array::from_fn(|i| &names[chain[i]]);
            for explicit_media in [false, true] {
                let mut check = NameCheck::new();
                check.endpoint("Plan.endpoint", refs, &shapes(&ops), explicit_media);
                let validated = check.finish().map_err(|e| e.problems()[0].declaration());
                for mask in 0..1u32 << N {
                    let present: [bool; N] = std::array::from_fn(|i| mask & (1 << i) != 0);
                    let composed = HeaderPlan::<N>::compose(
                        refs,
                        &shapes(&ops),
                        present,
                        [false; N],
                        explicit_media,
                        &META,
                    );
                    assert_eq!(
                        composed.is_ok(),
                        validated.is_ok(),
                        "chain {chain:?} ops {:?} present {present:?} media {explicit_media}",
                        ops.map(|o| o as u8)
                    );
                    if let (Err(e), Err(label)) = (&composed, &validated) {
                        assert!(e.to_string().contains(label), "{e} vs {label}");
                    }
                    let reference = reference_with_media(
                        &chain,
                        &ops,
                        &present,
                        content_type.filter(|_| explicit_media),
                    );
                    // The reserved `host` is not modelled by the reference: only compare its verdict
                    // where no reserved name is declared.
                    if !refs
                        .iter()
                        .zip(&ops)
                        .any(|(n, op)| *n == "host" && *op != HeaderOp::Remove)
                    {
                        assert_eq!(composed.is_ok(), reference.is_some(), "chain {chain:?}");
                    }
                    tally[usize::from(composed.is_ok())] += 1;
                }
            }
        }
    }
    let four = [
        HeaderName::from_static("authorization"),
        HeaderName::from_static("content-type"),
        HeaderName::from_static("accept"),
        HeaderName::from_static("host"),
    ];
    let mut tally = [0usize; 2];
    run::<1>(&four, &mut tally);
    run::<2>(&four, &mut tally);
    run::<3>(&four, &mut tally);
    run::<4>(&four[..3], &mut tally);
    // Both outcomes are well represented (errors, successes).
    assert!(tally[0] > 100_000 && tally[1] > 10_000, "{tally:?}");
}
