//! Count name inspections without depending on machine speed or other tests.
//!
//! The counts cover the lookups and scans the name check has today. Work
//! added beside them, such as a new scan that keeps the existing lookup, is
//! not counted: a new loop over names needs its own inspection here.
use std::{
    cell::Cell,
    collections::HashMap,
    hash::{BuildHasher, DefaultHasher, Hasher, RandomState},
};

use crate::{ast::Parameter, check, flat, name, parse, resolve};

#[derive(Clone, Copy)]
pub(crate) enum Inspection {
    Index,
    InputRepair,
    ChildRepair,
    OptionalChild,
    SpreadInput,
    ChildHash,
    InputHash,
    ExpandedInput,
}

const KINDS: usize = Inspection::ExpandedInput as usize + 1;

thread_local! {
    static INSPECTIONS: Cell<[usize; KINDS]> = const { Cell::new([0; KINDS]) };
}

pub(crate) fn inspect(kind: Inspection) {
    INSPECTIONS.with(|count| {
        let mut counts = count.get();
        counts[kind as usize] += 1;
        count.set(counts);
    });
}

struct CountingBuildHasher(RandomState, Inspection);

impl BuildHasher for CountingBuildHasher {
    type Hasher = CountingHasher;

    fn build_hasher(&self) -> Self::Hasher {
        CountingHasher(self.0.build_hasher(), self.1)
    }
}

struct CountingHasher(DefaultHasher, Inspection);

impl Hasher for CountingHasher {
    fn write(&mut self, bytes: &[u8]) {
        self.0.write(bytes);
    }

    fn finish(&self) -> u64 {
        inspect(self.1);
        self.0.finish()
    }
}

pub(crate) fn counted_index<K, V>(
    capacity: usize,
    kind: Inspection,
) -> HashMap<K, V, impl BuildHasher> {
    HashMap::with_capacity_and_hasher(capacity, CountingBuildHasher(RandomState::new(), kind))
}

fn measured(source: &str, expanded: bool) -> [usize; KINDS] {
    let mut parsed = parse::parse(source.parse().unwrap());
    assert!(parsed.diagnostics.is_empty());
    let client = parsed.client.as_ref().unwrap();
    INSPECTIONS.with(|count| count.set([0; KINDS]));
    name::check(client, &mut parsed.diagnostics);
    if expanded {
        let node = &client.node;
        let inputs = node
            .signature
            .as_ref()
            .unwrap()
            .entries
            .iter()
            .filter_map(|parameter| {
                if let Parameter::Input(input) = parameter {
                    Some(flat::InputLine {
                        input,
                        owner: node,
                        written: node,
                    })
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        let owners = [flat::Owner {
            node,
            inputs: inputs.clone(),
        }];
        name::expanded(node, &inputs, &owners, &mut parsed.diagnostics);
    }
    let count = INSPECTIONS.with(Cell::get);
    assert!(parsed.diagnostics.is_empty());
    count
}

#[test]
fn valid_siblings_have_linear_name_work() {
    const SIZE: usize = 2_000;
    let mut source = "client Scale { base \"https://scale.test\";".to_owned();
    for i in 0..SIZE {
        source.push_str(&format!("GET endpoint_{i}();"));
    }
    source.push('}');
    let counts = measured(&source, false);
    assert_eq!(counts[Inspection::Index as usize], 2 * SIZE);
    let count: usize = counts.iter().sum();
    assert!(
        count <= 8 * SIZE,
        "{count} name inspections for {SIZE} siblings (limit {})",
        8 * SIZE
    );
}

#[test]
fn optional_inputs_and_children_have_linear_name_work() {
    const SIZE: usize = 2_000;
    let mut source = "client Scale(".to_owned();
    for i in 0..SIZE {
        source.push_str(&format!("option_{i}?: u32,"));
    }
    source.push_str(") { base \"https://scale.test\";");
    for i in 0..SIZE {
        source.push_str(&format!("GET endpoint_{i}();"));
    }
    source.push('}');
    let counts = measured(&source, true);
    assert_eq!(
        (
            counts[Inspection::ChildHash as usize],
            counts[Inspection::OptionalChild as usize],
        ),
        (2 * SIZE, SIZE),
        "child index hashes and lookup attempts"
    );
    let count: usize = counts.iter().sum();
    assert!(
        count <= 16 * SIZE,
        "{count} name inspections for {SIZE} inputs and siblings (limit {})",
        16 * SIZE
    );
}

#[test]
fn repair_candidates_still_inspect_conflicts() {
    let mut parsed = parse::parse(
        "client Names { GET BadName(); GET bad_name(); }"
            .parse()
            .unwrap(),
    );
    name::check(parsed.client.as_ref().unwrap(), &mut parsed.diagnostics);
    assert!(!parsed.diagnostics.is_empty());
    // The count also observes the error-only path; a disconnected counter
    // would let an eager repair scan pass the growth checks.
    INSPECTIONS.with(|count| count.set([0; KINDS]));
    let client = parsed.client.as_ref().unwrap();
    let current = client.node.children.first().unwrap();
    assert!(!super::child_candidate(
        &client.node,
        current,
        "bad_name",
        &[]
    ));
    assert!(INSPECTIONS.with(Cell::get)[Inspection::ChildRepair as usize] > 0);
}

fn assert_part_work(inputs: usize, endpoints: usize) {
    let mut source = "client Scale { base \"https://scale.test\"; part shared(".to_owned();
    for i in 0..inputs {
        source.push_str(&format!("input_{i}: u32,"));
    }
    source.push_str(") { query { \"all\" = [");
    for i in 0..inputs {
        source.push_str(&format!("input_{i},"));
    }
    source.push_str("].len(); } }");
    for i in 0..endpoints {
        source.push_str(&format!("GET endpoint_{i}(..shared);"));
    }
    source.push('}');
    let mut parsed = parse::parse(source.parse().unwrap());
    assert!(parsed.diagnostics.is_empty());
    let client = parsed.client.as_ref().unwrap();
    INSPECTIONS.with(|count| count.set([0; KINDS]));
    name::check(client, &mut parsed.diagnostics);
    let resolved = resolve::resolve(client, &mut parsed.diagnostics);
    let counts = INSPECTIONS.with(Cell::get);
    check::check(client, &resolved, &mut parsed.diagnostics);
    assert!(
        parsed.diagnostics.is_empty(),
        "{:?}",
        parsed.diagnostics.messages()
    );
    let occurrences = inputs * endpoints;
    assert_eq!(resolved.endpoints.len(), endpoints);
    assert!(resolved
        .endpoints
        .iter()
        .all(|endpoint| endpoint.inputs.len() == inputs));
    assert_eq!(
        (
            counts[Inspection::InputHash as usize],
            counts[Inspection::SpreadInput as usize],
        ),
        (occurrences, occurrences),
        "input index hashes and lookup attempts"
    );
    assert_eq!(counts[Inspection::ExpandedInput as usize], occurrences);
    let written_names = 2 + inputs + 2 * endpoints; // client, part, inputs, endpoints, spreads
    let count: usize = counts.iter().sum();
    let limit = 4 * (written_names + occurrences);
    assert!(count <= limit, "{count} name inspections for {written_names} written names and {occurrences} spread inputs (limit {limit})");
    assert!(
        count <= 16 * occurrences,
        "{count} name inspections for {inputs} inputs spread by {endpoints} endpoints"
    );
}

#[test]
fn name_work_is_linear_in_written_names_and_spread_input_occurrences() {
    assert_part_work(128, 128);
}

#[test]
fn one_spread_has_linear_name_work_in_its_input_width() {
    assert_part_work(2_000, 1);
}
