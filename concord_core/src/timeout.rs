use core::time::Duration;

/// Runtime override for request timeout.
///
/// - `Inherit`: keep timeout from client/prefix/path/endpoint policy layers.
/// - `Clear`: remove any configured timeout for this request (no per-request timeout).
/// - `Set(d)`: force timeout for this request.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum TimeoutOverride {
    Inherit,
    Clear,
    Set(Duration),
}

impl Default for TimeoutOverride {
    #[inline]
    fn default() -> Self {
        Self::Inherit
    }
}
