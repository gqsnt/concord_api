//! Data Dragon locale path values.
//!
//! Required features: the Riot bundle. Mode: deterministic planning.
//! Documentation owner: `docs/spec.md`.
//! Proving test: `tests/riot_contract.rs`.

use std::fmt;

#[derive(Clone, Copy, Debug)]
pub enum DDragonLocale {
    EnUs,
    FrFr,
}

impl fmt::Display for DDragonLocale {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::EnUs => "en_US",
            Self::FrFr => "fr_FR",
        })
    }
}
