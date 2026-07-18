# Concord phase contract map

Keep this artifact uncommitted under ignored `git_diff/`.

## Verified baseline

- repository root;
- starting HEAD and branch;
- clean or pre-existing state;
- previous source, evidence, and anchor commits;
- current implementation program and active phase authority;
- normative status and approval evidence.

## Authority map

For every affected behavior, list:

- exact requirement ID;
- normative owner and section;
- accepted decision, when applicable;
- architecture constraint;
- current planning owner;
- source proving current behavior.

Record contradictions explicitly.

## Acceptance scenarios

For each complete workflow, record:

- configuration and feature set;
- public or generated call;
- observable success or failure;
- finite/security/cancellation boundary;
- exact requirement IDs;
- test oracle.

## Requirement inventory

Group every exact claimed requirement ID by the repository's normative domains. State whether the
phase intends direct or partial evidence and name the unimplemented clause for every planned partial.

## Public and generated signatures

Copy or precisely reference contractual:

- public types, traits, methods, builders, and errors;
- DSL syntax and semantic rules;
- generated types, constructors, methods, endpoints, and terminals;
- feature gates and export locations;
- generated-private or private-only integration.

Do not expose private core paths or invent syntax.

## Current source and migration map

For affected source/test areas, classify concepts as:

- retain;
- evolve;
- replace;
- remove;
- defer.

Identify proof-only scaffolding, duplicate paths, and obsolete concepts that must not survive as a
competing production architecture.

## Previous guarantees

List the high-risk earlier-phase contracts that must retain semantic and test proof.

## Architecture invariants

Define the single production owner for each relevant lifecycle or state. Explain why helpers,
adapters, and test doubles cannot bypass it.

## Contradiction and decision review

Resolve before source edits:

- normative ownership;
- public compatibility;
- lifecycle ordering;
- security and redaction;
- finite limits and error precedence;
- feature/dependency ownership;
- generated API compatibility;
- downstream extension boundaries;
- later-phase compatibility;
- MSRV and Reqwest constraints.

Record accepted alternatives and consequences. Stop when an approved project decision is required.

## Test oracle map

For every important invariant, identify the exact proof type and intended path:

- semantic/unit;
- Trybuild/UI;
- deterministic runtime;
- real local protocol integration;
- feature/topology fixture;
- downstream consumer;
- Rustdoc/public API;
- redaction sentinel;
- boundary/property/model-checking test;
- maintained command.

## Negative and removal map

Record how the phase avoids or removes:

- parallel production architectures;
- speculative public APIs;
- later-phase features;
- private/native value leakage;
- unnecessary dependencies or feature flags;
- source-only Cargo fixtures;
- weakened validation or historical evidence changes.

## Implementation outcomes

List coherent sequential outcomes, focused validation for each, and intended commit boundaries.
