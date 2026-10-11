//! The module file of `round4::thread::local_data`: rustc reads it through the
//! inline module's `#[path = "round4_files"]`, and so must migrate-v3 (a stray
//! file at the default location declares the opposite types).

/// An unrelated type.
pub type R = super::super::other::ItemRequest;

/// A generated type.
pub type G = super::super::api::ItemRequest;
