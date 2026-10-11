//! Window sets, rate policies, effective policies, coverage and sharing (§12).

use std::time::Duration;

use crate::rate_limit::RateDimensionKind;

use super::values::ValueRef;

/// One positive rate window: `allow capacity per duration`.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct WindowDescription {
    capacity: u32,
    duration: Duration,
}

impl WindowDescription {
    /// `allow capacity per duration`.
    #[must_use]
    pub const fn new(capacity: u32, duration: Duration) -> Self {
        Self { capacity, duration }
    }

    /// Returns the positive capacity.
    #[must_use]
    pub const fn capacity(&self) -> u32 {
        self.capacity
    }

    /// Returns the positive duration.
    #[must_use]
    pub const fn duration(&self) -> Duration {
        self.duration
    }

    pub(crate) fn canonical_key(&self) -> (Duration, u32) {
        (self.duration, self.capacity)
    }
}

/// A reusable numeric window set. It grants no identity and shares no state.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct WindowSetDescription {
    pub(crate) name: &'static str,
    pub(crate) windows: Box<[WindowDescription]>,
    pub(crate) docs: &'static [&'static str],
    pub(crate) used_by: Box<[&'static str]>,
}

impl WindowSetDescription {
    /// `windows name { ... }` with windows in source order.
    #[must_use]
    pub fn new(name: &'static str, windows: Vec<WindowDescription>) -> Self {
        Self {
            name,
            windows: windows.into_boxed_slice(),
            docs: &[],
            used_by: Box::new([]),
        }
    }

    /// Records author documentation lines.
    #[must_use]
    pub fn docs(mut self, docs: &'static [&'static str]) -> Self {
        self.docs = docs;
        self
    }

    /// Returns the set name.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        self.name
    }

    /// Returns windows in source order.
    #[must_use]
    pub fn windows(&self) -> &[WindowDescription] {
        &self.windows
    }

    /// Returns windows in canonical order (duration, then capacity).
    #[must_use]
    pub fn canonical_windows(&self) -> Vec<WindowDescription> {
        canonical_windows(&self.windows)
    }

    /// Returns author documentation lines.
    #[must_use]
    pub const fn doc_lines(&self) -> &'static [&'static str] {
        self.docs
    }

    /// Returns the policy IDs whose effective windows come from this set, in
    /// sorted order. Empty means the set is unused.
    #[must_use]
    pub fn used_by(&self) -> &[&'static str] {
        &self.used_by
    }
}

/// One formal dimension of a named policy.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum FormalDimension {
    /// The built-in normalized origin.
    Origin,
    /// The built-in operational endpoint ID.
    Endpoint,
    /// `"label" = formal [via Encoder]`.
    Custom {
        /// The explicit state-affecting label.
        label: &'static str,
        /// The policy formal name.
        formal: &'static str,
        /// The `RateKeyEncoder` Rust type spelling, when written.
        encoder: Option<&'static str>,
    },
}

/// One named policy in the client `rate_limits` registry.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct PolicyDefinition {
    pub(crate) name: &'static str,
    pub(crate) id: &'static str,
    pub(crate) formals: Vec<&'static str>,
    pub(crate) dimensions: Vec<FormalDimension>,
    pub(crate) cost: Option<u32>,
    pub(crate) windows: Vec<WindowDescription>,
    pub(crate) window_set: Option<&'static str>,
    pub(crate) docs: &'static [&'static str],
}

impl PolicyDefinition {
    /// `policy name(...) { id "id"; ... }` with no formals, dimensions or
    /// windows yet.
    #[must_use]
    pub fn new(name: &'static str, id: &'static str) -> Self {
        Self {
            name,
            id,
            formals: Vec::new(),
            dimensions: Vec::new(),
            cost: None,
            windows: Vec::new(),
            window_set: None,
            docs: &[],
        }
    }

    /// Appends one formal in declaration order.
    #[must_use]
    pub fn formal(mut self, formal: &'static str) -> Self {
        self.formals.push(formal);
        self
    }

    /// Appends the `origin` dimension.
    #[must_use]
    pub fn origin(mut self) -> Self {
        self.dimensions.push(FormalDimension::Origin);
        self
    }

    /// Appends the `endpoint` dimension.
    #[must_use]
    pub fn endpoint(mut self) -> Self {
        self.dimensions.push(FormalDimension::Endpoint);
        self
    }

    /// Appends `"label" = formal`.
    #[must_use]
    pub fn custom(mut self, label: &'static str, formal: &'static str) -> Self {
        self.dimensions.push(FormalDimension::Custom {
            label,
            formal,
            encoder: None,
        });
        self
    }

    /// Appends `"label" = formal via Encoder`.
    #[must_use]
    pub fn custom_via(
        mut self,
        label: &'static str,
        formal: &'static str,
        encoder: &'static str,
    ) -> Self {
        self.dimensions.push(FormalDimension::Custom {
            label,
            formal,
            encoder: Some(encoder),
        });
        self
    }

    /// Records `cost n;`.
    #[must_use]
    pub fn cost(mut self, cost: u32) -> Self {
        self.cost = Some(cost);
        self
    }

    /// Appends one inline window. Writing both inline windows and `using`
    /// is rejected when the contract is built.
    #[must_use]
    pub fn window(mut self, capacity: u32, duration: Duration) -> Self {
        self.windows
            .push(WindowDescription::new(capacity, duration));
        self
    }

    /// Records `using set_name;`.
    #[must_use]
    pub fn using(mut self, window_set: &'static str) -> Self {
        self.window_set = Some(window_set);
        self
    }

    /// Records author documentation lines.
    #[must_use]
    pub fn docs(mut self, docs: &'static [&'static str]) -> Self {
        self.docs = docs;
        self
    }

    /// Returns the policy name.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        self.name
    }

    /// Returns the explicit policy ID.
    #[must_use]
    pub const fn id(&self) -> &'static str {
        self.id
    }

    /// Returns formals in declaration order.
    #[must_use]
    pub fn formals(&self) -> &[&'static str] {
        &self.formals
    }

    /// Returns formal dimensions in key order.
    #[must_use]
    pub fn dimensions(&self) -> &[FormalDimension] {
        &self.dimensions
    }

    /// Returns the declared cost, when written.
    #[must_use]
    pub const fn declared_cost(&self) -> Option<u32> {
        self.cost
    }

    /// Returns inline windows in source order (empty when `using` a set).
    #[must_use]
    pub fn inline_windows(&self) -> &[WindowDescription] {
        &self.windows
    }

    /// Returns the `using` window set name, when written.
    #[must_use]
    pub const fn window_set(&self) -> Option<&'static str> {
        self.window_set
    }

    /// Returns author documentation lines.
    #[must_use]
    pub const fn doc_lines(&self) -> &'static [&'static str] {
        self.docs
    }
}

/// One exact named binding `formal = reference` of a policy attachment.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct RateBinding {
    pub(crate) formal: &'static str,
    pub(crate) value: ValueRef,
    pub(crate) encoding_id: Option<&'static str>,
}

impl RateBinding {
    /// Returns the formal name.
    #[must_use]
    pub const fn formal(&self) -> &'static str {
        self.formal
    }

    /// Returns the bound value.
    #[must_use]
    pub const fn value(&self) -> ValueRef {
        self.value
    }

    /// Returns the canonical key `ENCODING_ID` the generated code read from
    /// the bound type or encoder, when supplied.
    #[must_use]
    pub const fn encoding_id(&self) -> Option<&'static str> {
        self.encoding_id
    }
}

/// One dimension of an inline policy attachment.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum BoundDimension {
    /// The built-in normalized origin.
    Origin,
    /// The built-in operational endpoint ID.
    Endpoint,
    /// `"label" = reference [via Encoder]`.
    Custom {
        /// The explicit state-affecting label.
        label: &'static str,
        /// The bound value.
        value: ValueRef,
        /// The `RateKeyEncoder` Rust type spelling, when written.
        encoder: Option<&'static str>,
        /// The canonical key `ENCODING_ID` read by generated code, when
        /// supplied.
        encoding_id: Option<&'static str>,
    },
}

/// The source of one rate attachment.
#[non_exhaustive]
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum RateAttachmentSource {
    /// `rate_limit policy(formal = reference, ...)`.
    Named {
        /// The named policy.
        policy: &'static str,
        /// Exact, total named bindings in written order.
        bindings: Box<[RateBinding]>,
    },
    /// `rate_limit { id "..."; key [...]; ... }` or the `using` shorthand.
    ///
    /// In a built description exactly one of `windows` (nonempty) and
    /// `window_set` is present.
    Inline {
        /// The explicit policy ID.
        id: &'static str,
        /// Bound dimensions in key order.
        dimensions: Box<[BoundDimension]>,
        /// Inline windows in source order.
        windows: Box<[WindowDescription]>,
        /// The `using` window set, when written.
        window_set: Option<&'static str>,
    },
}

/// One owner's rate attachment as written.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct RateAttachment {
    pub(crate) source: RateAttachmentSource,
    pub(crate) cost: Option<u32>,
}

impl RateAttachment {
    /// `rate_limit policy(...)` with no bindings yet.
    #[must_use]
    pub fn named(policy: &'static str) -> Self {
        Self {
            source: RateAttachmentSource::Named {
                policy,
                bindings: Box::new([]),
            },
            cost: None,
        }
    }

    /// An inline `rate_limit { id "id"; ... }` with no dimensions yet.
    #[must_use]
    pub fn inline(id: &'static str) -> Self {
        Self {
            source: RateAttachmentSource::Inline {
                id,
                dimensions: Box::new([]),
                windows: Box::new([]),
                window_set: None,
            },
            cost: None,
        }
    }

    /// Appends `formal = reference` to a named attachment (ignored on an
    /// inline attachment, which the build then rejects as unbound).
    #[must_use]
    pub fn bind(self, formal: &'static str, value: ValueRef) -> Self {
        self.push_binding(RateBinding {
            formal,
            value,
            encoding_id: None,
        })
    }

    /// Appends `formal = reference` together with the canonical key
    /// `ENCODING_ID` generated code read from the bound type or encoder.
    #[must_use]
    pub fn bind_encoded(
        self,
        formal: &'static str,
        value: ValueRef,
        encoding_id: &'static str,
    ) -> Self {
        self.push_binding(RateBinding {
            formal,
            value,
            encoding_id: Some(encoding_id),
        })
    }

    fn push_binding(mut self, binding: RateBinding) -> Self {
        if let RateAttachmentSource::Named { bindings, .. } = &mut self.source {
            let mut next = std::mem::take(bindings).into_vec();
            next.push(binding);
            *bindings = next.into_boxed_slice();
        }
        self
    }

    /// Appends the `origin` dimension to an inline attachment.
    #[must_use]
    pub fn origin(self) -> Self {
        self.push_dimension(BoundDimension::Origin)
    }

    /// Appends the `endpoint` dimension to an inline attachment.
    #[must_use]
    pub fn endpoint(self) -> Self {
        self.push_dimension(BoundDimension::Endpoint)
    }

    /// Appends `"label" = reference` to an inline attachment.
    #[must_use]
    pub fn custom(self, label: &'static str, value: ValueRef) -> Self {
        self.push_dimension(BoundDimension::Custom {
            label,
            value,
            encoder: None,
            encoding_id: None,
        })
    }

    /// Appends `"label" = reference [via Encoder]` with the canonical key
    /// `ENCODING_ID` generated code read, when supplied.
    #[must_use]
    pub fn custom_encoded(
        self,
        label: &'static str,
        value: ValueRef,
        encoder: Option<&'static str>,
        encoding_id: Option<&'static str>,
    ) -> Self {
        self.push_dimension(BoundDimension::Custom {
            label,
            value,
            encoder,
            encoding_id,
        })
    }

    fn push_dimension(mut self, dimension: BoundDimension) -> Self {
        if let RateAttachmentSource::Inline { dimensions, .. } = &mut self.source {
            let mut next = std::mem::take(dimensions).into_vec();
            next.push(dimension);
            *dimensions = next.into_boxed_slice();
        }
        self
    }

    /// Appends one inline window to an inline attachment.
    #[must_use]
    pub fn window(mut self, capacity: u32, duration: Duration) -> Self {
        if let RateAttachmentSource::Inline { windows, .. } = &mut self.source {
            let mut next = std::mem::take(windows).into_vec();
            next.push(WindowDescription::new(capacity, duration));
            *windows = next.into_boxed_slice();
        }
        self
    }

    /// Records `using set_name` on an inline attachment.
    #[must_use]
    pub fn using(mut self, set: &'static str) -> Self {
        if let RateAttachmentSource::Inline { window_set, .. } = &mut self.source {
            *window_set = Some(set);
        }
        self
    }

    /// Records the attachment's `cost n`.
    #[must_use]
    pub fn cost(mut self, cost: u32) -> Self {
        self.cost = Some(cost);
        self
    }

    /// Returns the attachment source.
    #[must_use]
    pub const fn source(&self) -> &RateAttachmentSource {
        &self.source
    }

    /// Returns the attachment-level cost, when written.
    #[must_use]
    pub const fn declared_cost(&self) -> Option<u32> {
        self.cost
    }
}

pub(crate) fn canonical_windows(windows: &[WindowDescription]) -> Vec<WindowDescription> {
    let mut canonical = windows.to_vec();
    canonical.sort_by_key(WindowDescription::canonical_key);
    canonical
}

/// How a custom dimension's canonical encoding is selected.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum EncoderRole {
    /// Built-in `origin`/`endpoint` UTF-8 framing.
    BuiltinUtf8,
    /// The bound value type's `RateKeyPart` implementation.
    ValueType,
    /// An explicit `via` `RateKeyEncoder`.
    Encoder,
}

impl EncoderRole {
    /// Returns the stable lowercase label used by structural exports.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::BuiltinUtf8 => "builtin_utf8",
            Self::ValueType => "value_type",
            Self::Encoder => "encoder",
        }
    }
}

/// One dimension of one effective policy.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct EffectiveDimension {
    pub(crate) label: &'static str,
    pub(crate) kind: RateDimensionKind,
    pub(crate) binding: Option<ValueRef>,
    pub(crate) encoder_role: EncoderRole,
    pub(crate) encoder_type: Option<&'static str>,
    pub(crate) encoding_id: Option<&'static str>,
}

impl EffectiveDimension {
    /// Returns the state-affecting label.
    #[must_use]
    pub const fn label(&self) -> &'static str {
        self.label
    }

    /// Returns the dimension kind.
    #[must_use]
    pub const fn kind(&self) -> RateDimensionKind {
        self.kind
    }

    /// Returns the bound value of a custom dimension.
    #[must_use]
    pub const fn binding(&self) -> Option<ValueRef> {
        self.binding
    }

    /// Returns how the canonical encoding is selected.
    #[must_use]
    pub const fn encoder_role(&self) -> EncoderRole {
        self.encoder_role
    }

    /// Returns the encoding identity as a Rust type spelling: the `via`
    /// encoder or the bound value's payload type. `None` for built-ins.
    #[must_use]
    pub const fn encoder_type(&self) -> Option<&'static str> {
        self.encoder_type
    }

    /// Returns the canonical `ENCODING_ID` when known statically: always for
    /// built-ins, and for custom dimensions when generated code supplied it.
    #[must_use]
    pub const fn encoding_id(&self) -> Option<&'static str> {
        self.encoding_id
    }
}

/// Which declaration defined an effective policy.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum PolicyOrigin {
    /// A named registry policy.
    Named(&'static str),
    /// An inline attachment.
    Inline,
}

/// Where an effective cost came from.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum CostSource {
    /// No cost was written; the default is one.
    Default,
    /// The named policy definition's `cost`.
    Definition,
    /// The attachment's `cost`.
    Attachment,
}

impl CostSource {
    /// Returns the stable lowercase label used by structural exports.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Definition => "definition",
            Self::Attachment => "attachment",
        }
    }
}

/// One effective, deduplicated policy of one endpoint.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct EffectivePolicy {
    pub(crate) id: &'static str,
    pub(crate) definition: PolicyOrigin,
    pub(crate) origin: &'static str,
    pub(crate) dimensions: Box<[EffectiveDimension]>,
    pub(crate) cost: u32,
    pub(crate) cost_source: CostSource,
    pub(crate) windows: Box<[WindowDescription]>,
    pub(crate) window_set: Option<&'static str>,
}

impl EffectivePolicy {
    /// Returns the explicit policy ID.
    #[must_use]
    pub const fn id(&self) -> &'static str {
        self.id
    }

    /// Returns the defining declaration.
    #[must_use]
    pub const fn definition(&self) -> PolicyOrigin {
        self.definition
    }

    /// Returns the owner source path whose attachment introduced the policy.
    #[must_use]
    pub const fn origin(&self) -> &'static str {
        self.origin
    }

    /// Returns ordered key dimensions.
    #[must_use]
    pub fn dimensions(&self) -> &[EffectiveDimension] {
        &self.dimensions
    }

    /// Returns the positive per-admission cost.
    #[must_use]
    pub const fn cost(&self) -> u32 {
        self.cost
    }

    /// Returns where the cost came from.
    #[must_use]
    pub const fn cost_source(&self) -> CostSource {
        self.cost_source
    }

    /// Returns windows in source order (provenance).
    #[must_use]
    pub fn windows(&self) -> &[WindowDescription] {
        &self.windows
    }

    /// Returns windows in canonical schema order (duration, then capacity).
    #[must_use]
    pub fn canonical_windows(&self) -> Vec<WindowDescription> {
        canonical_windows(&self.windows)
    }

    /// Returns the named window set supplying the windows, when used.
    #[must_use]
    pub const fn window_set(&self) -> Option<&'static str> {
        self.window_set
    }

    /// Returns whether the key includes the `endpoint` dimension, which makes
    /// every covered endpoint its own bucket family.
    #[must_use]
    pub fn uses_endpoint(&self) -> bool {
        self.dimensions
            .iter()
            .any(|dimension| dimension.kind == RateDimensionKind::Endpoint)
    }

    /// Returns whether the key includes the `origin` dimension.
    #[must_use]
    pub fn uses_origin(&self) -> bool {
        self.dimensions
            .iter()
            .any(|dimension| dimension.kind == RateDimensionKind::Origin)
    }
}

/// One endpoint covered by a policy identity.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct CoveredEndpoint {
    pub(crate) id: crate::endpoint::EndpointId,
    pub(crate) path: &'static str,
    pub(crate) cost: u32,
    pub(crate) origin: &'static str,
}

impl CoveredEndpoint {
    /// Returns the operational endpoint ID.
    #[must_use]
    pub fn id(&self) -> &str {
        self.id.as_str()
    }

    /// Returns the endpoint source path.
    #[must_use]
    pub const fn path(&self) -> &'static str {
        self.path
    }

    /// Returns the endpoint's effective cost for this policy.
    #[must_use]
    pub const fn cost(&self) -> u32 {
        self.cost
    }

    /// Returns the owner whose attachment covers the endpoint.
    #[must_use]
    pub const fn attached_at(&self) -> &'static str {
        self.origin
    }
}

/// The binding of one custom label for one covered endpoint.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct LabelBinding {
    pub(crate) endpoint: &'static str,
    pub(crate) value: ValueRef,
}

impl LabelBinding {
    /// Returns the endpoint source path.
    #[must_use]
    pub const fn endpoint(&self) -> &'static str {
        self.endpoint
    }

    /// Returns the value whose canonical bytes fill the label.
    #[must_use]
    pub const fn value(&self) -> ValueRef {
        self.value
    }
}

/// The per-label part of a symbolic sharing condition.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct LabelCondition {
    pub(crate) label: &'static str,
    pub(crate) bindings: Box<[LabelBinding]>,
}

impl LabelCondition {
    /// Returns the custom label.
    #[must_use]
    pub const fn label(&self) -> &'static str {
        self.label
    }

    /// Returns each covered endpoint's binding for the label.
    #[must_use]
    pub fn bindings(&self) -> &[LabelBinding] {
        &self.bindings
    }
}

/// The symbolic condition under which endpoints of one group share a bucket
/// (RATE-02, RATE-03). It never asserts that arbitrary runtime values share.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct SharingCondition {
    pub(crate) same_origin: bool,
    pub(crate) same_endpoint: bool,
    pub(crate) labels: Box<[LabelCondition]>,
}

impl SharingCondition {
    /// Always true: separate limiter objects never share counters.
    #[must_use]
    pub const fn same_limiter(&self) -> bool {
        true
    }

    /// Always true: a bucket belongs to exactly one policy ID.
    #[must_use]
    pub const fn same_policy_id(&self) -> bool {
        true
    }

    /// Returns whether the normalized logical origin must be equal.
    #[must_use]
    pub const fn same_origin(&self) -> bool {
        self.same_origin
    }

    /// Returns whether the operational endpoint ID must be equal.
    #[must_use]
    pub const fn same_endpoint(&self) -> bool {
        self.same_endpoint
    }

    /// Returns labels whose canonical values must be equal, with bindings.
    #[must_use]
    pub fn labels(&self) -> &[LabelCondition] {
        &self.labels
    }
}

/// A set of endpoints that can share buckets of one policy identity.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct SharingGroup {
    pub(crate) endpoints: Box<[&'static str]>,
    pub(crate) condition: SharingCondition,
}

impl SharingGroup {
    /// Returns member endpoint source paths in source order.
    #[must_use]
    pub fn endpoints(&self) -> &[&'static str] {
        &self.endpoints
    }

    /// Returns the symbolic sharing condition.
    #[must_use]
    pub const fn condition(&self) -> &SharingCondition {
        &self.condition
    }
}

/// One attachment site contributing a policy identity.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct PolicyAttachmentSite {
    pub(crate) owner: &'static str,
    pub(crate) attachment: RateAttachment,
}

impl PolicyAttachmentSite {
    /// Returns the attaching owner source path.
    #[must_use]
    pub const fn owner(&self) -> &'static str {
        self.owner
    }

    /// Returns the resolved attachment as written.
    #[must_use]
    pub const fn attachment(&self) -> &RateAttachment {
        &self.attachment
    }
}

/// The coverage and sharing view of one policy identity (RATE-02).
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct PolicyCoverage {
    pub(crate) id: &'static str,
    pub(crate) definition: PolicyOrigin,
    pub(crate) dimensions: Box<[EffectiveDimension]>,
    pub(crate) windows: Box<[WindowDescription]>,
    pub(crate) window_set: Option<&'static str>,
    pub(crate) attachments: Box<[PolicyAttachmentSite]>,
    pub(crate) covered: Box<[CoveredEndpoint]>,
    pub(crate) sharing: Box<[SharingGroup]>,
}

impl PolicyCoverage {
    /// Returns the policy ID.
    #[must_use]
    pub const fn id(&self) -> &'static str {
        self.id
    }

    /// Returns the defining declaration.
    #[must_use]
    pub const fn definition(&self) -> PolicyOrigin {
        self.definition
    }

    /// Returns the schema dimensions. For an unused named policy, custom
    /// dimensions carry no binding.
    #[must_use]
    pub fn dimensions(&self) -> &[EffectiveDimension] {
        &self.dimensions
    }

    /// Returns schema windows in source order.
    #[must_use]
    pub fn windows(&self) -> &[WindowDescription] {
        &self.windows
    }

    /// Returns schema windows in canonical order.
    #[must_use]
    pub fn canonical_windows(&self) -> Vec<WindowDescription> {
        canonical_windows(&self.windows)
    }

    /// Returns the window set name, when used.
    #[must_use]
    pub const fn window_set(&self) -> Option<&'static str> {
        self.window_set
    }

    /// Returns attachment sites in owner source order.
    #[must_use]
    pub fn attachments(&self) -> &[PolicyAttachmentSite] {
        &self.attachments
    }

    /// Returns every covered endpoint in source order.
    #[must_use]
    pub fn covered(&self) -> &[CoveredEndpoint] {
        &self.covered
    }

    /// Returns the possible sharing groups.
    #[must_use]
    pub fn sharing(&self) -> &[SharingGroup] {
        &self.sharing
    }

    /// Returns whether no endpoint is covered (an unused named policy).
    #[must_use]
    pub fn is_unused(&self) -> bool {
        self.covered.is_empty()
    }
}
