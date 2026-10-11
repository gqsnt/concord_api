//! Generated owner projections (RUNTIME-01, BRIDGE-01, §16.3).
//!
//! Every generated client and scope handle implements [`OwnerProjection`].
//! The projection is the single source of the contributions an endpoint
//! inherits from its owner chain: generated host labels, generated path
//! segments, the owner's query/header patch-program segments, effective
//! authentication placements, rate-limit declarations and the effective
//! timeout selection. Static generated endpoints and runtime endpoints bound
//! at the same owner use the same projection, so binding at the root uses the
//! same projection as binding at a scope and there is no root-only bare-URL
//! path.
//!
//! A projection is not an authenticity token. It cannot supply a destination
//! outside the fixed base, cannot place new private credentials (it only
//! forwards placements whose credential references were created by this
//! client's build and are validated against its credential context), and
//! cannot bypass the executor: the client core it exposes is only usable
//! through core-owned request construction.

use std::time::Duration;

use super::{HeadBuilder, ProgramSegment};
use crate::client::ClientCore;
use crate::error::ApiError;

/// Visitor over one owner chain's program segments (root first).
#[doc(hidden)]
pub type ProgramVisitor<'v> = dyn FnMut(&[ProgramSegment<'_>]) -> Result<(), ApiError> + 'v;

/// Generated-private projection of one selected owner (client or scope).
#[doc(hidden)]
pub trait OwnerProjection: Send + Sync + 'static {
    /// The shared client core of the owner's client.
    fn core(&self) -> &ClientCore;

    /// Value-free display path of the owner (for example `CatalogApi::regional`).
    fn owner_path(&self) -> &'static str;

    /// Effective timeout selection of the owner chain; `None` inherits the
    /// client default.
    fn owner_timeout(&self) -> Option<Duration>;

    /// Appends the owner chain's generated host labels, leftmost label first.
    fn push_host(&self, builder: &mut HeadBuilder) -> Result<(), ApiError>;

    /// Appends the owner chain's generated path segments.
    fn push_path(&self, builder: &mut HeadBuilder) -> Result<(), ApiError>;

    /// Appends the owner chain's effective authentication placements.
    fn push_auth(&self, builder: &mut HeadBuilder) -> Result<(), ApiError>;

    /// Appends the owner chain's effective rate-limit declarations.
    fn push_rate_limits(&self, builder: &mut HeadBuilder) -> Result<(), ApiError>;

    /// Whether [`Self::push_auth`] contributes at least one placement.
    fn owner_has_auth(&self) -> bool;

    /// Whether [`Self::push_rate_limits`] contributes at least one declaration.
    fn owner_has_rate_limits(&self) -> bool;

    /// Calls `visit` exactly once with the owner chain's query/header program
    /// segments in lexical order (root first). The segments borrow the
    /// owner's immutable snapshots for the duration of the call.
    fn visit_program(&self, visit: &mut ProgramVisitor<'_>) -> Result<(), ApiError>;

    /// Operational IDs of every generated endpoint of the owner's client; a
    /// runtime endpoint binding may not collide with any of them.
    fn generated_endpoint_ids(&self) -> &'static [&'static str];
}
