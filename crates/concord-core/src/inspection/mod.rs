//! Complete, value-free effective contract descriptions (§18).
//!
//! A [`ContractDescription`] is the resolved static contract of one generated
//! client: the owner tree, value declarations, local and effective
//! attachments with provenance, route/DNS/query/header IR, credential
//! sources and effective auth placements, endpoint identities, rate
//! policies with coverage and possible sharing, status/effect/time facts,
//! pagination mappings, representation and driver capabilities, terminal
//! availability, opaque dependencies and client-level diagnostics.
//!
//! # Generated-code bridge
//!
//! Generated code constructs a description once, inside a
//! `std::sync::LazyLock`, through [`ContractBuilder`]:
//!
//! ```
//! use std::sync::LazyLock;
//! use std::time::Duration;
//! use concord_core::inspection::*;
//! use concord_core::contracts::ResponseAcquisition;
//!
//! static CONTRACT: LazyLock<ContractDescription> = LazyLock::new(|| {
//!     ContractDescription::builder(
//!         OwnerSpec::client("Catalog")
//!             .response_default(RepresentationSelection::Type("Json"))
//!             .value(ValueSpec::new("tenant", "String"))
//!             .query(KeyOperation::direct("tenant", Emission::value(ValueRef::local("tenant")))),
//!         "https://catalog.example.test/v1/",
//!     )
//!     .endpoint(
//!         EndpointSpec::new("Catalog::items", "GET")
//!             .path(PathOperation::Append(PathTemplate::new(
//!                 "items",
//!                 vec![PathSegment::literal("items")],
//!             )))
//!             .result(ResultSpec::new(
//!                 "Vec<Item>",
//!                 ResponseAcquisition::Buffered,
//!                 Some("application/json"),
//!             )),
//!     )
//!     .build()
//!     .expect("macro-validated contract")
//! });
//!
//! let contract: &'static ContractDescription = &CONTRACT;
//! assert_eq!(contract.endpoints()[0].id(), "catalog.items");
//! assert_eq!(contract.root_timeout().duration(), Duration::from_secs(30));
//! ```
//!
//! Runtime [`EndpointDescription`]
//! values for generated endpoints are then derived from the built record with
//! [`EndpointDescription::from_contract`],
//! so the executor and the inspection model share one source of meaning.
//!
//! # Ownership, concurrency, cancellation and sensitivity
//!
//! Descriptions are immutable `Send + Sync` data built without I/O; there is
//! nothing to cancel and no resource to release. Construction never evaluates
//! defaults, converters, encoders or providers: representation facts are read
//! from associated constants only. Sensitive literals are redacted at build
//! time and secret or credential material never enters the model.
//!
//! # Serialization
//!
//! Core has no JSON dependency. The versioned structural format
//! [`CONTRACT_FORMAT`] (`concord.contract/3`) is produced by the
//! `concord-inspect` tooling crate, which also implements semantic diffing.

mod auth;
mod contract;
mod endpoint;
mod owner;
mod rate;
mod resolve;
mod shape;
mod values;

#[cfg(test)]
mod tests;

/// HEAD-01 header policy shared with runtime-endpoint validation (one owner).
pub(crate) use resolve::{FORBIDDEN_PUBLIC_HEADERS, SINGLETON_HEADERS};

pub use crate::auth::ChallengeRecovery;
pub use crate::endpoint::{
    Effect, EffectSource, EndpointDescription, EndpointId, ResponseAcquisition, StatusSource,
};
pub use crate::rate_limit::RateDimensionKind;
pub use auth::{
    AuthOperation, AuthPlacement, AuthResetMode, ChallengeRule, CredentialDescription,
    CredentialSourceKind, DisplacedAuthPlacement, EffectiveAuth, EffectiveAuthPlacement,
    PlacementTarget, SecretDescription,
};
pub use contract::{
    BaseDescription, ContractBuildError, ContractBuildErrorKind, ContractBuilder,
    ContractDescription, ContractDiagnostic, DependencyKind, DiagnosticSeverity, OpaqueDependency,
};
pub use endpoint::{
    AcceptedStatuses, BodyContract, BodySpec, EffectiveKeyOperation, EffectiveRepresentation,
    EffectiveRoute, EffectiveTimeout, EndpointContract, EndpointIdentity, EndpointSpec, IdSource,
    InitField, InitValue, PaginationContract, RepresentationSource, ResultContract, ResultSpec,
    RouteLabel, RouteReset, RouteSegment, StatusRange, TerminalAvailability, TimeoutSource,
};
pub use owner::{
    EffectiveDefault, EffectiveOwnerView, LocalAttachments, OwnerDescription, OwnerSpec,
};
pub use rate::{
    BoundDimension, CostSource, CoveredEndpoint, EffectiveDimension, EffectivePolicy, EncoderRole,
    FormalDimension, LabelBinding, LabelCondition, PolicyAttachmentSite, PolicyCoverage,
    PolicyDefinition, PolicyOrigin, RateAttachment, RateAttachmentSource, RateBinding,
    SharingCondition, SharingGroup, WindowDescription, WindowSetDescription,
};
pub use shape::{
    AtomExpression, Emission, FamilyMode, HeadFamily, KeyOperation, KeyOperationKind, LiteralKind,
    LiteralText, OperationSite, PathOperation, PathSegment, PathTemplate, RepresentationSelection,
    SubdomainOperation, TemplatePart,
};
pub use values::{
    Deprecation, InitializerExpression, OwnerKind, ReferenceQualifier, Sensitivity, ValueControl,
    ValueDescription, ValueInitializer, ValueLiveness, ValuePresence, ValueRef, ValueRole,
    ValueSpec,
};

/// The versioned structural export format name produced by tooling.
pub const CONTRACT_FORMAT: &str = "concord.contract/3";

/// Placeholder rendered for a redacted literal in human-readable views.
pub const REDACTED_MARKER: &str = "<redacted>";

/// Renders an atom in DSL display form (`{owner.name}` holes, literal text,
/// [`REDACTED_MARKER`] for redacted literals).
pub(crate) fn render_atom(atom: &AtomExpression, display: &impl Fn(&ValueRef) -> String) -> String {
    match atom {
        AtomExpression::Literal { text, .. } => match text {
            LiteralText::Text(text) => (*text).to_owned(),
            LiteralText::Redacted => REDACTED_MARKER.to_owned(),
        },
        AtomExpression::Value { value, adapter } => match adapter {
            Some(adapter) => format!("{{{} via {adapter}}}", display(value)),
            None => format!("{{{}}}", display(value)),
        },
        AtomExpression::Concat(parts) => parts
            .iter()
            .map(|part| render_atom(part, display))
            .collect(),
    }
}

/// Renders one path segment in template display form.
pub(crate) fn render_segment(
    segment: &PathSegment,
    display: &impl Fn(&ValueRef) -> String,
) -> String {
    segment
        .parts()
        .iter()
        .map(|part| match part {
            TemplatePart::Literal(text) => (*text).to_owned(),
            TemplatePart::Hole { value, adapter } => match adapter {
                Some(adapter) => format!("{{{} via {adapter}}}", display(value)),
                None => format!("{{{}}}", display(value)),
            },
        })
        .collect()
}
