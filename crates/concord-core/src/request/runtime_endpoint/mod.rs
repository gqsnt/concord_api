//! Runtime endpoints (§16, RUNTIME-01/02).
//!
//! A [`RuntimeEndpoint`] is a typed, manually constructed endpoint relative to
//! a selected generated owner (client root or scope). Construction records a
//! bounded **symbolic** operation program: path atoms, query/header patch
//! statements with owned caller values, family fences, authentication
//! removals, additive rate policies and endpoint-local status/effect/timeout
//! selections. No value is encoded, no iterator or adapter is invoked and no
//! body, credential, limiter or transport work happens at construction or
//! binding. Values are encoded only at plan/execution time by the same
//! [`apply_head_program`](crate::request::apply_head_program) evaluator that
//! generated endpoints use: the runtime program is one more
//! [`ProgramSegment`](crate::request::ProgramSegment) after the owner chain's
//! segments, so suppression, ordering and sticky sensitivity are identical to
//! static code (PATCH-01..03, PRIVACY-01).
//!
//! A runtime endpoint is not an escape: it cannot supply an origin, scheme,
//! port or absolute URL, cannot create a private authentication placement,
//! select another credential source or change a challenge, and cannot remove
//! an inherited rate policy. Binding validates owner-level collisions,
//! authentication/status relationships, generated-ID collisions and rate
//! policy compatibility (including limiter presence) before any operation.

mod bind;
mod emission;
mod facts;
#[cfg(test)]
pub(crate) mod test_owner;
#[cfg(test)]
mod tests;

use std::fmt;
use std::marker::PhantomData;
use std::sync::Arc;
use std::time::Duration;

use http::{HeaderName, Method, StatusCode};

pub use bind::BoundRuntimeRequest;
pub(crate) use bind::{bind_runtime_at_owner, plan_runtime_at_owner};
pub use facts::{
    RuntimeAuthDisposition, RuntimeBindingFacts, RuntimeFamily, RuntimeFamilyMode,
    RuntimeOperation, RuntimeOperationFact, RuntimeRouteMode,
};

use self::emission::{
    Canonical, EachSlot, EmissionTable, PathSlot, RuntimeEmission, RuntimePathAtom, ScalarEncoding,
    ScalarSlot, Via,
};
use super::program::{NO_EMISSION, PatchFamily, PatchKind, PatchOp};
use crate::atom::{AtomEncoder, OrderedValues, RequestAtom};
use crate::auth::AuthTarget;
use crate::body::{BodyDescription, EmptyBody, PreparedRequestBody, RequestRepresentation};
use crate::construction::{ErrorLatch, StructuralBudget};
use crate::endpoint::{
    AcceptedStatus, Effect, EffectSource, EndpointCapabilities, EndpointDescription, EndpointId,
    MAX_RUNTIME_ENDPOINT_COMPONENTS, MAX_RUNTIME_ENDPOINT_METHOD_BYTES,
    MAX_RUNTIME_ENDPOINT_PUBLIC_BYTES, RequestBodyCategory, StatusSource, TimeoutSelection,
};
use crate::error::{ConfigurationError, ConfigurationErrorKind};
use crate::rate_limit::RateLimitDeclaration;
use crate::response::{AcceptedBytesDriver, ResponseDriver, ResponseRepresentation};

/// Maximum UTF-8 bytes of one runtime query key or header name (§20.1
/// "static query/header target name").
pub const MAX_RUNTIME_KEY_BYTES: usize = 256;

/// Largest number of distinct final statuses (`200..=599`).
const MAX_STATUS_CODES: usize = 400;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AuthMode {
    Inherit,
    Off,
}

/// One recorded runtime keyed statement or family fence.
struct RuntimeOp {
    family: PatchFamily,
    kind: PatchKind,
    key: Arc<str>,
    emission: u16,
    sensitive: bool,
}

impl RuntimeOp {
    fn as_patch(&self) -> PatchOp<'_> {
        PatchOp {
            family: self.family,
            kind: self.kind,
            key: &self.key,
            emission: self.emission,
            sensitive: self.sensitive,
        }
    }
}

struct RuntimePathSegment {
    atom: Box<dyn RuntimePathAtom>,
    sensitive: bool,
}

/// The immutable recorded program shared by an endpoint and every request
/// bound from it.
pub(crate) struct RuntimeProgram {
    id: EndpointId,
    method: Method,
    route: RuntimeRouteMode,
    path: Vec<RuntimePathSegment>,
    ops: Vec<RuntimeOp>,
    table: EmissionTable,
    auth: AuthMode,
    auth_removals: Vec<AuthTarget>,
    rate_limits: Vec<RateLimitDeclaration>,
    status: Option<AcceptedStatus>,
    accepted_bytes: bool,
    description: EndpointDescription,
    facts: RuntimeBindingFacts,
}

/// Construction-time state of one family (query or headers).
struct FamilyState {
    mode: RuntimeFamilyMode,
    local_ops: bool,
    /// Keys the runtime program itself potentially writes so far.
    writers: Vec<Arc<str>>,
}

impl FamilyState {
    const fn new() -> Self {
        Self {
            mode: RuntimeFamilyMode::Inherit,
            local_ops: false,
            writers: Vec::new(),
        }
    }
}

struct Parts {
    id: EndpointId,
    method: Method,
    route: RuntimeRouteMode,
    path: Vec<RuntimePathSegment>,
    ops: Vec<RuntimeOp>,
    op_facts: Vec<RuntimeOperationFact>,
    table: EmissionTable,
    query: FamilyState,
    header: FamilyState,
    auth: AuthMode,
    auth_removals: Vec<AuthTarget>,
    rate_limits: Vec<RateLimitDeclaration>,
    body: Option<(PreparedRequestBody, &'static str)>,
    timeout: Option<Duration>,
    status: Option<AcceptedStatus>,
    effect: Option<Effect>,
    accepted_bytes: bool,
    budget: StructuralBudget,
}

impl Parts {
    fn family_mut(&mut self, family: PatchFamily) -> &mut FamilyState {
        match family {
            PatchFamily::Query => &mut self.query,
            PatchFamily::Header => &mut self.header,
        }
    }

    fn reserve(
        &mut self,
        components: usize,
        bytes: usize,
        kind: ConfigurationErrorKind,
    ) -> Result<(), ConfigurationErrorKind> {
        self.budget
            .try_add(
                components,
                bytes,
                MAX_RUNTIME_ENDPOINT_COMPONENTS,
                MAX_RUNTIME_ENDPOINT_PUBLIC_BYTES,
            )
            .map_err(|_| kind)
    }
}

/// Value-free shape of one keyed emission source, for facts.
#[derive(Clone, Copy)]
struct Shape {
    repeated: bool,
    optional: bool,
    adapter: Option<&'static str>,
}

/// Builds one [`RuntimeEndpoint`] as a bounded symbolic operation program.
///
/// Obtained from [`RuntimeEndpoint::builder`]. The builder starts without a
/// response selection (`RuntimeEndpointBuilder<(), ()>`); exactly one
/// [`response::<R, O>()`](RuntimeEndpointBuilder::response) call selects the
/// typed output pair and only then can [`build`](RuntimeEndpointBuilder::build)
/// produce a usable descriptor.
///
/// Every operation is recorded, not evaluated: caller values are stored
/// owned and encoded only at plan/execution time. Keys and methods are
/// validated immediately. The structural rules of static declarations apply
/// (§6.2, §16.2): a plain introduction cannot target a key the program
/// already potentially owns; `replace`/`when_present`/`remove` require an
/// earlier potential writer (checked against the owner at bind);
/// `query_replace_all()`/`query_off()` (and header equivalents) must precede
/// every same-family statement and appear at most once; a singleton request
/// header cannot be repeated with `append`/`each`; transport-, media- and
/// authentication-owned header names are rejected; endpoint-local selections
/// (`timeout`, `status`, `effect`, `body_repr`, accepted-bytes exposure) are
/// singletons, not last-writer-wins.
///
/// **First-error latch (V3-S11).** The first structural or limit error is
/// latched: every later call drops its argument immediately without
/// retaining, iterating or encoding it, and [`build`](Self::build) returns
/// that first error. At most [`MAX_RUNTIME_ENDPOINT_COMPONENTS`] operations
/// and [`MAX_RUNTIME_ENDPOINT_PUBLIC_BYTES`] library-retained descriptor bytes
/// (method, keys, rate declarations, authentication targets) are recorded;
/// caller-owned payload allocations are the caller's own.
///
/// ```
/// # use concord_core::contracts::{EndpointId, Method, Text, ConfigurationError};
/// # use concord_core::contracts::RuntimeEndpoint;
/// # fn main() -> Result<(), ConfigurationError> {
/// let endpoint = RuntimeEndpoint::builder(EndpointId::new("catalog.dynamic.lookup")?, Method::GET)
///     .path("items")
///     .path("42")
///     .query("view", "summary")
///     .response::<Text, String>()
///     .build()?;
/// assert_eq!(endpoint.description().id(), "catalog.dynamic.lookup");
/// # Ok(()) }
/// ```
#[must_use = "a runtime endpoint builder does nothing until built and bound"]
pub struct RuntimeEndpointBuilder<R = (), O = ()> {
    state: ErrorLatch<Parts, ConfigurationErrorKind>,
    response: PhantomData<fn() -> (R, O)>,
}

/// One immutable runtime-defined endpoint descriptor.
///
/// It records a symbolic program relative to whichever generated owner it
/// is later bound at: `owner.bind(endpoint)` (client root or any generated
/// scope handle) returns a [`BoundRuntimeRequest`] that inherits the owner's
/// fixed origin, generated host labels, path, query/header program,
/// authentication, timeout and every rate constraint — the same projection a
/// generated endpoint uses, with no root-only bare-URL path.
/// `owner.plan_runtime(&endpoint)` builds the same inert plan without
/// binding. The descriptor owns its request body input, so it is not
/// `Clone`; hold a factory closure to bind the same definition repeatedly.
pub struct RuntimeEndpoint<R = (), O = ()> {
    program: Arc<RuntimeProgram>,
    body: PreparedRequestBody,
    response: PhantomData<fn() -> (R, O)>,
}

impl RuntimeEndpoint<(), ()> {
    /// Starts a runtime endpoint with a validated operational identity and
    /// HTTP method.
    ///
    /// The method must be a token of at most
    /// [`MAX_RUNTIME_ENDPOINT_METHOD_BYTES`] bytes; `CONNECT` and `TRACE`
    /// are rejected case-insensitively (§10.1). The returned builder has no
    /// response selection yet; see [`RuntimeEndpointBuilder::response`].
    pub fn builder(endpoint_id: EndpointId, method: Method) -> RuntimeEndpointBuilder<(), ()> {
        let method_bytes = method.as_str().len();
        let forbidden = method.as_str().eq_ignore_ascii_case("CONNECT")
            || method.as_str().eq_ignore_ascii_case("TRACE");
        if method_bytes > MAX_RUNTIME_ENDPOINT_METHOD_BYTES || forbidden {
            return RuntimeEndpointBuilder {
                state: ErrorLatch::from_error(ConfigurationErrorKind::InvalidValue),
                response: PhantomData,
            };
        }
        let mut parts = Parts {
            id: endpoint_id,
            method,
            route: RuntimeRouteMode::Inherit,
            path: Vec::new(),
            ops: Vec::new(),
            op_facts: Vec::new(),
            table: EmissionTable::default(),
            query: FamilyState::new(),
            header: FamilyState::new(),
            auth: AuthMode::Inherit,
            auth_removals: Vec::new(),
            rate_limits: Vec::new(),
            body: None,
            timeout: None,
            status: None,
            effect: None,
            accepted_bytes: false,
            budget: StructuralBudget::new(),
        };
        let state = match parts.reserve(1, method_bytes, ConfigurationErrorKind::InvalidValue) {
            Ok(()) => ErrorLatch::from_value(parts),
            Err(kind) => ErrorLatch::from_error(kind),
        };
        RuntimeEndpointBuilder {
            state,
            response: PhantomData,
        }
    }
}

impl<R, O> RuntimeEndpoint<R, O> {
    /// Returns the value-free unbound description (identity, method, body
    /// and response facts, local selections and runtime program facts).
    #[must_use]
    pub fn description(&self) -> &EndpointDescription {
        &self.program.description
    }
}

impl<R, O> fmt::Debug for RuntimeEndpoint<R, O> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RuntimeEndpoint")
            .field("description", &self.program.description)
            .finish_non_exhaustive()
    }
}

impl<R, O> fmt::Debug for RuntimeEndpointBuilder<R, O> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RuntimeEndpointBuilder")
            .finish_non_exhaustive()
    }
}

fn family_error(family: PatchFamily) -> ConfigurationErrorKind {
    match family {
        PatchFamily::Query => ConfigurationErrorKind::InvalidValue,
        PatchFamily::Header => ConfigurationErrorKind::InvalidHeader,
    }
}

fn same_key(family: PatchFamily, left: &str, right: &str) -> bool {
    match family {
        PatchFamily::Query => left == right,
        PatchFamily::Header => left.eq_ignore_ascii_case(right),
    }
}

/// Validates a runtime key: query keys are nonempty and bounded; header names
/// are bounded HTTP field-name tokens that are not transport-, media- or
/// authentication-owned (HEAD-01).
fn validate_key(family: PatchFamily, key: &str) -> Result<(), ConfigurationErrorKind> {
    let invalid = family_error(family);
    if key.is_empty() || key.len() > MAX_RUNTIME_KEY_BYTES {
        return Err(invalid);
    }
    if family == PatchFamily::Header {
        HeaderName::from_bytes(key.as_bytes()).map_err(|_| invalid)?;
        if crate::inspection::FORBIDDEN_PUBLIC_HEADERS
            .iter()
            .any(|forbidden| key.eq_ignore_ascii_case(forbidden))
        {
            return Err(invalid);
        }
    }
    Ok(())
}

fn singleton_header(key: &str) -> bool {
    crate::inspection::SINGLETON_HEADERS
        .iter()
        .any(|singleton| key.eq_ignore_ascii_case(singleton))
}

const fn runtime_family(family: PatchFamily) -> RuntimeFamily {
    match family {
        PatchFamily::Query => RuntimeFamily::Query,
        PatchFamily::Header => RuntimeFamily::Header,
    }
}

const fn runtime_operation(kind: PatchKind) -> RuntimeOperation {
    match kind {
        PatchKind::Introduce | PatchKind::Clear => RuntimeOperation::Introduce,
        PatchKind::Replace => RuntimeOperation::Replace,
        PatchKind::WhenPresent => RuntimeOperation::WhenPresent,
        PatchKind::Append => RuntimeOperation::Append,
        PatchKind::Remove => RuntimeOperation::Remove,
    }
}

/// Four forwarding methods (plain, `_via`, `_sensitive`, `_sensitive_via`)
/// over one operation representation for each keyed family and shape.
macro_rules! keyed_methods {
    (scalar, $family:expr, $kind:expr, $doc:expr,
     $plain:ident, $via:ident, $sensitive:ident, $sensitive_via:ident) => {
        #[doc = $doc]
        #[doc = ""]
        #[doc = "Takes one owned value encoded by its canonical `RequestAtom` at plan/execution time."]
        pub fn $plain<T>(self, name: &str, value: T) -> Self
        where
            T: RequestAtom + Send + Sync + 'static,
        {
            self.keyed_scalar::<T, Canonical>($family, $kind, name, Some(value), false, false)
        }

        #[doc = $doc]
        #[doc = ""]
        #[doc = "Encodes the owned value with the explicit adapter `A`."]
        pub fn $via<T, A>(self, name: &str, value: T) -> Self
        where
            T: Send + Sync + 'static,
            A: AtomEncoder<T> + 'static,
        {
            self.keyed_scalar::<T, Via<A>>($family, $kind, name, Some(value), false, false)
        }

        #[doc = $doc]
        #[doc = ""]
        #[doc = "Classifies the emission sensitive: the real request carries the value; automatic observation redacts every occurrence of the key (sticky)."]
        pub fn $sensitive<T>(self, name: &str, value: T) -> Self
        where
            T: RequestAtom + Send + Sync + 'static,
        {
            self.keyed_scalar::<T, Canonical>($family, $kind, name, Some(value), false, true)
        }

        #[doc = $doc]
        #[doc = ""]
        #[doc = "Sensitive classification with the explicit adapter `A`."]
        pub fn $sensitive_via<T, A>(self, name: &str, value: T) -> Self
        where
            T: Send + Sync + 'static,
            A: AtomEncoder<T> + 'static,
        {
            self.keyed_scalar::<T, Via<A>>($family, $kind, name, Some(value), false, true)
        }
    };
    (optional, $family:expr, $kind:expr, $doc:expr,
     $plain:ident, $via:ident, $sensitive:ident, $sensitive_via:ident) => {
        #[doc = $doc]
        #[doc = ""]
        #[doc = "Takes an exact outer-optional value; presence is read from the slot, never from an adapter."]
        pub fn $plain<T>(self, name: &str, value: Option<T>) -> Self
        where
            T: RequestAtom + Send + Sync + 'static,
        {
            self.keyed_scalar::<T, Canonical>($family, $kind, name, value, true, false)
        }

        #[doc = $doc]
        #[doc = ""]
        #[doc = "Encodes a present value with the explicit adapter `A`."]
        pub fn $via<T, A>(self, name: &str, value: Option<T>) -> Self
        where
            T: Send + Sync + 'static,
            A: AtomEncoder<T> + 'static,
        {
            self.keyed_scalar::<T, Via<A>>($family, $kind, name, value, true, false)
        }

        #[doc = $doc]
        #[doc = ""]
        #[doc = "Classifies the emission sensitive (sticky per key)."]
        pub fn $sensitive<T>(self, name: &str, value: Option<T>) -> Self
        where
            T: RequestAtom + Send + Sync + 'static,
        {
            self.keyed_scalar::<T, Canonical>($family, $kind, name, value, true, true)
        }

        #[doc = $doc]
        #[doc = ""]
        #[doc = "Sensitive classification with the explicit adapter `A`."]
        pub fn $sensitive_via<T, A>(self, name: &str, value: Option<T>) -> Self
        where
            T: Send + Sync + 'static,
            A: AtomEncoder<T> + 'static,
        {
            self.keyed_scalar::<T, Via<A>>($family, $kind, name, value, true, true)
        }
    };
    (each, $family:expr, $kind:expr, $doc:expr,
     $plain:ident, $via:ident, $sensitive:ident, $sensitive_via:ident) => {
        #[doc = $doc]
        #[doc = ""]
        #[doc = "Takes an owned ordered sequence; each item is one occurrence, encoded once, in order, only at plan/execution time. A present empty sequence emits no occurrence."]
        pub fn $plain<V>(self, name: &str, values: V) -> Self
        where
            V: OrderedValues + Send + Sync + 'static,
            V::Item: RequestAtom,
        {
            self.keyed_each::<V, Canonical>($family, $kind, name, Some(values), false, false)
        }

        #[doc = $doc]
        #[doc = ""]
        #[doc = "Encodes each item with the explicit per-item adapter `A`."]
        pub fn $via<V, A>(self, name: &str, values: V) -> Self
        where
            V: OrderedValues + Send + Sync + 'static,
            A: AtomEncoder<V::Item> + 'static,
        {
            self.keyed_each::<V, Via<A>>($family, $kind, name, Some(values), false, false)
        }

        #[doc = $doc]
        #[doc = ""]
        #[doc = "Classifies every occurrence sensitive (sticky per key)."]
        pub fn $sensitive<V>(self, name: &str, values: V) -> Self
        where
            V: OrderedValues + Send + Sync + 'static,
            V::Item: RequestAtom,
        {
            self.keyed_each::<V, Canonical>($family, $kind, name, Some(values), false, true)
        }

        #[doc = $doc]
        #[doc = ""]
        #[doc = "Sensitive classification with the explicit per-item adapter `A`."]
        pub fn $sensitive_via<V, A>(self, name: &str, values: V) -> Self
        where
            V: OrderedValues + Send + Sync + 'static,
            A: AtomEncoder<V::Item> + 'static,
        {
            self.keyed_each::<V, Via<A>>($family, $kind, name, Some(values), false, true)
        }
    };
    (each_optional, $family:expr, $kind:expr, $doc:expr,
     $plain:ident, $via:ident, $sensitive:ident, $sensitive_via:ident) => {
        #[doc = $doc]
        #[doc = ""]
        #[doc = "Takes an exact outer-optional ordered sequence. An absent sequence is not iterated; a present empty sequence is distinct from absence."]
        pub fn $plain<V>(self, name: &str, values: Option<V>) -> Self
        where
            V: OrderedValues + Send + Sync + 'static,
            V::Item: RequestAtom,
        {
            self.keyed_each::<V, Canonical>($family, $kind, name, values, true, false)
        }

        #[doc = $doc]
        #[doc = ""]
        #[doc = "Encodes each item with the explicit per-item adapter `A`."]
        pub fn $via<V, A>(self, name: &str, values: Option<V>) -> Self
        where
            V: OrderedValues + Send + Sync + 'static,
            A: AtomEncoder<V::Item> + 'static,
        {
            self.keyed_each::<V, Via<A>>($family, $kind, name, values, true, false)
        }

        #[doc = $doc]
        #[doc = ""]
        #[doc = "Classifies every occurrence sensitive (sticky per key)."]
        pub fn $sensitive<V>(self, name: &str, values: Option<V>) -> Self
        where
            V: OrderedValues + Send + Sync + 'static,
            V::Item: RequestAtom,
        {
            self.keyed_each::<V, Canonical>($family, $kind, name, values, true, true)
        }

        #[doc = $doc]
        #[doc = ""]
        #[doc = "Sensitive classification with the explicit per-item adapter `A`."]
        pub fn $sensitive_via<V, A>(self, name: &str, values: Option<V>) -> Self
        where
            V: OrderedValues + Send + Sync + 'static,
            A: AtomEncoder<V::Item> + 'static,
        {
            self.keyed_each::<V, Via<A>>($family, $kind, name, values, true, true)
        }
    };
}

/// All keyed families of one target family (§16.2 exact method names).
macro_rules! family_methods {
    ($family:expr, $label:literal,
     $introduce:ident, $introduce_via:ident, $introduce_sensitive:ident, $introduce_sensitive_via:ident,
     $optional:ident, $optional_via:ident, $optional_sensitive:ident, $optional_sensitive_via:ident,
     $each:ident, $each_via:ident, $each_sensitive:ident, $each_sensitive_via:ident,
     $each_optional:ident, $each_optional_via:ident, $each_optional_sensitive:ident, $each_optional_sensitive_via:ident,
     $replace:ident, $replace_via:ident, $replace_sensitive:ident, $replace_sensitive_via:ident,
     $replace_optional:ident, $replace_optional_via:ident, $replace_optional_sensitive:ident, $replace_optional_sensitive_via:ident,
     $replace_each:ident, $replace_each_via:ident, $replace_each_sensitive:ident, $replace_each_sensitive_via:ident,
     $replace_each_optional:ident, $replace_each_optional_via:ident, $replace_each_optional_sensitive:ident, $replace_each_optional_sensitive_via:ident,
     $when_present:ident, $when_present_via:ident, $when_present_sensitive:ident, $when_present_sensitive_via:ident,
     $when_present_each:ident, $when_present_each_via:ident, $when_present_each_sensitive:ident, $when_present_each_sensitive_via:ident,
     $append:ident, $append_via:ident, $append_sensitive:ident, $append_sensitive_via:ident,
     $append_optional:ident, $append_optional_via:ident, $append_optional_sensitive:ident, $append_optional_sensitive_via:ident,
     $append_each:ident, $append_each_via:ident, $append_each_sensitive:ident, $append_each_sensitive_via:ident,
     $append_each_optional:ident, $append_each_optional_via:ident, $append_each_optional_sensitive:ident, $append_each_optional_sensitive_via:ident,
     $remove:ident, $replace_all:ident, $off:ident) => {
        keyed_methods!(scalar, $family, PatchKind::Introduce,
            concat!("Introduces one ", $label, " occurrence (`\"k\" = e`). The key must have no earlier potential writer in the owner chain or this program."),
            $introduce, $introduce_via, $introduce_sensitive, $introduce_sensitive_via);
        keyed_methods!(optional, $family, PatchKind::Introduce,
            concat!("Introduces an outer-optional ", $label, " emission; absence emits nothing. The key must have no earlier potential writer."),
            $optional, $optional_via, $optional_sensitive, $optional_sensitive_via);
        keyed_methods!(each, $family, PatchKind::Introduce,
            concat!("Introduces repeated ", $label, " occurrences (`\"k\" = each v`). The key must have no earlier potential writer."),
            $each, $each_via, $each_sensitive, $each_sensitive_via);
        keyed_methods!(each_optional, $family, PatchKind::Introduce,
            concat!("Introduces an outer-optional repeated ", $label, " emission. The key must have no earlier potential writer."),
            $each_optional, $each_optional_via, $each_optional_sensitive, $each_optional_sensitive_via);
        keyed_methods!(scalar, $family, PatchKind::Replace,
            concat!("Exactly replaces every earlier ", $label, " occurrence of the key (`replace \"k\" = e`) at this position. Requires an earlier potential writer."),
            $replace, $replace_via, $replace_sensitive, $replace_sensitive_via);
        keyed_methods!(optional, $family, PatchKind::Replace,
            concat!("Exact ", $label, " replacement with an outer-optional source: absence removes every earlier occurrence. Requires an earlier potential writer."),
            $replace_optional, $replace_optional_via, $replace_optional_sensitive, $replace_optional_sensitive_via);
        keyed_methods!(each, $family, PatchKind::Replace,
            concat!("Exact ", $label, " replacement with repeated occurrences; a present empty sequence removes every earlier occurrence. Requires an earlier potential writer."),
            $replace_each, $replace_each_via, $replace_each_sensitive, $replace_each_sensitive_via);
        keyed_methods!(each_optional, $family, PatchKind::Replace,
            concat!("Exact ", $label, " replacement with an outer-optional sequence: absent or present-empty removes every earlier occurrence. Requires an earlier potential writer."),
            $replace_each_optional, $replace_each_optional_via, $replace_each_optional_sensitive, $replace_each_optional_sensitive_via);
        keyed_methods!(optional, $family, PatchKind::WhenPresent,
            concat!("Conditional ", $label, " replacement (`when_present`): an absent value keeps the earlier occurrences and positions; a present value replaces them at this position. Requires an earlier potential writer."),
            $when_present, $when_present_via, $when_present_sensitive, $when_present_sensitive_via);
        keyed_methods!(each_optional, $family, PatchKind::WhenPresent,
            concat!("Conditional repeated ", $label, " replacement: absent keeps the earlier occurrences; present (including present-empty) replaces them. Requires an earlier potential writer."),
            $when_present_each, $when_present_each_via, $when_present_each_sensitive, $when_present_each_sensitive_via);
        keyed_methods!(scalar, $family, PatchKind::Append,
            concat!("Appends one ", $label, " occurrence after the earlier ones (`append`)."),
            $append, $append_via, $append_sensitive, $append_sensitive_via);
        keyed_methods!(optional, $family, PatchKind::Append,
            concat!("Appends one ", $label, " occurrence when the outer value is present."),
            $append_optional, $append_optional_via, $append_optional_sensitive, $append_optional_sensitive_via);
        keyed_methods!(each, $family, PatchKind::Append,
            concat!("Appends repeated ", $label, " occurrences in sequence order."),
            $append_each, $append_each_via, $append_each_sensitive, $append_each_sensitive_via);
        keyed_methods!(each_optional, $family, PatchKind::Append,
            concat!("Appends repeated ", $label, " occurrences when the outer sequence is present."),
            $append_each_optional, $append_each_optional_via, $append_each_optional_sensitive, $append_each_optional_sensitive_via);

        #[doc = concat!("Removes every earlier ", $label, " occurrence of the key (`remove`). Requires an earlier potential writer; the removed key's sensitivity taint remains.")]
        pub fn $remove(self, name: &str) -> Self {
            self.keyed_op(
                $family,
                PatchKind::Remove,
                name,
                false,
                Shape { repeated: false, optional: false, adapter: None },
                None::<fn() -> Box<dyn RuntimeEmission>>,
            )
        }

        #[doc = concat!("Clears the inherited ", $label, " emission program and starts this runtime endpoint's replacement program; following ", $label, " statements populate it. Must precede every ", $label, " statement and appear at most once.")]
        pub fn $replace_all(self) -> Self {
            self.family_mode($family, RuntimeFamilyMode::ReplaceAll)
        }

        #[doc = concat!("Clears the inherited ", $label, " emission program with no local emission. Must precede every ", $label, " statement; no later ", $label, " statement is accepted.")]
        pub fn $off(self) -> Self {
            self.family_mode($family, RuntimeFamilyMode::Off)
        }
    };
}

impl<R, O> RuntimeEndpointBuilder<R, O> {
    fn update<F>(self, update: F) -> Self
    where
        F: FnOnce(Parts) -> Result<Parts, ConfigurationErrorKind>,
    {
        Self {
            state: self.state.then(update),
            response: PhantomData,
        }
    }

    fn keyed_scalar<T, E>(
        self,
        family: PatchFamily,
        kind: PatchKind,
        name: &str,
        value: Option<T>,
        optional: bool,
        sensitive: bool,
    ) -> Self
    where
        T: Send + Sync + 'static,
        E: ScalarEncoding<T>,
    {
        self.keyed_op(
            family,
            kind,
            name,
            sensitive,
            Shape {
                repeated: false,
                optional,
                adapter: E::adapter(),
            },
            Some(move || Box::new(ScalarSlot::<T, E>::new(value)) as Box<dyn RuntimeEmission>),
        )
    }

    fn keyed_each<V, E>(
        self,
        family: PatchFamily,
        kind: PatchKind,
        name: &str,
        values: Option<V>,
        optional: bool,
        sensitive: bool,
    ) -> Self
    where
        V: OrderedValues + Send + Sync + 'static,
        E: ScalarEncoding<V::Item>,
    {
        self.keyed_op(
            family,
            kind,
            name,
            sensitive,
            Shape {
                repeated: true,
                optional,
                adapter: E::adapter(),
            },
            Some(move || Box::new(EachSlot::<V, E>::new(values)) as Box<dyn RuntimeEmission>),
        )
    }

    fn keyed_op<F>(
        self,
        family: PatchFamily,
        kind: PatchKind,
        name: &str,
        sensitive: bool,
        shape: Shape,
        emission: Option<F>,
    ) -> Self
    where
        F: FnOnce() -> Box<dyn RuntimeEmission>,
    {
        self.update(move |mut parts| {
            let invalid = family_error(family);
            validate_key(family, name)?;
            if family == PatchFamily::Header
                && singleton_header(name)
                && (kind == PatchKind::Append || shape.repeated)
            {
                return Err(invalid);
            }
            let state = parts.family_mut(family);
            if state.mode == RuntimeFamilyMode::Off {
                return Err(invalid);
            }
            let existing = state
                .writers
                .iter()
                .position(|writer| same_key(family, writer, name));
            let cleared = state.mode == RuntimeFamilyMode::ReplaceAll;
            match kind {
                PatchKind::Introduce => {
                    if existing.is_some() {
                        return Err(invalid);
                    }
                    state.writers.push(Arc::from(name));
                }
                PatchKind::Replace | PatchKind::WhenPresent => {
                    // After a local family fence the inherited program is
                    // cleared: only an earlier runtime writer can satisfy
                    // the precondition. Otherwise the owner is checked at
                    // bind.
                    if existing.is_none() {
                        if cleared {
                            return Err(invalid);
                        }
                        state.writers.push(Arc::from(name));
                    }
                }
                PatchKind::Remove => match existing {
                    Some(index) => {
                        state.writers.remove(index);
                    }
                    None if cleared => return Err(invalid),
                    None => {}
                },
                PatchKind::Append => {
                    if existing.is_none() {
                        state.writers.push(Arc::from(name));
                    }
                }
                PatchKind::Clear => return Err(ConfigurationErrorKind::Internal),
            }
            state.local_ops = true;
            parts.reserve(1, name.len(), invalid)?;
            let index = match emission {
                Some(make) => {
                    let index = u16::try_from(parts.table.len()).map_err(|_| invalid)?;
                    if index == NO_EMISSION {
                        return Err(invalid);
                    }
                    parts.table.push(make());
                    index
                }
                None => NO_EMISSION,
            };
            let key: Arc<str> = Arc::from(name);
            parts.op_facts.push(RuntimeOperationFact::new(
                runtime_family(family),
                runtime_operation(kind),
                Arc::clone(&key),
                sensitive,
                shape.repeated,
                shape.optional,
                shape.adapter,
            ));
            parts.ops.push(RuntimeOp {
                family,
                kind,
                key,
                emission: index,
                sensitive,
            });
            Ok(parts)
        })
    }

    fn family_mode(self, family: PatchFamily, mode: RuntimeFamilyMode) -> Self {
        self.update(move |mut parts| {
            let invalid = family_error(family);
            let state = parts.family_mut(family);
            // A fence after recorded same-family statements would silently
            // discard caller work; a second fence would be last-writer-wins.
            if state.mode != RuntimeFamilyMode::Inherit || state.local_ops {
                return Err(invalid);
            }
            state.mode = mode;
            state.writers.clear();
            parts.reserve(1, 0, invalid)?;
            parts.ops.push(RuntimeOp {
                family,
                kind: PatchKind::Clear,
                key: Arc::from(""),
                emission: NO_EMISSION,
                sensitive: false,
            });
            Ok(parts)
        })
    }

    fn path_atom<T, E>(self, value: T, sensitive: bool) -> Self
    where
        T: Send + Sync + 'static,
        E: ScalarEncoding<T>,
    {
        self.update(move |mut parts| {
            if parts.route == RuntimeRouteMode::Off {
                return Err(ConfigurationErrorKind::InvalidRoute);
            }
            parts.reserve(1, 0, ConfigurationErrorKind::InvalidRoute)?;
            parts.path.push(RuntimePathSegment {
                atom: Box::new(PathSlot::<T, E>::new(value)),
                sensitive,
            });
            Ok(parts)
        })
    }

    fn route_mode(self, mode: RuntimeRouteMode) -> Self {
        self.update(move |mut parts| {
            if parts.route != RuntimeRouteMode::Inherit || !parts.path.is_empty() {
                return Err(ConfigurationErrorKind::InvalidRoute);
            }
            parts.reserve(1, 0, ConfigurationErrorKind::InvalidRoute)?;
            parts.route = mode;
            Ok(parts)
        })
    }

    /// Appends one runtime path atom as exactly one generated path segment
    /// after the owner's segments (or after the fixed base path following
    /// [`path_replace`](Self::path_replace)).
    ///
    /// The value is encoded once per plan/execution preparation, validated as
    /// one decoded segment (nonempty, not `.`/`..`, no `/` or backslash, no
    /// pre-encoded escape) and percent-encoded exactly once. A runtime path
    /// cannot name a host, scheme, port or absolute URL.
    pub fn path<T>(self, value: T) -> Self
    where
        T: RequestAtom + Send + Sync + 'static,
    {
        self.path_atom::<T, Canonical>(value, false)
    }

    /// [`path`](Self::path) with the explicit atom adapter `A`.
    pub fn path_via<T, A>(self, value: T) -> Self
    where
        T: Send + Sync + 'static,
        A: AtomEncoder<T> + 'static,
    {
        self.path_atom::<T, Via<A>>(value, false)
    }

    /// [`path`](Self::path) classified sensitive: the real request carries
    /// the segment; automatic observation replaces the whole segment.
    pub fn path_sensitive<T>(self, value: T) -> Self
    where
        T: RequestAtom + Send + Sync + 'static,
    {
        self.path_atom::<T, Canonical>(value, true)
    }

    /// Sensitive [`path_via`](Self::path_via).
    pub fn path_sensitive_via<T, A>(self, value: T) -> Self
    where
        T: Send + Sync + 'static,
        A: AtomEncoder<T> + 'static,
    {
        self.path_atom::<T, Via<A>>(value, true)
    }

    /// Drops the owner's generated path segments (retaining the fixed base
    /// path) before the following [`path`](Self::path) operations. Must
    /// precede every runtime path segment and appear at most once.
    pub fn path_replace(self) -> Self {
        self.route_mode(RuntimeRouteMode::Replace)
    }

    /// Drops the owner's generated path segments and appends nothing; the
    /// fixed base path remains. Must precede every runtime path segment; no
    /// later segment is accepted.
    pub fn path_off(self) -> Self {
        self.route_mode(RuntimeRouteMode::Off)
    }

    family_methods!(
        PatchFamily::Query,
        "query",
        query,
        query_via,
        query_sensitive,
        query_sensitive_via,
        query_optional,
        query_optional_via,
        query_optional_sensitive,
        query_optional_sensitive_via,
        query_each,
        query_each_via,
        query_each_sensitive,
        query_each_sensitive_via,
        query_each_optional,
        query_each_optional_via,
        query_each_optional_sensitive,
        query_each_optional_sensitive_via,
        query_replace,
        query_replace_via,
        query_replace_sensitive,
        query_replace_sensitive_via,
        query_replace_optional,
        query_replace_optional_via,
        query_replace_optional_sensitive,
        query_replace_optional_sensitive_via,
        query_replace_each,
        query_replace_each_via,
        query_replace_each_sensitive,
        query_replace_each_sensitive_via,
        query_replace_each_optional,
        query_replace_each_optional_via,
        query_replace_each_optional_sensitive,
        query_replace_each_optional_sensitive_via,
        query_when_present,
        query_when_present_via,
        query_when_present_sensitive,
        query_when_present_sensitive_via,
        query_when_present_each,
        query_when_present_each_via,
        query_when_present_each_sensitive,
        query_when_present_each_sensitive_via,
        query_append,
        query_append_via,
        query_append_sensitive,
        query_append_sensitive_via,
        query_append_optional,
        query_append_optional_via,
        query_append_optional_sensitive,
        query_append_optional_sensitive_via,
        query_append_each,
        query_append_each_via,
        query_append_each_sensitive,
        query_append_each_sensitive_via,
        query_append_each_optional,
        query_append_each_optional_via,
        query_append_each_optional_sensitive,
        query_append_each_optional_sensitive_via,
        query_remove,
        query_replace_all,
        query_off
    );

    family_methods!(
        PatchFamily::Header,
        "header",
        header,
        header_via,
        header_sensitive,
        header_sensitive_via,
        header_optional,
        header_optional_via,
        header_optional_sensitive,
        header_optional_sensitive_via,
        header_each,
        header_each_via,
        header_each_sensitive,
        header_each_sensitive_via,
        header_each_optional,
        header_each_optional_via,
        header_each_optional_sensitive,
        header_each_optional_sensitive_via,
        header_replace,
        header_replace_via,
        header_replace_sensitive,
        header_replace_sensitive_via,
        header_replace_optional,
        header_replace_optional_via,
        header_replace_optional_sensitive,
        header_replace_optional_sensitive_via,
        header_replace_each,
        header_replace_each_via,
        header_replace_each_sensitive,
        header_replace_each_sensitive_via,
        header_replace_each_optional,
        header_replace_each_optional_via,
        header_replace_each_optional_sensitive,
        header_replace_each_optional_sensitive_via,
        header_when_present,
        header_when_present_via,
        header_when_present_sensitive,
        header_when_present_sensitive_via,
        header_when_present_each,
        header_when_present_each_via,
        header_when_present_each_sensitive,
        header_when_present_each_sensitive_via,
        header_append,
        header_append_via,
        header_append_sensitive,
        header_append_sensitive_via,
        header_append_optional,
        header_append_optional_via,
        header_append_optional_sensitive,
        header_append_optional_sensitive_via,
        header_append_each,
        header_append_each_via,
        header_append_each_sensitive,
        header_append_each_sensitive_via,
        header_append_each_optional,
        header_append_each_optional_via,
        header_append_each_optional_sensitive,
        header_append_each_optional_sensitive_via,
        header_remove,
        header_replace_all,
        header_off
    );

    /// Selects the typed request body: `B` prepares `input` into the one
    /// managed prepared body at execution (never at construction, binding or
    /// planning). Omit it for a bodyless endpoint. At most one body.
    pub fn body_repr<B, T>(self, input: T) -> Self
    where
        B: RequestRepresentation<T>,
        T: Send + 'static,
    {
        self.update(move |mut parts| {
            if parts.body.is_some() {
                return Err(ConfigurationErrorKind::InvalidValue);
            }
            parts.body = Some((
                PreparedRequestBody::new::<B, T>(input),
                std::any::type_name::<B>(),
            ));
            Ok(parts)
        })
    }

    /// Removes every inherited authentication placement (RUNTIME-02).
    ///
    /// This changes the request's declared security behavior and is displayed
    /// prominently in the bound description and plan; it does not preserve
    /// the owner's authorization semantics. It cannot be combined with
    /// [`auth_remove`](Self::auth_remove) and appears at most once.
    pub fn auth_off(self) -> Self {
        self.update(|mut parts| {
            if parts.auth == AuthMode::Off || !parts.auth_removals.is_empty() {
                return Err(ConfigurationErrorKind::InvalidAuthentication);
            }
            parts.reserve(1, 0, ConfigurationErrorKind::InvalidAuthentication)?;
            parts.auth = AuthMode::Off;
            Ok(parts)
        })
    }

    /// Removes the one inherited placement owning `target` (its safe
    /// descriptor identity). Binding fails when no effective owner placement
    /// owns the target. No runtime API can add a placement, select another
    /// credential source, change a challenge or reintroduce a removal.
    pub fn auth_remove(self, target: AuthTarget) -> Self {
        self.update(move |mut parts| {
            if parts.auth == AuthMode::Off || parts.auth_removals.contains(&target) {
                return Err(ConfigurationErrorKind::InvalidAuthentication);
            }
            parts.reserve(
                1,
                target.retained_bytes(),
                ConfigurationErrorKind::InvalidAuthentication,
            )?;
            parts.auth_removals.push(target);
            Ok(parts)
        })
    }

    /// Attaches one explicitly identified, explicitly labelled rate policy.
    ///
    /// Runtime policies are additive: every inherited owner policy remains in
    /// force and no runtime operation can remove one. The duplicate rules of
    /// generated attachments apply (§12.2): an exact repetition (same ID,
    /// cost, canonical windows and ordered dimensions) is deduplicated and
    /// any other repetition of an ID is rejected, here among runtime policies
    /// and at bind against the owner's policies. A client built without a
    /// limiter selection rejects a runtime policy at bind. The built-in
    /// `origin`/`endpoint` dimensions resolve from the bound head.
    pub fn rate_limit(self, declaration: RateLimitDeclaration) -> Self {
        self.update(move |mut parts| {
            let retained = declaration
                .retained_bytes()
                .map_err(|_| ConfigurationErrorKind::InvalidRateLimit)?;
            let before = parts.rate_limits.len();
            crate::rate_limit::push_declaration(&mut parts.rate_limits, declaration)
                .map_err(|_| ConfigurationErrorKind::InvalidRateLimit)?;
            if parts.rate_limits.len() > before {
                if let Err(kind) =
                    parts.reserve(1, retained, ConfigurationErrorKind::InvalidRateLimit)
                {
                    parts.rate_limits.pop();
                    return Err(kind);
                }
            }
            Ok(parts)
        })
    }

    /// Selects this endpoint's finite operation timeout. Without it the
    /// binding owner's effective timeout applies. The duration must be
    /// positive and representable; the selection is made at most once.
    pub fn timeout(self, duration: Duration) -> Self {
        self.update(move |mut parts| {
            if parts.timeout.is_some() {
                return Err(ConfigurationErrorKind::InvalidValue);
            }
            crate::controls::validate_positive_duration(duration)
                .map_err(|_| ConfigurationErrorKind::InvalidValue)?;
            parts.timeout = Some(duration);
            Ok(parts)
        })
    }

    /// Asserts the endpoint's application effect explicitly (otherwise the
    /// method default applies). At most once.
    pub fn effect(self, effect: Effect) -> Self {
        self.update(move |mut parts| {
            if parts.effect.is_some() {
                return Err(ConfigurationErrorKind::InvalidValue);
            }
            parts.effect = Some(effect);
            Ok(parts)
        })
    }

    /// Selects the accepted final status set. Every explicit code must be a
    /// final status in `200..=599`; binding rejects any overlap with an
    /// effective authentication challenge. At most once.
    pub fn status(self, accepted: AcceptedStatus) -> Self {
        self.update(move |mut parts| {
            if parts.status.is_some() {
                return Err(ConfigurationErrorKind::InvalidValue);
            }
            if accepted
                .explicit_codes()
                .is_some_and(|codes| codes.iter().any(|code| !final_status(*code)))
            {
                return Err(ConfigurationErrorKind::InvalidValue);
            }
            parts.status = Some(accepted);
            Ok(parts)
        })
    }

    /// Selects an explicit nonempty, duplicate-free set of final accepted
    /// statuses (`200..=599`). The iterator is read only while the builder
    /// is valid and at most one entry past the largest possible set.
    pub fn status_codes<I>(self, codes: I) -> Self
    where
        I: IntoIterator<Item = StatusCode>,
    {
        self.update(move |mut parts| {
            if parts.status.is_some() {
                return Err(ConfigurationErrorKind::InvalidValue);
            }
            let codes = codes
                .into_iter()
                .take(MAX_STATUS_CODES + 1)
                .collect::<Vec<_>>();
            let mut sorted = codes.clone();
            sorted.sort_unstable();
            sorted.dedup();
            if codes.is_empty()
                || codes.len() > MAX_STATUS_CODES
                || sorted.len() != codes.len()
                || codes.iter().any(|code| !final_status(*code))
            {
                return Err(ConfigurationErrorKind::InvalidValue);
            }
            parts.status = Some(
                AcceptedStatus::codes(codes).map_err(|_| ConfigurationErrorKind::InvalidValue)?,
            );
            Ok(parts)
        })
    }
}

const fn final_status(code: StatusCode) -> bool {
    code.as_u16() >= 200 && code.as_u16() <= 599
}

impl RuntimeEndpointBuilder<(), ()> {
    /// Selects the typed response pair: `R` converts accepted responses into
    /// `O`. Exactly one selection per endpoint; no representation is guessed
    /// from the output type.
    pub fn response<R, O>(self) -> RuntimeEndpointBuilder<R, O>
    where
        R: ResponseRepresentation<O>,
        O: Send + 'static,
    {
        RuntimeEndpointBuilder {
            state: self.state,
            response: PhantomData,
        }
    }
}

impl<R, O> RuntimeEndpointBuilder<R, O>
where
    R: ResponseRepresentation<O>,
    O: Send + 'static,
    R::Driver: AcceptedBytesDriver,
{
    /// Explicitly exposes the alternate accepted-bytes terminal on requests
    /// bound from this descriptor (§10.4). Only buffered and discard response
    /// drivers qualify; the terminal keeps origin, authentication, rate,
    /// status and deadline rules and bypasses only representation-specific
    /// validation and conversion.
    pub fn expose_accepted_bytes(self) -> Self {
        self.update(|mut parts| {
            if parts.accepted_bytes {
                return Err(ConfigurationErrorKind::InvalidValue);
            }
            parts.accepted_bytes = true;
            Ok(parts)
        })
    }
}

impl<R, O> RuntimeEndpointBuilder<R, O>
where
    R: ResponseRepresentation<O>,
    O: Send + 'static,
{
    /// Validates the representation's static `Accept` constant and returns
    /// the immutable descriptor, or the first latched structural error.
    ///
    /// # Errors
    ///
    /// The first latched [`ConfigurationError`]; its category is safe and it
    /// never contains keys' values, rate-key bytes or credential material.
    pub fn build(self) -> Result<RuntimeEndpoint<R, O>, ConfigurationError> {
        if R::ACCEPT.is_some_and(|accept| !crate::response::accept_is_valid(accept)) {
            return Err(ConfigurationError::new(
                ConfigurationErrorKind::InvalidHeader,
            ));
        }
        let Parts {
            id,
            method,
            route,
            path,
            ops,
            op_facts,
            table,
            query,
            header,
            auth,
            auth_removals,
            rate_limits,
            body,
            timeout,
            status,
            effect,
            accepted_bytes,
            budget: _,
        } = self.state.into_result().map_err(ConfigurationError::new)?;

        let (body, body_category, body_specification, body_description) = match body {
            Some((body, specification)) => {
                let description = body.description();
                (
                    body,
                    RequestBodyCategory::Specification,
                    Some(specification),
                    description,
                )
            }
            None => (
                PreparedRequestBody::new::<EmptyBody, ()>(()),
                RequestBodyCategory::Empty,
                None,
                BodyDescription::EMPTY,
            ),
        };
        let effect_source = if effect.is_some() {
            EffectSource::Explicit
        } else {
            EffectSource::MethodDefault
        };
        let effect = effect.unwrap_or_else(|| Effect::for_method(&method));
        let status_source = if status.is_some() {
            StatusSource::Explicit
        } else {
            StatusSource::DefaultSuccess
        };
        let authentication = match auth {
            AuthMode::Off => RuntimeAuthDisposition::RemovedAll,
            AuthMode::Inherit if auth_removals.is_empty() => RuntimeAuthDisposition::Inherited,
            AuthMode::Inherit => RuntimeAuthDisposition::Removed(auth_removals.clone().into()),
        };
        let facts = RuntimeBindingFacts {
            owner: None,
            route,
            path_segments: path.len(),
            sensitive_path_segments: path.iter().filter(|segment| segment.sensitive).count(),
            query: query.mode,
            headers: header.mode,
            operations: op_facts.into(),
            authentication,
            inherited_auth_placements: 0,
            effective_auth_placements: 0,
            rate_policies: rate_limits
                .iter()
                .filter_map(|declaration| declaration.id().map(|id| Arc::from(id.as_str())))
                .collect(),
            effective_rate_policies: 0,
            accepted_bytes,
        };
        let description = EndpointDescription::runtime(
            id.clone(),
            method.clone(),
            Arc::from("runtime"),
            route_shape(None, route, path.len()),
            body_category,
            body_specification,
            <R::Driver as ResponseDriver>::ACQUISITION,
            EndpointCapabilities::generated(
                false,
                accepted_bytes,
                false,
                false,
                !rate_limits.is_empty(),
            ),
            timeout.map_or(TimeoutSelection::InheritClient, TimeoutSelection::Explicit),
            status.clone().unwrap_or_default(),
            status_source,
            effect,
            effect_source,
        )
        .with_body_description(body_description)
        .with_response_accept(R::ACCEPT)
        .with_runtime_facts(facts.clone());

        Ok(RuntimeEndpoint {
            program: Arc::new(RuntimeProgram {
                id,
                method,
                route,
                path,
                ops,
                table,
                auth,
                auth_removals,
                rate_limits,
                status,
                accepted_bytes,
                description,
                facts,
            }),
            body,
            response: PhantomData,
        })
    }
}

/// Value-free structural route shape of a runtime description.
fn route_shape(owner: Option<&str>, route: RuntimeRouteMode, segments: usize) -> Arc<str> {
    let mut shape = match (route, owner) {
        (RuntimeRouteMode::Inherit, None) => String::from("<owner-route>"),
        (RuntimeRouteMode::Inherit, Some(owner)) => format!("<{owner} route>"),
        (RuntimeRouteMode::Replace | RuntimeRouteMode::Off, _) => String::from("<client-base>"),
    };
    for _ in 0..segments {
        shape.push_str("/<runtime-segment>");
    }
    Arc::from(shape)
}
