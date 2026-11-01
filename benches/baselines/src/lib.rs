//! Idiomatic hand-written reqwest wrappers for the S1–S3 shapes, written independently of Concord
//! during V4 research (see `v4/evidence/research/claude_ergonomics.md`). Used only as a baseline for
//! line counts, compile time and request-construction/execution timing in `v4/evidence/PERF.md`.
#![allow(missing_docs, clippy::new_without_default)]

pub mod s1_simple;
pub mod s2_scoped;
pub mod s3_query;
