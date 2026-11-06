//! Compatibility facade for the standard formats in [`concord_codecs`].
//!
//! Every exported item is the same item as in `concord-codecs`. New consumers can depend on
//! `concord-codecs` directly. Features of this crate forward to the corresponding codec features.

pub use concord_codecs::*;
