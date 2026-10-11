//! Value-free runtime program and binding facts (EXPLAIN-04, RUNTIME-02).

use std::fmt;
use std::sync::Arc;

use crate::auth::AuthTarget;

/// How a runtime endpoint treats its binding owner's generated path.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum RuntimeRouteMode {
    /// Runtime segments are appended after the owner's generated segments.
    Inherit,
    /// `path_replace()`: the owner's generated segments are dropped and the
    /// runtime segments follow the fixed base path.
    Replace,
    /// `path_off()`: the owner's generated segments are dropped and nothing
    /// is appended; the fixed base path remains.
    Off,
}

/// A runtime endpoint's whole-family mode for its query or header program.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum RuntimeFamilyMode {
    /// The owner's program is inherited and runtime statements follow it.
    Inherit,
    /// `query_replace_all()`/`header_replace_all()`: the inherited emission
    /// program is cleared before the runtime statements.
    ReplaceAll,
    /// `query_off()`/`header_off()`: the inherited emission program is
    /// cleared and the runtime endpoint emits nothing in the family.
    Off,
}

/// The family one runtime keyed operation targets.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum RuntimeFamily {
    /// Public query occurrences (case-sensitive keys).
    Query,
    /// Public header occurrences (case-insensitive names).
    Header,
}

/// The structural meaning of one runtime keyed operation (PATCH-02).
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum RuntimeOperation {
    /// Plain introduction; no earlier potential writer may exist.
    Introduce,
    /// Exact replacement of an earlier potential writer.
    Replace,
    /// Conditional replacement of an earlier potential writer.
    WhenPresent,
    /// Appended occurrences.
    Append,
    /// Removal of every earlier occurrence.
    Remove,
}

/// One value-free runtime keyed operation, in source order.
///
/// The key name is safe structural data; no value, encoded occurrence or
/// adapter output is ever recorded.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RuntimeOperationFact {
    family: RuntimeFamily,
    operation: RuntimeOperation,
    key: Arc<str>,
    sensitive: bool,
    repeated: bool,
    optional: bool,
    adapter: Option<&'static str>,
}

impl RuntimeOperationFact {
    pub(crate) fn new(
        family: RuntimeFamily,
        operation: RuntimeOperation,
        key: Arc<str>,
        sensitive: bool,
        repeated: bool,
        optional: bool,
        adapter: Option<&'static str>,
    ) -> Self {
        Self {
            family,
            operation,
            key,
            sensitive,
            repeated,
            optional,
            adapter,
        }
    }

    /// The targeted family.
    #[must_use]
    pub const fn family(&self) -> RuntimeFamily {
        self.family
    }

    /// The structural operation.
    #[must_use]
    pub const fn operation(&self) -> RuntimeOperation {
        self.operation
    }

    /// The target key (query) or field name (header).
    #[must_use]
    pub fn key(&self) -> &str {
        &self.key
    }

    /// Whether the emission is explicitly classified sensitive. Sensitivity
    /// is sticky per key along the whole owner-to-runtime program.
    #[must_use]
    pub const fn sensitive(&self) -> bool {
        self.sensitive
    }

    /// Whether the source is an ordered `each` sequence.
    #[must_use]
    pub const fn repeated(&self) -> bool {
        self.repeated
    }

    /// Whether the source has a Concord-owned outer presence (`Option`).
    #[must_use]
    pub const fn optional(&self) -> bool {
        self.optional
    }

    /// The explicit atom adapter type, when the `_via` form was used.
    #[must_use]
    pub const fn adapter(&self) -> Option<&'static str> {
        self.adapter
    }
}

/// What a runtime endpoint does with its binding owner's authentication.
///
/// Removing inherited authentication changes the request's declared security
/// behavior; it is never described as preserving the owner's authorization
/// semantics (RUNTIME-02). Runtime endpoints cannot create placements, select
/// another credential source or change a challenge.
#[non_exhaustive]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RuntimeAuthDisposition {
    /// Every effective owner placement is inherited unchanged.
    Inherited,
    /// `auth_off()`: every inherited placement is removed.
    RemovedAll,
    /// `auth_remove(target)`: exactly these inherited placements are removed.
    Removed(Arc<[AuthTarget]>),
}

impl RuntimeAuthDisposition {
    /// Whether any inherited authentication is removed.
    #[must_use]
    pub const fn removes_inherited(&self) -> bool {
        !matches!(self, Self::Inherited)
    }
}

impl fmt::Display for RuntimeAuthDisposition {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Inherited => formatter.write_str("inherits the owner's effective authentication"),
            Self::RemovedAll => formatter.write_str(
                "INHERITED AUTHENTICATION REMOVED by runtime auth_off(): this request does not \
                 carry the owner's declared credentials and does not preserve its authorization \
                 semantics",
            ),
            Self::Removed(targets) => {
                formatter.write_str(
                    "INHERITED AUTHENTICATION PARTIALLY REMOVED by runtime auth_remove(): ",
                )?;
                for (index, target) in targets.iter().enumerate() {
                    if index > 0 {
                        formatter.write_str(", ")?;
                    }
                    write!(formatter, "{target}")?;
                }
                formatter.write_str(
                    " no longer applied; the request does not preserve the owner's authorization \
                     semantics",
                )
            }
        }
    }
}

/// Value-free facts of one runtime endpoint's local program and, once bound,
/// its binding owner (EXPLAIN-04).
///
/// Available through [`EndpointDescription::runtime_binding`] and
/// [`RequestPlan::runtime_binding`]. It records the explicit runtime
/// removals and overrides with provenance; the inherited owner attachments
/// themselves are reflected by the bound plan's URL, headers, capabilities,
/// rate summary and timeout.
///
/// [`EndpointDescription::runtime_binding`]: crate::contracts::EndpointDescription::runtime_binding
/// [`RequestPlan::runtime_binding`]: crate::contracts::RequestPlan::runtime_binding
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RuntimeBindingFacts {
    pub(crate) owner: Option<&'static str>,
    pub(crate) route: RuntimeRouteMode,
    pub(crate) path_segments: usize,
    pub(crate) sensitive_path_segments: usize,
    pub(crate) query: RuntimeFamilyMode,
    pub(crate) headers: RuntimeFamilyMode,
    pub(crate) operations: Arc<[RuntimeOperationFact]>,
    pub(crate) authentication: RuntimeAuthDisposition,
    pub(crate) inherited_auth_placements: usize,
    pub(crate) effective_auth_placements: usize,
    pub(crate) rate_policies: Arc<[Arc<str>]>,
    pub(crate) effective_rate_policies: usize,
    pub(crate) accepted_bytes: bool,
}

impl RuntimeBindingFacts {
    /// The value-free display path of the binding owner (for example
    /// `CatalogApi::regional`), or `None` before binding.
    #[must_use]
    pub const fn owner(&self) -> Option<&'static str> {
        self.owner
    }

    /// How the owner's generated path is treated.
    #[must_use]
    pub const fn route(&self) -> RuntimeRouteMode {
        self.route
    }

    /// Number of runtime path segments.
    #[must_use]
    pub const fn path_segments(&self) -> usize {
        self.path_segments
    }

    /// Number of runtime path segments classified sensitive (rendered as a
    /// whole-segment redaction in automatic observation).
    #[must_use]
    pub const fn sensitive_path_segments(&self) -> usize {
        self.sensitive_path_segments
    }

    /// The runtime query family mode.
    #[must_use]
    pub const fn query(&self) -> RuntimeFamilyMode {
        self.query
    }

    /// The runtime header family mode.
    #[must_use]
    pub const fn headers(&self) -> RuntimeFamilyMode {
        self.headers
    }

    /// The runtime keyed operations in source order.
    #[must_use]
    pub fn operations(&self) -> &[RuntimeOperationFact] {
        &self.operations
    }

    /// The authentication disposition; see [`RuntimeAuthDisposition`].
    #[must_use]
    pub const fn authentication(&self) -> &RuntimeAuthDisposition {
        &self.authentication
    }

    /// Number of effective owner placements before runtime removal (zero
    /// before binding).
    #[must_use]
    pub const fn inherited_auth_placements(&self) -> usize {
        self.inherited_auth_placements
    }

    /// Number of placements the bound request actually applies.
    #[must_use]
    pub const fn effective_auth_placements(&self) -> usize {
        self.effective_auth_placements
    }

    /// IDs of the runtime-added rate policies, in attachment order. Every
    /// inherited owner policy additionally remains in force.
    #[must_use]
    pub fn rate_policies(&self) -> &[Arc<str>] {
        &self.rate_policies
    }

    /// Number of distinct effective policies after binding (inherited plus
    /// runtime, exact duplicates deduplicated); zero before binding.
    #[must_use]
    pub const fn effective_rate_policies(&self) -> usize {
        self.effective_rate_policies
    }

    /// Whether the descriptor explicitly exposes the accepted-bytes terminal.
    #[must_use]
    pub const fn accepted_bytes(&self) -> bool {
        self.accepted_bytes
    }
}
