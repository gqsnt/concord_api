//! Immutable, value-free contract reflection.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::sync::Arc;
use std::time::Duration;

use crate::endpoint::{EndpointDescription, TimeoutSelection};

/// The kind of declaration that owns a reflected fact.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum DeclarationKind {
    /// The generated client root.
    Client,
    /// A lexical scope.
    Scope,
    /// An endpoint leaf.
    Endpoint,
}

/// Where a reflected fact obtains its precision.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ContractFactClass {
    /// The fact is statically declared by the Concord contract.
    StaticDeclaration,
    /// The fact is supplied by a trait implementation at initialization.
    TraitCapability,
    /// The behavior belongs to trusted extension code and is intentionally opaque.
    OpaqueExtension,
    /// The fact is selected only when a client or runtime endpoint is bound.
    RuntimeBound,
}

/// One declaration's documentation and lifecycle metadata.
#[non_exhaustive]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeclarationDescription {
    kind: DeclarationKind,
    name: &'static str,
    path: &'static str,
    docs: Box<[&'static str]>,
    deprecated: Option<&'static str>,
}

impl DeclarationDescription {
    /// Constructs generated declaration metadata.
    #[doc(hidden)]
    #[must_use]
    pub fn generated(
        kind: DeclarationKind,
        name: &'static str,
        path: &'static str,
        docs: Vec<&'static str>,
        deprecated: Option<&'static str>,
    ) -> Self {
        Self {
            kind,
            name,
            path,
            docs: docs.into_boxed_slice(),
            deprecated,
        }
    }

    /// Returns the declaration kind.
    #[must_use]
    pub const fn kind(&self) -> DeclarationKind {
        self.kind
    }

    /// Returns the source name.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        self.name
    }

    /// Returns the stable lexical declaration path.
    #[must_use]
    pub const fn path(&self) -> &'static str {
        self.path
    }

    /// Returns author documentation in source order.
    #[must_use]
    pub fn docs(&self) -> &[&'static str] {
        &self.docs
    }

    /// Returns the deprecation note, when declared.
    #[must_use]
    pub const fn deprecated(&self) -> Option<&'static str> {
        self.deprecated
    }
}

/// Which lexical value owner is named by reflection.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ValueOwnerKind {
    /// Client snapshot storage.
    Client,
    /// One exact named scope snapshot.
    Scope,
    /// One endpoint intent/request snapshot.
    Endpoint,
}

/// Concord-owned outer presence, independent of Rust type spelling.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ValuePresence {
    /// The declaration adds no outer presence layer.
    Required,
    /// The declaration adds exactly one outer `Option` layer.
    Optional,
}

/// The declaration initializer category.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ValueInitializer {
    /// No declaration initializer exists.
    None,
    /// The payload's `Default` implementation is evaluated at its owner boundary.
    TraitDefault,
    /// A bounded stable literal or const/static path is evaluated at its owner boundary.
    StableValue,
}

/// The observation authority associated with a declared value.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ObservationClass {
    /// The value may appear in credential-free public observation.
    OrdinaryPublic,
    /// The real public value is used internally but observations contain only a marker.
    RedactedPublic,
    /// Secret or credential material is structurally absent from ordinary observation.
    PrivateSecretOrCredential,
}

/// Value semantics shared by generated code, Rustdoc, and contract reflection.
#[non_exhaustive]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValueDescription {
    owner: ValueOwnerKind,
    owner_path: &'static str,
    name: &'static str,
    payload_type: &'static str,
    presence: ValuePresence,
    initializer: ValueInitializer,
    observation: ObservationClass,
    docs: Box<[&'static str]>,
}

impl ValueDescription {
    /// Constructs one generated value description.
    #[doc(hidden)]
    #[must_use]
    #[allow(
        clippy::too_many_arguments,
        reason = "all value semantics are independent"
    )]
    pub fn generated(
        owner: ValueOwnerKind,
        owner_path: &'static str,
        name: &'static str,
        payload_type: &'static str,
        presence: ValuePresence,
        initializer: ValueInitializer,
        observation: ObservationClass,
        docs: Vec<&'static str>,
    ) -> Self {
        Self {
            owner,
            owner_path,
            name,
            payload_type,
            presence,
            initializer,
            observation,
            docs: docs.into_boxed_slice(),
        }
    }

    /// Returns the owner category.
    #[must_use]
    pub const fn owner(&self) -> ValueOwnerKind {
        self.owner
    }

    /// Returns the exact lexical owner path.
    #[must_use]
    pub const fn owner_path(&self) -> &'static str {
        self.owner_path
    }

    /// Returns the declared value name.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        self.name
    }

    /// Returns the opaque written payload type spelling.
    #[must_use]
    pub const fn payload_type(&self) -> &'static str {
        self.payload_type
    }

    /// Returns Concord-owned outer presence.
    #[must_use]
    pub const fn presence(&self) -> ValuePresence {
        self.presence
    }

    /// Returns the initializer category without evaluating or exposing a value.
    #[must_use]
    pub const fn initializer(&self) -> ValueInitializer {
        self.initializer
    }

    /// Returns the observation class.
    #[must_use]
    pub const fn observation(&self) -> ObservationClass {
        self.observation
    }

    /// Returns value documentation in source order.
    #[must_use]
    pub fn docs(&self) -> &[&'static str] {
        &self.docs
    }
}

/// An inheritable attachment family.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum AttachmentFamily {
    /// Inherited request-body representation selection or fence.
    RequestRepresentation,
    /// Inherited response representation selection or fence.
    ResponseRepresentation,
    /// Generated subdomain labels.
    Subdomain,
    /// Generated path segments.
    Path,
    /// Ordered public query pairs.
    Query,
    /// Public headers.
    Headers,
    /// Private authentication placements.
    Authentication,
    /// Operation timeout selection.
    Timeout,
    /// Additive rate policies.
    RateLimit,
    /// Endpoint-owned response negotiation.
    Accept,
}

/// The semantic patch operation represented by one attachment record.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum AttachmentOperation {
    /// Add before inherited entries.
    Prepend,
    /// Add after inherited or prior entries.
    Append,
    /// Replace inherited/current state.
    Replace,
    /// Remove a named entry.
    Remove,
    /// Clear the complete family.
    Off,
    /// Select one finite duration.
    Set,
    /// Replace only when the authored Concord-optional source is present.
    WhenPresent,
}

/// Whether an attachment record describes source-local intent or effective state.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum AttachmentView {
    /// The operation written at one declaration site.
    Local,
    /// One entry in the fully inherited resolved state.
    Effective,
}

/// One value-free attachment operation with stable lexical provenance.
#[non_exhaustive]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AttachmentDescription {
    family: AttachmentFamily,
    operation: AttachmentOperation,
    view: AttachmentView,
    origin: &'static str,
    target: Option<&'static str>,
    item_count: usize,
}

impl AttachmentDescription {
    /// Constructs a generated attachment record.
    #[doc(hidden)]
    #[must_use]
    pub const fn generated(
        family: AttachmentFamily,
        operation: AttachmentOperation,
        view: AttachmentView,
        origin: &'static str,
        target: Option<&'static str>,
        item_count: usize,
    ) -> Self {
        Self {
            family,
            operation,
            view,
            origin,
            target,
            item_count,
        }
    }

    /// Returns the attachment family.
    #[must_use]
    pub const fn family(&self) -> AttachmentFamily {
        self.family
    }

    /// Returns the patch operation.
    #[must_use]
    pub const fn operation(&self) -> AttachmentOperation {
        self.operation
    }

    /// Returns whether this is a local or effective view.
    #[must_use]
    pub const fn view(&self) -> AttachmentView {
        self.view
    }

    /// Returns the lexical client/scope/endpoint origin.
    #[must_use]
    pub const fn origin(&self) -> &'static str {
        self.origin
    }

    /// Returns a static field/query/policy target when the operation has one.
    #[must_use]
    pub const fn target(&self) -> Option<&'static str> {
        self.target
    }

    /// Returns the number of structural atoms represented by this record.
    #[must_use]
    pub const fn item_count(&self) -> usize {
        self.item_count
    }
}

/// Static request-body signature facts.
#[non_exhaustive]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BodyDescription {
    argument: &'static str,
    input_type: &'static str,
    specification_type: &'static str,
    docs: Box<[&'static str]>,
}

impl BodyDescription {
    /// Constructs generated body signature metadata.
    #[doc(hidden)]
    #[must_use]
    pub fn generated(
        argument: &'static str,
        input_type: &'static str,
        specification_type: &'static str,
        docs: Vec<&'static str>,
    ) -> Self {
        Self {
            argument,
            input_type,
            specification_type,
            docs: docs.into_boxed_slice(),
        }
    }

    /// Returns the caller argument name.
    #[must_use]
    pub const fn argument(&self) -> &'static str {
        self.argument
    }

    /// Returns the exact caller input type spelling.
    #[must_use]
    pub const fn input_type(&self) -> &'static str {
        self.input_type
    }

    /// Returns the request specification type spelling.
    #[must_use]
    pub const fn specification_type(&self) -> &'static str {
        self.specification_type
    }

    /// Returns body-parameter documentation.
    #[must_use]
    pub fn docs(&self) -> &[&'static str] {
        &self.docs
    }
}

/// Static response signature and trait-capability facts.
#[non_exhaustive]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResponseDescription {
    output_type: &'static str,
    specification_type: &'static str,
    acquisition_class: ContractFactClass,
}

impl ResponseDescription {
    /// Constructs generated response signature metadata.
    #[doc(hidden)]
    #[must_use]
    pub const fn generated(output_type: &'static str, specification_type: &'static str) -> Self {
        Self {
            output_type,
            specification_type,
            acquisition_class: ContractFactClass::TraitCapability,
        }
    }

    /// Returns the exact `.await` output type spelling.
    #[must_use]
    pub const fn output_type(&self) -> &'static str {
        self.output_type
    }

    /// Returns the response specification type spelling.
    #[must_use]
    pub const fn specification_type(&self) -> &'static str {
        self.specification_type
    }

    /// Returns how acquisition precision is obtained.
    #[must_use]
    pub const fn acquisition_class(&self) -> ContractFactClass {
        self.acquisition_class
    }
}

/// One private placement category, without credential material.
#[non_exhaustive]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuthenticationDescription {
    placement: &'static str,
    recovery: &'static str,
    origin: &'static str,
}

/// One protected builder input declaration, never its runtime material.
#[non_exhaustive]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SecretDescription {
    name: &'static str,
}

impl SecretDescription {
    /// Constructs generated secret-input metadata.
    #[doc(hidden)]
    #[must_use]
    pub const fn generated(name: &'static str) -> Self {
        Self { name }
    }

    /// Returns the generated builder-input name.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        self.name
    }

    /// Returns the structural observation class of protected input material.
    #[must_use]
    pub const fn observation(&self) -> ObservationClass {
        ObservationClass::PrivateSecretOrCredential
    }
}

/// The authority used to produce one declared credential slot.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum CredentialSourceKind {
    /// A static API-key value derived from one protected builder input.
    ApiKey,
    /// A static bearer value derived from one protected builder input.
    Bearer,
    /// A static Basic credential derived from two protected builder inputs.
    Basic,
    /// A credential acquired through one generated endpoint.
    Endpoint,
    /// A trusted application credential provider.
    CustomProvider,
}

/// One credential declaration without secret names or runtime material.
#[non_exhaustive]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CredentialDescription {
    name: &'static str,
    source: CredentialSourceKind,
    protected_input_count: usize,
    endpoint_id: Option<&'static str>,
    provider_type: Option<&'static str>,
    behavior_class: ContractFactClass,
}

impl CredentialDescription {
    /// Constructs generated credential-source metadata.
    #[doc(hidden)]
    #[must_use]
    pub const fn generated(
        name: &'static str,
        source: CredentialSourceKind,
        protected_input_count: usize,
        endpoint_id: Option<&'static str>,
        provider_type: Option<&'static str>,
        behavior_class: ContractFactClass,
    ) -> Self {
        Self {
            name,
            source,
            protected_input_count,
            endpoint_id,
            provider_type,
            behavior_class,
        }
    }

    /// Returns the declared credential name.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        self.name
    }

    /// Returns the credential source category.
    #[must_use]
    pub const fn source(&self) -> CredentialSourceKind {
        self.source
    }

    /// Returns how many protected builder inputs the source consumes.
    #[must_use]
    pub const fn protected_input_count(&self) -> usize {
        self.protected_input_count
    }

    /// Returns the stable acquisition endpoint ID for an endpoint-backed source.
    #[must_use]
    pub const fn endpoint_id(&self) -> Option<&'static str> {
        self.endpoint_id
    }

    /// Returns the opaque provider type spelling for a custom source.
    #[must_use]
    pub const fn provider_type(&self) -> Option<&'static str> {
        self.provider_type
    }

    /// Returns whether behavior is static or belongs to trusted extension code.
    #[must_use]
    pub const fn behavior_class(&self) -> ContractFactClass {
        self.behavior_class
    }

    /// Returns the structural observation class of credential material.
    #[must_use]
    pub const fn observation(&self) -> ObservationClass {
        ObservationClass::PrivateSecretOrCredential
    }
}

impl AuthenticationDescription {
    /// Constructs generated authentication metadata.
    #[doc(hidden)]
    #[must_use]
    pub const fn generated(
        placement: &'static str,
        recovery: &'static str,
        origin: &'static str,
    ) -> Self {
        Self {
            placement,
            recovery,
            origin,
        }
    }

    /// Returns the placement category, never material.
    #[must_use]
    pub const fn placement(&self) -> &'static str {
        self.placement
    }

    /// Returns the declared recovery signal.
    #[must_use]
    pub const fn recovery(&self) -> &'static str {
        self.recovery
    }

    /// Returns the lexical origin.
    #[must_use]
    pub const fn origin(&self) -> &'static str {
        self.origin
    }
}

/// One finite rate-limit window.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RateLimitWindowDescription {
    capacity: u32,
    duration: Duration,
}

impl RateLimitWindowDescription {
    /// Constructs a generated window.
    #[doc(hidden)]
    #[must_use]
    pub const fn generated(capacity: u32, duration: Duration) -> Self {
        Self { capacity, duration }
    }

    /// Returns the positive capacity.
    #[must_use]
    pub const fn capacity(&self) -> u32 {
        self.capacity
    }

    /// Returns the positive window duration.
    #[must_use]
    pub const fn duration(&self) -> Duration {
        self.duration
    }
}

/// One effective rate policy schema, never its runtime key values.
#[non_exhaustive]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RatePolicyDescription {
    id: &'static str,
    explicit_id: bool,
    rename_sensitive: bool,
    origin: &'static str,
    dimensions: Box<[&'static str]>,
    cost: u32,
    windows: Box<[RateLimitWindowDescription]>,
}

/// One client-owned reusable rate-limit profile before lexical binding.
#[non_exhaustive]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RateProfileDescription {
    name: &'static str,
    id: &'static str,
    explicit_id: bool,
    rename_sensitive: bool,
    formals: Box<[&'static str]>,
    dimensions: Box<[&'static str]>,
    cost: u32,
    windows: Box<[RateLimitWindowDescription]>,
}

impl RateProfileDescription {
    /// Constructs generated reusable-profile metadata.
    #[doc(hidden)]
    #[must_use]
    #[allow(
        clippy::too_many_arguments,
        reason = "profile schema components are independent"
    )]
    pub fn generated(
        name: &'static str,
        id: &'static str,
        explicit_id: bool,
        formals: Vec<&'static str>,
        dimensions: Vec<&'static str>,
        cost: u32,
        windows: Vec<RateLimitWindowDescription>,
    ) -> Self {
        Self {
            name,
            id,
            explicit_id,
            rename_sensitive: !explicit_id,
            formals: formals.into_boxed_slice(),
            dimensions: dimensions.into_boxed_slice(),
            cost,
            windows: windows.into_boxed_slice(),
        }
    }

    /// Returns the source profile name.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        self.name
    }

    /// Returns the explicit or deterministically derived policy ID.
    #[must_use]
    pub const fn id(&self) -> &'static str {
        self.id
    }

    /// Returns whether the author supplied the ID.
    #[must_use]
    pub const fn explicit_id(&self) -> bool {
        self.explicit_id
    }

    /// Returns whether declaration renaming can change the derived ID.
    #[must_use]
    pub const fn rename_sensitive(&self) -> bool {
        self.rename_sensitive
    }

    /// Returns profile formals in declaration order.
    #[must_use]
    pub fn formals(&self) -> &[&'static str] {
        &self.formals
    }

    /// Returns canonical key-dimension names in semantic order.
    #[must_use]
    pub fn dimensions(&self) -> &[&'static str] {
        &self.dimensions
    }

    /// Returns the profile's default positive admission cost.
    #[must_use]
    pub const fn cost(&self) -> u32 {
        self.cost
    }

    /// Returns ordered finite windows.
    #[must_use]
    pub fn windows(&self) -> &[RateLimitWindowDescription] {
        &self.windows
    }
}

impl RatePolicyDescription {
    /// Constructs generated policy metadata.
    #[doc(hidden)]
    #[must_use]
    #[allow(
        clippy::too_many_arguments,
        reason = "policy schema components are independent"
    )]
    pub fn generated(
        id: &'static str,
        explicit_id: bool,
        origin: &'static str,
        dimensions: Vec<&'static str>,
        cost: u32,
        windows: Vec<RateLimitWindowDescription>,
    ) -> Self {
        Self {
            id,
            explicit_id,
            rename_sensitive: !explicit_id,
            origin,
            dimensions: dimensions.into_boxed_slice(),
            cost,
            windows: windows.into_boxed_slice(),
        }
    }

    /// Returns the stable or deterministically derived policy identity.
    #[must_use]
    pub const fn id(&self) -> &'static str {
        self.id
    }

    /// Returns whether the author supplied the ID.
    #[must_use]
    pub const fn explicit_id(&self) -> bool {
        self.explicit_id
    }

    /// Returns whether declaration renaming can change the derived ID.
    #[must_use]
    pub const fn rename_sensitive(&self) -> bool {
        self.rename_sensitive
    }

    /// Returns the lexical origin.
    #[must_use]
    pub const fn origin(&self) -> &'static str {
        self.origin
    }

    /// Returns ordered dimension identities without values.
    #[must_use]
    pub fn dimensions(&self) -> &[&'static str] {
        &self.dimensions
    }

    /// Returns the positive admission cost.
    #[must_use]
    pub const fn cost(&self) -> u32 {
        self.cost
    }

    /// Returns ordered finite windows.
    #[must_use]
    pub fn windows(&self) -> &[RateLimitWindowDescription] {
        &self.windows
    }
}

/// One pagination controller field.
#[non_exhaustive]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PaginationFieldDescription {
    field: &'static str,
    destination: Option<&'static str>,
    stable_initializer: bool,
}

impl PaginationFieldDescription {
    /// Constructs generated pagination-field metadata.
    #[doc(hidden)]
    #[must_use]
    pub const fn generated(
        field: &'static str,
        destination: Option<&'static str>,
        stable_initializer: bool,
    ) -> Self {
        Self {
            field,
            destination,
            stable_initializer,
        }
    }

    /// Returns the controller field.
    #[must_use]
    pub const fn field(&self) -> &'static str {
        self.field
    }

    /// Returns the sole live endpoint destination, if any.
    #[must_use]
    pub const fn destination(&self) -> Option<&'static str> {
        self.destination
    }

    /// Returns whether the field is initialized once without mutation authority.
    #[must_use]
    pub const fn stable_initializer(&self) -> bool {
        self.stable_initializer
    }
}

/// Pagination structure without controller state or request values.
#[non_exhaustive]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PaginationContractDescription {
    controller_type: &'static str,
    fields: Box<[PaginationFieldDescription]>,
    bounds_class: ContractFactClass,
}

impl PaginationContractDescription {
    /// Constructs generated pagination metadata.
    #[doc(hidden)]
    #[must_use]
    pub fn generated(
        controller_type: &'static str,
        fields: Vec<PaginationFieldDescription>,
    ) -> Self {
        Self {
            controller_type,
            fields: fields.into_boxed_slice(),
            bounds_class: ContractFactClass::RuntimeBound,
        }
    }

    /// Returns the controller type spelling.
    #[must_use]
    pub const fn controller_type(&self) -> &'static str {
        self.controller_type
    }

    /// Returns ordered controller fields.
    #[must_use]
    pub fn fields(&self) -> &[PaginationFieldDescription] {
        &self.fields
    }

    /// Returns where aggregate bound values are selected.
    #[must_use]
    pub const fn bounds_class(&self) -> ContractFactClass {
        self.bounds_class
    }
}

/// Detailed static endpoint facts attached to [`EndpointDescription`].
#[non_exhaustive]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EndpointContractDescription {
    declaration: DeclarationDescription,
    values: Box<[ValueDescription]>,
    local_attachments: Box<[AttachmentDescription]>,
    effective_attachments: Box<[AttachmentDescription]>,
    body: Option<BodyDescription>,
    response: ResponseDescription,
    authentication: Box<[AuthenticationDescription]>,
    rate_policies: Box<[RatePolicyDescription]>,
    pagination: Option<PaginationContractDescription>,
    explicit_endpoint_id: bool,
}

impl EndpointContractDescription {
    /// Constructs generated endpoint detail metadata.
    #[doc(hidden)]
    #[must_use]
    #[allow(
        clippy::too_many_arguments,
        reason = "endpoint projections have independent semantic families"
    )]
    pub fn generated(
        declaration: DeclarationDescription,
        values: Vec<ValueDescription>,
        local_attachments: Vec<AttachmentDescription>,
        effective_attachments: Vec<AttachmentDescription>,
        body: Option<BodyDescription>,
        response: ResponseDescription,
        authentication: Vec<AuthenticationDescription>,
        rate_policies: Vec<RatePolicyDescription>,
        pagination: Option<PaginationContractDescription>,
        explicit_endpoint_id: bool,
    ) -> Self {
        Self {
            declaration,
            values: values.into_boxed_slice(),
            local_attachments: local_attachments.into_boxed_slice(),
            effective_attachments: effective_attachments.into_boxed_slice(),
            body,
            response,
            authentication: authentication.into_boxed_slice(),
            rate_policies: rate_policies.into_boxed_slice(),
            pagination,
            explicit_endpoint_id,
        }
    }

    /// Returns declaration docs and deprecation.
    #[must_use]
    pub const fn declaration(&self) -> &DeclarationDescription {
        &self.declaration
    }

    /// Returns endpoint-owned ordinary values.
    #[must_use]
    pub fn values(&self) -> &[ValueDescription] {
        &self.values
    }

    /// Returns attachment operations written on this endpoint.
    #[must_use]
    pub fn local_attachments(&self) -> &[AttachmentDescription] {
        &self.local_attachments
    }

    /// Returns the inherited and patched effective attachment state.
    #[must_use]
    pub fn effective_attachments(&self) -> &[AttachmentDescription] {
        &self.effective_attachments
    }

    /// Returns request-body signature facts.
    #[must_use]
    pub const fn body(&self) -> Option<&BodyDescription> {
        self.body.as_ref()
    }

    /// Returns response signature facts.
    #[must_use]
    pub const fn response(&self) -> &ResponseDescription {
        &self.response
    }

    /// Returns private placement categories without material.
    #[must_use]
    pub fn authentication(&self) -> &[AuthenticationDescription] {
        &self.authentication
    }

    /// Returns rate schemas without canonical key values.
    #[must_use]
    pub fn rate_policies(&self) -> &[RatePolicyDescription] {
        &self.rate_policies
    }

    /// Returns pagination structure when declared.
    #[must_use]
    pub const fn pagination(&self) -> Option<&PaginationContractDescription> {
        self.pagination.as_ref()
    }

    /// Returns whether the endpoint ID was explicitly authored.
    #[must_use]
    pub const fn explicit_endpoint_id(&self) -> bool {
        self.explicit_endpoint_id
    }

    /// Returns whether the endpoint ID is derived and rename-sensitive.
    #[must_use]
    pub const fn rename_sensitive_endpoint_id(&self) -> bool {
        !self.explicit_endpoint_id
    }
}

/// One reflected lexical scope and its domain-shaped descendants.
#[non_exhaustive]
#[derive(Clone, Debug)]
pub struct ScopeDescription {
    declaration: DeclarationDescription,
    values: Box<[ValueDescription]>,
    local_attachments: Box<[AttachmentDescription]>,
    effective_attachments: Box<[AttachmentDescription]>,
    scopes: Box<[ScopeDescription]>,
    endpoints: Box<[EndpointDescription]>,
}

impl ScopeDescription {
    /// Constructs a generated scope tree node.
    #[doc(hidden)]
    #[must_use]
    pub fn generated(
        declaration: DeclarationDescription,
        values: Vec<ValueDescription>,
        local_attachments: Vec<AttachmentDescription>,
        effective_attachments: Vec<AttachmentDescription>,
        scopes: Vec<Self>,
        endpoints: Vec<EndpointDescription>,
    ) -> Self {
        Self {
            declaration,
            values: values.into_boxed_slice(),
            local_attachments: local_attachments.into_boxed_slice(),
            effective_attachments: effective_attachments.into_boxed_slice(),
            scopes: scopes.into_boxed_slice(),
            endpoints: endpoints.into_boxed_slice(),
        }
    }

    /// Returns declaration docs and deprecation.
    #[must_use]
    pub const fn declaration(&self) -> &DeclarationDescription {
        &self.declaration
    }

    /// Returns scope-owned values.
    #[must_use]
    pub fn values(&self) -> &[ValueDescription] {
        &self.values
    }

    /// Returns source-local attachments.
    #[must_use]
    pub fn local_attachments(&self) -> &[AttachmentDescription] {
        &self.local_attachments
    }

    /// Returns fully inherited attachment state with lexical origins.
    #[must_use]
    pub fn effective_attachments(&self) -> &[AttachmentDescription] {
        &self.effective_attachments
    }

    /// Returns nested lexical scopes.
    #[must_use]
    pub fn scopes(&self) -> &[ScopeDescription] {
        &self.scopes
    }

    /// Returns endpoint leaves owned directly by this scope.
    #[must_use]
    pub fn endpoints(&self) -> &[EndpointDescription] {
        &self.endpoints
    }
}

/// The complete immutable generated-client contract tree.
#[non_exhaustive]
#[derive(Clone, Debug)]
pub struct ContractDescription {
    declaration: DeclarationDescription,
    base_origin: &'static str,
    timeout: TimeoutSelection,
    values: Box<[ValueDescription]>,
    secrets: Box<[SecretDescription]>,
    credentials: Box<[CredentialDescription]>,
    rate_profiles: Box<[RateProfileDescription]>,
    local_attachments: Box<[AttachmentDescription]>,
    effective_attachments: Box<[AttachmentDescription]>,
    scopes: Box<[ScopeDescription]>,
    endpoints: Box<[EndpointDescription]>,
}

impl ContractDescription {
    /// Constructs a generated contract tree.
    #[doc(hidden)]
    #[must_use]
    #[allow(
        clippy::too_many_arguments,
        reason = "contract projections have independent semantic families"
    )]
    pub fn generated(
        declaration: DeclarationDescription,
        base_origin: &'static str,
        timeout: TimeoutSelection,
        values: Vec<ValueDescription>,
        secrets: Vec<SecretDescription>,
        credentials: Vec<CredentialDescription>,
        rate_profiles: Vec<RateProfileDescription>,
        local_attachments: Vec<AttachmentDescription>,
        effective_attachments: Vec<AttachmentDescription>,
        scopes: Vec<ScopeDescription>,
        endpoints: Vec<EndpointDescription>,
    ) -> Self {
        Self {
            declaration,
            base_origin,
            timeout,
            values: values.into_boxed_slice(),
            secrets: secrets.into_boxed_slice(),
            credentials: credentials.into_boxed_slice(),
            rate_profiles: rate_profiles.into_boxed_slice(),
            local_attachments: local_attachments.into_boxed_slice(),
            effective_attachments: effective_attachments.into_boxed_slice(),
            scopes: scopes.into_boxed_slice(),
            endpoints: endpoints.into_boxed_slice(),
        }
    }

    /// Returns client documentation and deprecation.
    #[must_use]
    pub const fn declaration(&self) -> &DeclarationDescription {
        &self.declaration
    }

    /// Returns the fixed base origin and path declaration.
    #[must_use]
    pub const fn base_origin(&self) -> &'static str {
        self.base_origin
    }

    /// Returns the root timeout selection.
    #[must_use]
    pub const fn timeout(&self) -> TimeoutSelection {
        self.timeout
    }

    /// Returns client-owned ordinary values.
    #[must_use]
    pub fn values(&self) -> &[ValueDescription] {
        &self.values
    }

    /// Returns protected builder-input declarations without material.
    #[must_use]
    pub fn secrets(&self) -> &[SecretDescription] {
        &self.secrets
    }

    /// Returns credential source declarations without material.
    #[must_use]
    pub fn credentials(&self) -> &[CredentialDescription] {
        &self.credentials
    }

    /// Returns client-owned reusable rate-limit profile schemas.
    #[must_use]
    pub fn rate_profiles(&self) -> &[RateProfileDescription] {
        &self.rate_profiles
    }

    /// Returns source-local client attachments.
    #[must_use]
    pub fn local_attachments(&self) -> &[AttachmentDescription] {
        &self.local_attachments
    }

    /// Returns effective root attachment state with lexical origins.
    #[must_use]
    pub fn effective_attachments(&self) -> &[AttachmentDescription] {
        &self.effective_attachments
    }

    /// Returns root scopes in source order.
    #[must_use]
    pub fn scopes(&self) -> &[ScopeDescription] {
        &self.scopes
    }

    /// Returns root endpoints in source order.
    #[must_use]
    pub fn endpoints(&self) -> &[EndpointDescription] {
        &self.endpoints
    }
}

impl fmt::Display for ContractDescription {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{} ({})",
            self.declaration.name(),
            self.declaration.path()
        )
    }
}

/// Shared immutable endpoint-detail storage.
pub(crate) type SharedEndpointContractDescription = Arc<EndpointContractDescription>;

/// A compatibility category emitted by the tool-private semantic diff.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum SemanticChangeClass {
    /// Generated constructors, methods, or exact Rust signatures changed.
    CallerApi,
    /// Value presence, payload, default, owner, or observation meaning changed.
    ValueOrDomain,
    /// Public route, query, header, body-media, or negotiation behavior changed.
    Wire,
    /// Authentication or credential authority changed.
    SecurityOrAuthentication,
    /// Effect, timeout, deadline, or other managed runtime policy changed.
    RuntimePolicy,
    /// Generated terminal or response acquisition capability changed.
    Capability,
    /// Endpoint identity or rate-policy schema changed.
    IdentityOrRateSchema,
    /// Only or additionally author documentation/deprecation changed.
    Documentation,
}

impl SemanticChangeClass {
    /// Returns the deterministic tool label.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::CallerApi => "caller API change",
            Self::ValueOrDomain => "value/domain meaning change",
            Self::Wire => "wire route/query/header change",
            Self::SecurityOrAuthentication => "security/auth change",
            Self::RuntimePolicy => "runtime-policy/effect/deadline change",
            Self::Capability => "capability/terminal change",
            Self::IdentityOrRateSchema => "identity/rate-schema change",
            Self::Documentation => "documentation-only change",
        }
    }
}

/// Serializes a descriptor into Concord's deterministic, tool-private snapshot.
///
/// The format is deliberately not a public interchange promise. Fields use
/// explicit byte-length framing, records retain semantic/source order, and no
/// runtime values, canonical rate keys, body values, secrets, or credential
/// material participate.
#[doc(hidden)]
#[must_use]
pub fn contract_tool_snapshot(contract: &ContractDescription) -> String {
    let mut output = String::new();
    record(
        &mut output,
        "CALLER",
        &["client", contract.declaration.name()],
    );
    record(&mut output, "WIRE", &["base", contract.base_origin]);
    record(
        &mut output,
        "RUNTIME",
        &["root-timeout", &format!("{:?}", contract.timeout)],
    );
    declaration_snapshot(&mut output, &contract.declaration);
    for value in &contract.values {
        value_snapshot(&mut output, value);
    }
    for secret in &contract.secrets {
        record(&mut output, "SECURITY", &["protected-input", secret.name]);
    }
    for credential in &contract.credentials {
        record(
            &mut output,
            "SECURITY",
            &[
                "credential-source",
                credential.name,
                &format!("{:?}", credential.source),
                &credential.protected_input_count.to_string(),
                credential.endpoint_id.unwrap_or(""),
                credential.provider_type.unwrap_or(""),
                &format!("{:?}", credential.behavior_class),
            ],
        );
    }
    for profile in &contract.rate_profiles {
        let mut fields = vec![
            "rate-profile",
            profile.name,
            profile.id,
            if profile.explicit_id {
                "explicit"
            } else {
                "derived"
            },
        ];
        fields.extend(profile.formals.iter().copied());
        fields.push("dimensions");
        fields.extend(profile.dimensions.iter().copied());
        let cost = profile.cost.to_string();
        fields.push(&cost);
        let windows = profile
            .windows
            .iter()
            .map(|window| format!("{}@{:?}", window.capacity, window.duration))
            .collect::<Vec<_>>();
        fields.extend(windows.iter().map(String::as_str));
        record(&mut output, "IDENTITY", &fields);
    }
    attachments_snapshot(&mut output, &contract.local_attachments);
    attachments_snapshot(&mut output, &contract.effective_attachments);
    endpoints_snapshot(&mut output, &contract.endpoints);
    for scope in &contract.scopes {
        scope_snapshot(&mut output, scope);
    }
    output
}

/// Classifies two tool-private semantic snapshots.
///
/// Both inputs must have been produced by [`contract_tool_snapshot`] or an
/// xtask fixture via the same line-tag and length-framing convention.
#[doc(hidden)]
#[must_use]
pub fn diff_contract_tool_snapshots(before: &str, after: &str) -> Vec<SemanticChangeClass> {
    let before = snapshot_groups(before);
    let after = snapshot_groups(after);
    let mut changes = BTreeSet::new();
    for tag in before.keys().chain(after.keys()) {
        if before.get(tag) == after.get(tag) {
            continue;
        }
        let class = match tag.as_str() {
            "CALLER" => SemanticChangeClass::CallerApi,
            "VALUE" => SemanticChangeClass::ValueOrDomain,
            "WIRE" => SemanticChangeClass::Wire,
            "SECURITY" => SemanticChangeClass::SecurityOrAuthentication,
            "RUNTIME" => SemanticChangeClass::RuntimePolicy,
            "CAPABILITY" => SemanticChangeClass::Capability,
            "IDENTITY" => SemanticChangeClass::IdentityOrRateSchema,
            "DOC" => SemanticChangeClass::Documentation,
            _ => continue,
        };
        changes.insert(class);
    }
    changes.into_iter().collect()
}

fn snapshot_groups(snapshot: &str) -> BTreeMap<String, Vec<&str>> {
    let mut groups = BTreeMap::<String, Vec<&str>>::new();
    for line in snapshot.lines() {
        let tag = line.split_once('|').map_or(line, |(tag, _)| tag);
        groups.entry(tag.to_owned()).or_default().push(line);
    }
    groups
}

fn scope_snapshot(output: &mut String, scope: &ScopeDescription) {
    record(output, "CALLER", &["scope", scope.declaration.path]);
    declaration_snapshot(output, &scope.declaration);
    for value in &scope.values {
        value_snapshot(output, value);
    }
    attachments_snapshot(output, &scope.local_attachments);
    attachments_snapshot(output, &scope.effective_attachments);
    endpoints_snapshot(output, &scope.endpoints);
    for nested in &scope.scopes {
        scope_snapshot(output, nested);
    }
}

fn endpoints_snapshot(output: &mut String, endpoints: &[EndpointDescription]) {
    for endpoint in endpoints {
        record(
            output,
            "CALLER",
            &[
                "endpoint",
                endpoint.source_path(),
                endpoint.method().as_str(),
            ],
        );
        record(output, "IDENTITY", &["endpoint", endpoint.id()]);
        record(
            output,
            "WIRE",
            &["route", endpoint.source_path(), endpoint.route_shape()],
        );
        record(
            output,
            "RUNTIME",
            &[
                "endpoint",
                endpoint.source_path(),
                &format!("{:?}", endpoint.timeout()),
                &format!("{:?}", endpoint.accepted_status()),
                &format!("{:?}", endpoint.status_source()),
                &format!("{:?}", endpoint.effect()),
                &format!("{:?}", endpoint.effect_source()),
            ],
        );
        let capabilities = endpoint.capabilities();
        record(
            output,
            "CAPABILITY",
            &[
                endpoint.source_path(),
                &capabilities.pagination().to_string(),
                &capabilities.accepted_bytes().to_string(),
                &capabilities.credential_storage().to_string(),
                &format!("{:?}", endpoint.response_acquisition()),
            ],
        );
        let Some(details) = endpoint.contract() else {
            continue;
        };
        declaration_snapshot(output, &details.declaration);
        record(
            output,
            "CALLER",
            &[
                "response",
                details.response.output_type,
                details.response.specification_type,
            ],
        );
        if let Some(body) = &details.body {
            record(
                output,
                "CALLER",
                &[
                    "body",
                    body.argument,
                    body.input_type,
                    body.specification_type,
                ],
            );
        }
        for value in &details.values {
            value_snapshot(output, value);
        }
        attachments_snapshot(output, &details.local_attachments);
        attachments_snapshot(output, &details.effective_attachments);
        for auth in &details.authentication {
            record(
                output,
                "SECURITY",
                &[auth.placement, auth.recovery, auth.origin],
            );
        }
        for policy in &details.rate_policies {
            let mut fields = vec![
                policy.id,
                if policy.explicit_id {
                    "explicit"
                } else {
                    "derived"
                },
                policy.origin,
            ];
            fields.extend(policy.dimensions.iter().copied());
            let cost = policy.cost.to_string();
            fields.push(&cost);
            let windows = policy
                .windows
                .iter()
                .map(|window| format!("{}@{:?}", window.capacity, window.duration))
                .collect::<Vec<_>>();
            fields.extend(windows.iter().map(String::as_str));
            record(output, "IDENTITY", &fields);
        }
        if let Some(pagination) = &details.pagination {
            record(
                output,
                "CAPABILITY",
                &["pagination-controller", pagination.controller_type],
            );
            for field in &pagination.fields {
                record(
                    output,
                    "RUNTIME",
                    &[
                        "pagination-field",
                        field.field,
                        field.destination.unwrap_or("stable"),
                    ],
                );
            }
        }
    }
}

fn declaration_snapshot(output: &mut String, declaration: &DeclarationDescription) {
    record(
        output,
        "DOC",
        &[
            declaration.path,
            declaration.deprecated.unwrap_or(""),
            &declaration.docs.join("\n"),
        ],
    );
}

fn value_snapshot(output: &mut String, value: &ValueDescription) {
    let fields = [
        value.owner_path,
        value.name,
        value.payload_type,
        match value.owner {
            ValueOwnerKind::Client => "client",
            ValueOwnerKind::Scope => "scope",
            ValueOwnerKind::Endpoint => "endpoint",
        },
        match value.presence {
            ValuePresence::Required => "required",
            ValuePresence::Optional => "optional",
        },
        match value.initializer {
            ValueInitializer::None => "none",
            ValueInitializer::TraitDefault => "trait-default",
            ValueInitializer::StableValue => "stable-value",
        },
        match value.observation {
            ObservationClass::OrdinaryPublic => "ordinary",
            ObservationClass::RedactedPublic => "redacted",
            ObservationClass::PrivateSecretOrCredential => "private",
        },
    ];
    record(output, "VALUE", &fields);
    record(
        output,
        "CALLER",
        &["value", value.owner_path, value.name, value.payload_type],
    );
    record(
        output,
        "DOC",
        &[value.owner_path, value.name, &value.docs.join("\n")],
    );
}

fn attachments_snapshot(output: &mut String, attachments: &[AttachmentDescription]) {
    for attachment in attachments {
        let tag = match attachment.family {
            AttachmentFamily::Authentication => "SECURITY",
            AttachmentFamily::Timeout => "RUNTIME",
            AttachmentFamily::RateLimit => "IDENTITY",
            AttachmentFamily::RequestRepresentation
            | AttachmentFamily::ResponseRepresentation
            | AttachmentFamily::Subdomain
            | AttachmentFamily::Path
            | AttachmentFamily::Query
            | AttachmentFamily::Headers
            | AttachmentFamily::Accept => "WIRE",
        };
        record(
            output,
            tag,
            &[
                &format!("{:?}", attachment.family),
                &format!("{:?}", attachment.operation),
                &format!("{:?}", attachment.view),
                attachment.origin,
                attachment.target.unwrap_or(""),
                &attachment.item_count.to_string(),
            ],
        );
    }
}

fn record(output: &mut String, tag: &str, fields: &[&str]) {
    output.push_str(tag);
    for field in fields {
        output.push('|');
        output.push_str(&field.len().to_string());
        output.push(':');
        output.push_str(field);
    }
    output.push('\n');
}

#[cfg(test)]
mod snapshot_tests {
    use super::*;

    #[test]
    fn diff_classification_is_deterministic_and_ignores_unknown_tool_records() {
        let before = "CALLER|1:a\nVALUE|1:a\nWIRE|1:a\nSECURITY|1:a\nRUNTIME|1:a\nCAPABILITY|1:a\nIDENTITY|1:a\nDOC|1:a\nUNKNOWN|1:a\n";
        let after = "CALLER|1:b\nVALUE|1:b\nWIRE|1:b\nSECURITY|1:b\nRUNTIME|1:b\nCAPABILITY|1:b\nIDENTITY|1:b\nDOC|1:b\nUNKNOWN|1:b\n";
        assert_eq!(
            diff_contract_tool_snapshots(before, after),
            vec![
                SemanticChangeClass::CallerApi,
                SemanticChangeClass::ValueOrDomain,
                SemanticChangeClass::Wire,
                SemanticChangeClass::SecurityOrAuthentication,
                SemanticChangeClass::RuntimePolicy,
                SemanticChangeClass::Capability,
                SemanticChangeClass::IdentityOrRateSchema,
                SemanticChangeClass::Documentation,
            ]
        );
        assert!(diff_contract_tool_snapshots(before, before).is_empty());
    }
}
