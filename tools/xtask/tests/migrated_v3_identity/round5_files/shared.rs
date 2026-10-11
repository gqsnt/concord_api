//! One module file mounted twice: rustc compiles it as `round5::first::shared`
//! (where `super::Req` is the generated request) and as
//! `round5::second::shared` (where it is an unrelated type), so migrate-v3
//! may not rewrite a call here that it proved in one mount only.

/// Clears `x` through whichever `Req` the mount names.
pub fn clear(r: super::Req) -> super::Out {
    r.set_x(None) // unrelated in the second mount
}
