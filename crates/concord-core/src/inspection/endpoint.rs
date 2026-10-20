//! Endpoint declarations and their resolved effective contract records.

use std::time::Duration;

use crate::body::{BodyDescription, PageRequestRepresentation, RequestRepresentation};
use crate::endpoint::{Effect, EffectSource, EndpointId, ResponseAcquisition, StatusSource};
use crate::response::{ResponseDriver, ResponseRepresentation};

use super::auth::{AuthOperation, EffectiveAuth};
use super::owner::OwnerSpec;
use super::rate::{EffectivePolicy, RateAttachment};
use super::shape::{
    AtomExpression, FamilyMode, HeadFamily, KeyOperation, PathOperation, PathSegment,
};
use super::values::{Deprecation, OwnerKind, ValuePresence, ValueRef, ValueSpec};

/// One inclusive final-status range of an explicit `status` clause.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct StatusRange {
    start: u16,
    end: u16,
}

impl StatusRange {
    /// One status code.
    #[must_use]
    pub const fn single(code: u16) -> Self {
        Self {
            start: code,
            end: code,
        }
    }

    /// An inclusive `start..=end` range.
    #[must_use]
    pub const fn range(start: u16, end: u16) -> Self {
        Self { start, end }
    }

    /// Returns the first code.
    #[must_use]
    pub const fn start(&self) -> u16 {
        self.start
    }

    /// Returns the last code (inclusive).
    #[must_use]
    pub const fn end(&self) -> u16 {
        self.end
    }

    /// Returns whether `code` is inside the range.
    #[must_use]
    pub const fn contains(&self, code: u16) -> bool {
        self.start <= code && code <= self.end
    }
}

/// The endpoint's accepted payload statuses (§10.1).
#[non_exhaustive]
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum AcceptedStatuses {
    /// Every final status in `200..=299`.
    Success,
    /// A canonical sorted, disjoint set of ranges.
    Explicit(Box<[StatusRange]>),
}

impl AcceptedStatuses {
    /// Returns whether a status is accepted.
    #[must_use]
    pub fn contains(&self, code: u16) -> bool {
        match self {
            Self::Success => (200..=299).contains(&code),
            Self::Explicit(ranges) => ranges.iter().any(|range| range.contains(code)),
        }
    }
}

/// A pagination `init` field value.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum InitValue {
    /// A Rust literal as written.
    Literal(&'static str),
    /// A Rust constant path as written (an opaque dependency).
    Constant(&'static str),
    /// A value reference.
    Value(ValueRef),
}

/// One `init Type { field: value }` mapping.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct InitField {
    field: &'static str,
    value: InitValue,
}

impl InitField {
    /// Returns the public initialization-type field name.
    #[must_use]
    pub const fn field(&self) -> &'static str {
        self.field
    }

    /// Returns the mapped value.
    #[must_use]
    pub const fn value(&self) -> InitValue {
        self.value
    }
}

/// The pagination declaration of one endpoint (§14.1).
///
/// Used both as generated-code bridge input and in built descriptions; in a
/// built description every reference is resolved.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct PaginationContract {
    controller: &'static str,
    init_type: &'static str,
    init: Vec<InitField>,
    cursor: Vec<ValueRef>,
}

impl PaginationContract {
    /// `paginate via Controller { init InitType { ... }; cursor -> ...; }`.
    #[must_use]
    pub fn new(controller: &'static str, init_type: &'static str) -> Self {
        Self {
            controller,
            init_type,
            init: Vec::new(),
            cursor: Vec::new(),
        }
    }

    /// Appends `field: reference`.
    #[must_use]
    pub fn init_value(mut self, field: &'static str, value: ValueRef) -> Self {
        self.init.push(InitField {
            field,
            value: InitValue::Value(value),
        });
        self
    }

    /// Appends `field: literal`.
    #[must_use]
    pub fn init_literal(mut self, field: &'static str, literal: &'static str) -> Self {
        self.init.push(InitField {
            field,
            value: InitValue::Literal(literal),
        });
        self
    }

    /// Appends `field: CONSTANT`.
    #[must_use]
    pub fn init_constant(mut self, field: &'static str, path: &'static str) -> Self {
        self.init.push(InitField {
            field,
            value: InitValue::Constant(path),
        });
        self
    }

    /// Appends one cursor destination; several destinations form a tuple.
    #[must_use]
    pub fn cursor(mut self, destination: ValueRef) -> Self {
        self.cursor.push(destination);
        self
    }

    /// Returns the controller Rust type spelling.
    #[must_use]
    pub const fn controller(&self) -> &'static str {
        self.controller
    }

    /// Returns the public `Init` Rust type spelling.
    #[must_use]
    pub const fn init_type(&self) -> &'static str {
        self.init_type
    }

    /// Returns `init` field mappings in written order.
    #[must_use]
    pub fn init(&self) -> &[InitField] {
        &self.init
    }

    /// Returns cursor destinations (one entry, or a tuple in order).
    #[must_use]
    pub fn cursor_destinations(&self) -> &[ValueRef] {
        &self.cursor
    }

    pub(crate) fn refs_mut(&mut self) -> impl Iterator<Item = &mut ValueRef> {
        self.init
            .iter_mut()
            .filter_map(|field| match &mut field.value {
                InitValue::Value(value) => Some(value),
                _ => None,
            })
            .chain(self.cursor.iter_mut())
    }
}

/// Generated-code bridge input for the endpoint result.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ResultSpec {
    pub(crate) output_type: &'static str,
    pub(crate) inline_representation: Option<&'static str>,
    pub(crate) driver: ResponseAcquisition,
    pub(crate) accept: Option<&'static str>,
}

impl ResultSpec {
    /// A result with explicitly supplied static facts.
    #[must_use]
    pub const fn new(
        output_type: &'static str,
        driver: ResponseAcquisition,
        accept: Option<&'static str>,
    ) -> Self {
        Self {
            output_type,
            inline_representation: None,
            driver,
            accept,
        }
    }

    /// A result whose static facts are read from the selected representation
    /// `R` for payload `O`: `R::ACCEPT` and the driver's acquisition
    /// category. Reading these constants runs no converter code.
    ///
    /// For head-only expected absence pass the present payload type as `O`.
    #[must_use]
    pub const fn from_representation<R, O>(output_type: &'static str) -> Self
    where
        R: ResponseRepresentation<O>,
        O: Send + 'static,
    {
        Self::new(
            output_type,
            <R::Driver as ResponseDriver>::ACQUISITION,
            R::ACCEPT,
        )
    }

    /// Records an inline `-> T via R` representation type spelling.
    #[must_use]
    pub const fn via(mut self, representation: &'static str) -> Self {
        self.inline_representation = Some(representation);
        self
    }
}

/// Generated-code bridge input for the endpoint body parameter.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct BodySpec {
    pub(crate) name: &'static str,
    pub(crate) input_type: &'static str,
    pub(crate) presence: ValuePresence,
    pub(crate) inline_representation: Option<&'static str>,
    pub(crate) description: BodyDescription,
    pub(crate) page_capable: bool,
    pub(crate) docs: &'static [&'static str],
}

impl BodySpec {
    /// `body name: InputType` with an explicitly supplied static description.
    #[must_use]
    pub const fn new(
        name: &'static str,
        input_type: &'static str,
        description: BodyDescription,
    ) -> Self {
        Self {
            name,
            input_type,
            presence: ValuePresence::Required,
            inline_representation: None,
            description,
            page_capable: false,
            docs: &[],
        }
    }

    /// `body name: I` whose static description is read from the selected
    /// request representation `B` (`B::DESCRIPTION`); no representation code
    /// runs.
    #[must_use]
    pub const fn from_representation<B, I>(name: &'static str, input_type: &'static str) -> Self
    where
        B: RequestRepresentation<I>,
        I: Send + 'static,
    {
        Self::new(name, input_type, B::DESCRIPTION)
    }

    /// Marks `body name?: T`.
    #[must_use]
    pub const fn optional(mut self) -> Self {
        self.presence = ValuePresence::Optional;
        self
    }

    /// Records an inline `body name: T via R` representation spelling.
    #[must_use]
    pub const fn via(mut self, representation: &'static str) -> Self {
        self.inline_representation = Some(representation);
        self
    }

    /// Records the static page-body capability, proven by the bound
    /// `B: PageRequestRepresentation<I>`.
    #[must_use]
    pub const fn page_capable_for<B, I>(mut self) -> Self
    where
        B: PageRequestRepresentation<I>,
        I: Clone + Send + 'static,
    {
        self.page_capable = true;
        self
    }

    /// Records author documentation lines.
    #[must_use]
    pub const fn docs(mut self, docs: &'static [&'static str]) -> Self {
        self.docs = docs;
        self
    }
}

/// Generated-code bridge input for one endpoint.
#[derive(Clone, Debug)]
pub struct EndpointSpec {
    pub(crate) owner: OwnerSpec,
    pub(crate) method: &'static str,
    pub(crate) id: Option<&'static str>,
    pub(crate) result: Option<ResultSpec>,
    pub(crate) body: Option<(BodySpec, usize)>,
    pub(crate) accepted: Option<AcceptedStatuses>,
    pub(crate) absent_on: Option<&'static [u16]>,
    pub(crate) effect: Option<Effect>,
    pub(crate) pagination: Option<PaginationContract>,
    pub(crate) expose_accepted_bytes: bool,
    pub(crate) duplicate_clause: Option<&'static str>,
}

impl EndpointSpec {
    /// An endpoint with its complete source path (for example
    /// `CatalogApi::regional::item`) and HTTP method token as written
    /// (`GET`, `POST`, or a `METHOD("...")` token).
    #[must_use]
    pub fn new(path: &'static str, method: &'static str) -> Self {
        Self {
            owner: OwnerSpec::new(OwnerKind::Endpoint, path),
            method,
            id: None,
            result: None,
            body: None,
            accepted: None,
            absent_on: None,
            effect: None,
            pagination: None,
            expose_accepted_bytes: false,
            duplicate_clause: None,
        }
    }

    fn duplicate(&mut self, clause: &'static str, already: bool) {
        if already && self.duplicate_clause.is_none() {
            self.duplicate_clause = Some(clause);
        }
    }

    /// Records author documentation lines.
    #[must_use]
    pub fn docs(mut self, docs: &'static [&'static str]) -> Self {
        self.owner = self.owner.docs(docs);
        self
    }

    /// Records author deprecation.
    #[must_use]
    pub fn deprecated(mut self, deprecation: Deprecation) -> Self {
        self.owner = self.owner.deprecated(deprecation);
        self
    }

    /// Appends one ordinary parameter in declaration order.
    #[must_use]
    pub fn value(mut self, value: ValueSpec) -> Self {
        self.owner = self.owner.value(value);
        self
    }

    /// Records the body parameter at its declaration position (after the
    /// values appended so far).
    #[must_use]
    pub fn body(mut self, body: BodySpec) -> Self {
        let already = self.body.is_some();
        self.duplicate("body", already);
        self.body = Some((body, self.owner.values.len()));
        self
    }

    /// Records the endpoint `path` clause.
    #[must_use]
    pub fn path(mut self, operation: PathOperation) -> Self {
        self.owner = self.owner.path(operation);
        self
    }

    /// Records the query family mode.
    #[must_use]
    pub fn query_mode(mut self, mode: FamilyMode) -> Self {
        self.owner = self.owner.query_mode(mode);
        self
    }

    /// Appends one query operation in normalized local order.
    #[must_use]
    pub fn query(mut self, operation: KeyOperation) -> Self {
        self.owner = self.owner.query(operation);
        self
    }

    /// Records the header family mode.
    #[must_use]
    pub fn header_mode(mut self, mode: FamilyMode) -> Self {
        self.owner = self.owner.header_mode(mode);
        self
    }

    /// Appends one header operation in normalized local order.
    #[must_use]
    pub fn header(mut self, operation: KeyOperation) -> Self {
        self.owner = self.owner.header(operation);
        self
    }

    /// Records the endpoint `auth` clause.
    #[must_use]
    pub fn auth(mut self, operation: AuthOperation) -> Self {
        self.owner = self.owner.auth(operation);
        self
    }

    /// Records the endpoint `timeout` selection.
    #[must_use]
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.owner = self.owner.timeout(timeout);
        self
    }

    /// Appends one endpoint rate attachment.
    #[must_use]
    pub fn rate_limit(mut self, attachment: RateAttachment) -> Self {
        self.owner = self.owner.rate_limit(attachment);
        self
    }

    /// Records the explicit `id "..."` clause.
    #[must_use]
    pub fn id(mut self, id: &'static str) -> Self {
        let already = self.id.is_some();
        self.duplicate("id", already);
        self.id = Some(id);
        self
    }

    /// Records the result and its static representation facts.
    #[must_use]
    pub fn result(mut self, result: ResultSpec) -> Self {
        let already = self.result.is_some();
        self.duplicate("result", already);
        self.result = Some(result);
        self
    }

    /// Records `status success;` written explicitly.
    #[must_use]
    pub fn status_success(mut self) -> Self {
        let already = self.accepted.is_some();
        self.duplicate("status", already);
        self.accepted = Some(AcceptedStatuses::Success);
        self
    }

    /// Records an explicit `status` code/range list.
    #[must_use]
    pub fn status(mut self, ranges: Vec<StatusRange>) -> Self {
        let already = self.accepted.is_some();
        self.duplicate("status", already);
        self.accepted = Some(AcceptedStatuses::Explicit(ranges.into_boxed_slice()));
        self
    }

    /// Records `absent on codes...;`.
    #[must_use]
    pub fn absent_on(mut self, codes: &'static [u16]) -> Self {
        let already = self.absent_on.is_some();
        self.duplicate("absent", already);
        self.absent_on = Some(codes);
        self
    }

    /// Records an explicit `effect` assertion.
    #[must_use]
    pub fn effect(mut self, effect: Effect) -> Self {
        let already = self.effect.is_some();
        self.duplicate("effect", already);
        self.effect = Some(effect);
        self
    }

    /// Records the `paginate` clause.
    #[must_use]
    pub fn pagination(mut self, pagination: PaginationContract) -> Self {
        let already = self.pagination.is_some();
        self.duplicate("paginate", already);
        self.pagination = Some(pagination);
        self
    }

    /// Records `expose accepted_bytes;`.
    #[must_use]
    pub fn expose_accepted_bytes(mut self) -> Self {
        let already = self.expose_accepted_bytes;
        self.duplicate("expose", already);
        self.expose_accepted_bytes = true;
        self
    }
}

/// Whether the endpoint ID was written or derived (IDENTITY-01).
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum IdSource {
    /// `id "..."` pins operational identity.
    Explicit,
    /// Navigation-derived `snake_case(Client).scope_path.endpoint`; it
    /// changes on relevant renames and moves.
    Derived,
}

impl IdSource {
    /// Returns the stable lowercase label used by structural exports.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Explicit => "explicit",
            Self::Derived => "derived",
        }
    }
}

/// The operational identity of one endpoint.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct EndpointIdentity {
    pub(crate) id: EndpointId,
    pub(crate) source: IdSource,
    pub(crate) derivation_inputs: Box<[Box<str>]>,
}

impl EndpointIdentity {
    /// Returns the validated operational ID.
    #[must_use]
    pub const fn id(&self) -> &EndpointId {
        &self.id
    }

    /// Returns whether the ID was written or derived.
    #[must_use]
    pub const fn source(&self) -> IdSource {
        self.source
    }

    /// Returns the derivation inputs of a derived ID: the snake-cased client
    /// name followed by the logical scope and endpoint identifiers. Empty for
    /// explicit IDs.
    #[must_use]
    pub fn derivation_inputs(&self) -> &[Box<str>] {
        &self.derivation_inputs
    }
}

/// Where an effective representation was selected.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum RepresentationSource {
    /// An inline `via` on the endpoint body/result.
    Inline,
    /// The nearest ancestor default, naming that owner's source path.
    Inherited(&'static str),
}

/// The effective representation of one direction and its provenance.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct EffectiveRepresentation {
    pub(crate) type_name: &'static str,
    pub(crate) source: RepresentationSource,
    pub(crate) blocking_fence: Option<&'static str>,
    pub(crate) shadowed_default: Option<(&'static str, &'static str)>,
}

impl EffectiveRepresentation {
    /// Returns the selected representation Rust type spelling.
    #[must_use]
    pub const fn type_name(&self) -> &'static str {
        self.type_name
    }

    /// Returns where the selection came from.
    #[must_use]
    pub const fn source(&self) -> RepresentationSource {
        self.source
    }

    /// Returns the nearest ancestor `explicit` fence that blocks inheritance
    /// for this endpoint, when one exists.
    #[must_use]
    pub const fn blocking_fence(&self) -> Option<&'static str> {
        self.blocking_fence
    }

    /// Returns the inherited default `(owner path, type)` that an inline
    /// selection overrides, when one would otherwise apply.
    #[must_use]
    pub const fn shadowed_default(&self) -> Option<(&'static str, &'static str)> {
        self.shadowed_default
    }
}

/// The effective result of one endpoint.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ResultContract {
    pub(crate) output_type: &'static str,
    pub(crate) representation: EffectiveRepresentation,
    pub(crate) driver: ResponseAcquisition,
    pub(crate) accept: Option<&'static str>,
}

impl ResultContract {
    /// Returns the exact `.await` output type spelling.
    #[must_use]
    pub const fn output_type(&self) -> &'static str {
        self.output_type
    }

    /// Returns the effective response representation.
    #[must_use]
    pub const fn representation(&self) -> &EffectiveRepresentation {
        &self.representation
    }

    /// Returns the core-owned response driver category.
    #[must_use]
    pub const fn driver(&self) -> ResponseAcquisition {
        self.driver
    }

    /// Returns the representation's static `ACCEPT` constant.
    #[must_use]
    pub const fn accept(&self) -> Option<&'static str> {
        self.accept
    }
}

/// The effective body parameter of one endpoint.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct BodyContract {
    pub(crate) spec: BodySpec,
    pub(crate) representation: EffectiveRepresentation,
    pub(crate) constructor_position: Option<usize>,
    pub(crate) declaration_index: usize,
}

impl BodyContract {
    /// Returns the body parameter name.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        self.spec.name
    }

    /// Returns the exact caller input type spelling.
    #[must_use]
    pub const fn input_type(&self) -> &'static str {
        self.spec.input_type
    }

    /// Returns body outer presence.
    #[must_use]
    pub const fn presence(&self) -> ValuePresence {
        self.spec.presence
    }

    /// Returns the effective request representation.
    #[must_use]
    pub const fn representation(&self) -> &EffectiveRepresentation {
        &self.representation
    }

    /// Returns the static body family/media description.
    #[must_use]
    pub const fn description(&self) -> BodyDescription {
        self.spec.description
    }

    /// Returns whether the page-body capability was proven.
    #[must_use]
    pub const fn page_capable(&self) -> bool {
        self.spec.page_capable
    }

    /// Returns the positional constructor index of a required body.
    #[must_use]
    pub const fn constructor_position(&self) -> Option<usize> {
        self.constructor_position
    }

    /// Returns the zero-based position among the endpoint's parameters.
    #[must_use]
    pub const fn declaration_index(&self) -> usize {
        self.declaration_index
    }

    /// Returns author documentation lines.
    #[must_use]
    pub const fn docs(&self) -> &'static [&'static str] {
        self.spec.docs
    }
}

/// Where the effective timeout came from.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum TimeoutSource {
    /// The nearest owner with a `timeout` clause.
    Declared(&'static str),
    /// No owner declared a timeout; the v3 root default applies.
    RootDefault,
}

/// The effective finite operation/traversal timeout.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct EffectiveTimeout {
    pub(crate) duration: Duration,
    pub(crate) source: TimeoutSource,
}

impl EffectiveTimeout {
    /// Returns the finite duration.
    #[must_use]
    pub const fn duration(&self) -> Duration {
        self.duration
    }

    /// Returns where it was selected.
    #[must_use]
    pub const fn source(&self) -> TimeoutSource {
        self.source
    }
}

/// One effective generated DNS label with provenance.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct RouteLabel {
    pub(crate) atom: AtomExpression,
    pub(crate) origin: &'static str,
}

impl RouteLabel {
    /// Returns the label atom.
    #[must_use]
    pub const fn atom(&self) -> &AtomExpression {
        &self.atom
    }

    /// Returns the owner that introduced the label.
    #[must_use]
    pub const fn origin(&self) -> &'static str {
        self.origin
    }
}

/// One effective generated path segment with provenance.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct RouteSegment {
    pub(crate) segment: PathSegment,
    pub(crate) origin: &'static str,
}

impl RouteSegment {
    /// Returns the segment IR.
    #[must_use]
    pub const fn segment(&self) -> &PathSegment {
        &self.segment
    }

    /// Returns the owner that introduced the segment.
    #[must_use]
    pub const fn origin(&self) -> &'static str {
        self.origin
    }
}

/// A `replace`/`off` fence that removed inherited generated route parts.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct RouteReset {
    pub(crate) owner: &'static str,
    pub(crate) off: bool,
}

impl RouteReset {
    /// Returns the owner whose clause removed inherited parts.
    #[must_use]
    pub const fn owner(&self) -> &'static str {
        self.owner
    }

    /// Returns true for `off`, false for `replace`.
    #[must_use]
    pub const fn is_off(&self) -> bool {
        self.off
    }
}

/// The effective generated route of one endpoint (under the fixed base).
#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
pub struct EffectiveRoute {
    pub(crate) labels: Box<[RouteLabel]>,
    pub(crate) segments: Box<[RouteSegment]>,
    pub(crate) subdomain_reset: Option<RouteReset>,
    pub(crate) path_reset: Option<RouteReset>,
}

impl EffectiveRoute {
    /// Returns generated DNS labels, leftmost first, above the fixed suffix.
    #[must_use]
    pub fn labels(&self) -> &[RouteLabel] {
        &self.labels
    }

    /// Returns generated path segments after the fixed base path.
    #[must_use]
    pub fn segments(&self) -> &[RouteSegment] {
        &self.segments
    }

    /// Returns the nearest subdomain `replace`/`off` fence.
    #[must_use]
    pub const fn subdomain_reset(&self) -> Option<RouteReset> {
        self.subdomain_reset
    }

    /// Returns the nearest path `replace`/`off` fence.
    #[must_use]
    pub const fn path_reset(&self) -> Option<RouteReset> {
        self.path_reset
    }
}

/// One surviving symbolic operation of an endpoint's effective head program.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct EffectiveKeyOperation {
    pub(crate) family: HeadFamily,
    pub(crate) operation: KeyOperation,
    pub(crate) origin: &'static str,
    pub(crate) sensitive: bool,
    pub(crate) conditional_on: Option<ValueRef>,
    pub(crate) superseded_when_present: Box<[ValueRef]>,
}

impl EffectiveKeyOperation {
    /// Returns the head family.
    #[must_use]
    pub const fn family(&self) -> HeadFamily {
        self.family
    }

    /// Returns the operation (with literals redacted under sensitive keys).
    #[must_use]
    pub const fn operation(&self) -> &KeyOperation {
        &self.operation
    }

    /// Returns the owner that wrote the operation.
    #[must_use]
    pub const fn origin(&self) -> &'static str {
        self.origin
    }

    /// Returns the sticky sensitivity taint of the key along the owner chain
    /// (explicit `#[sensitive]` sources or mandatory name-based redaction).
    #[must_use]
    pub const fn sensitive(&self) -> bool {
        self.sensitive
    }

    /// Returns the outer-optional value whose presence gates emission, when
    /// the emission's top-level source is optional.
    #[must_use]
    pub const fn conditional_on(&self) -> Option<ValueRef> {
        self.conditional_on
    }

    /// Returns later `when_present` sources whose presence suppresses this
    /// occurrence (an inherited fallback).
    #[must_use]
    pub fn superseded_when_present(&self) -> &[ValueRef] {
        &self.superseded_when_present
    }
}

/// Which caller terminals exist on the endpoint (CALL-03).
#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
pub struct TerminalAvailability {
    pub(crate) accepted_bytes: bool,
    pub(crate) pagination: bool,
    pub(crate) store_as: Box<[&'static str]>,
}

impl TerminalAvailability {
    /// `.await` / `.response()` always exist.
    #[must_use]
    pub const fn response(&self) -> bool {
        true
    }

    /// `.accepted_bytes()` exists.
    #[must_use]
    pub const fn accepted_bytes(&self) -> bool {
        self.accepted_bytes
    }

    /// `.pages()`, `.collect_prefix()` and `.collect_all()` exist.
    #[must_use]
    pub const fn pagination(&self) -> bool {
        self.pagination
    }

    /// Returns credential names with a `store_as_<name>()` terminal.
    #[must_use]
    pub fn store_as(&self) -> &[&'static str] {
        &self.store_as
    }
}

/// The complete resolved contract record of one generated endpoint.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EndpointContract {
    pub(crate) path: &'static str,
    pub(crate) name: &'static str,
    pub(crate) owner: &'static str,
    pub(crate) identity: EndpointIdentity,
    pub(crate) method: &'static str,
    pub(crate) effect: Effect,
    pub(crate) effect_source: EffectSource,
    pub(crate) accepted: AcceptedStatuses,
    pub(crate) status_source: StatusSource,
    pub(crate) absent_on: Option<Box<[u16]>>,
    pub(crate) timeout: EffectiveTimeout,
    pub(crate) result: ResultContract,
    pub(crate) body: Option<BodyContract>,
    pub(crate) route: EffectiveRoute,
    pub(crate) query: Box<[EffectiveKeyOperation]>,
    pub(crate) headers: Box<[EffectiveKeyOperation]>,
    pub(crate) auth: EffectiveAuth,
    pub(crate) policies: Box<[EffectivePolicy]>,
    pub(crate) pagination: Option<PaginationContract>,
    pub(crate) terminals: TerminalAvailability,
}

impl EndpointContract {
    /// Returns the endpoint source path.
    #[must_use]
    pub const fn path(&self) -> &'static str {
        self.path
    }

    /// Returns the logical endpoint name.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        self.name
    }

    /// Returns the parent owner source path.
    #[must_use]
    pub const fn owner(&self) -> &'static str {
        self.owner
    }

    /// Returns the operational identity.
    #[must_use]
    pub const fn identity(&self) -> &EndpointIdentity {
        &self.identity
    }

    /// Returns the operational endpoint ID text.
    #[must_use]
    pub fn id(&self) -> &str {
        self.identity.id.as_str()
    }

    /// Returns the method token as written.
    #[must_use]
    pub const fn method(&self) -> &'static str {
        self.method
    }

    /// Returns the effect class.
    #[must_use]
    pub const fn effect(&self) -> Effect {
        self.effect
    }

    /// Returns whether the effect was written or method-derived.
    #[must_use]
    pub const fn effect_source(&self) -> EffectSource {
        self.effect_source
    }

    /// Returns the accepted payload statuses.
    #[must_use]
    pub const fn accepted(&self) -> &AcceptedStatuses {
        &self.accepted
    }

    /// Returns whether the status clause was written or defaulted.
    #[must_use]
    pub const fn status_source(&self) -> StatusSource {
        self.status_source
    }

    /// Returns the canonical head-only absence statuses.
    #[must_use]
    pub fn absent_on(&self) -> Option<&[u16]> {
        self.absent_on.as_deref()
    }

    /// Returns the effective timeout and its source.
    #[must_use]
    pub const fn timeout(&self) -> EffectiveTimeout {
        self.timeout
    }

    /// Returns the effective result.
    #[must_use]
    pub const fn result(&self) -> &ResultContract {
        &self.result
    }

    /// Returns the effective body parameter.
    #[must_use]
    pub const fn body(&self) -> Option<&BodyContract> {
        self.body.as_ref()
    }

    /// Returns the effective generated route.
    #[must_use]
    pub const fn route(&self) -> &EffectiveRoute {
        &self.route
    }

    /// Returns the effective symbolic program of `family`.
    #[must_use]
    pub fn operations(&self, family: HeadFamily) -> &[EffectiveKeyOperation] {
        match family {
            HeadFamily::Query => &self.query,
            HeadFamily::Header => &self.headers,
        }
    }

    /// Returns the effective authentication.
    #[must_use]
    pub const fn auth(&self) -> &EffectiveAuth {
        &self.auth
    }

    /// Returns effective, deduplicated policies in attachment order.
    #[must_use]
    pub fn policies(&self) -> &[EffectivePolicy] {
        &self.policies
    }

    /// Returns the pagination declaration.
    #[must_use]
    pub const fn pagination(&self) -> Option<&PaginationContract> {
        self.pagination.as_ref()
    }

    /// Returns terminal availability.
    #[must_use]
    pub const fn terminals(&self) -> &TerminalAvailability {
        &self.terminals
    }

    /// Returns the generated route shape used by runtime descriptions:
    /// `/` followed by generated segments with holes rendered as
    /// `{owner.name}` (the fixed base path is not included).
    #[must_use]
    pub fn route_shape(&self, display: impl Fn(&ValueRef) -> String) -> String {
        let mut shape = String::from("/");
        let rendered = self
            .route
            .segments
            .iter()
            .map(|segment| super::render_segment(&segment.segment, &display))
            .collect::<Vec<_>>();
        shape.push_str(&rendered.join("/"));
        shape
    }
}
