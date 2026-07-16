//! Test-only credential transition model.
//!
//! This module deliberately models identities and state transitions, not real
//! credential material or a production authentication API.

#![allow(dead_code)]

mod challenge;
mod model;
mod transition;

#[cfg(test)]
mod systematic;
#[cfg(test)]
mod tests;
