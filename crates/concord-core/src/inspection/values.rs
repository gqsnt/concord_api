//! Owner identity, value declarations and exact value references.

/// The kind of lexical owner that declares values and attachments.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum OwnerKind {
    /// The generated client root.
    Client,
    /// A lexical scope.
    Scope,
    /// An endpoint leaf.
    Endpoint,
}

impl OwnerKind {
    /// Returns the stable lowercase label used by structural exports.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Client => "client",
            Self::Scope => "scope",
            Self::Endpoint => "endpoint",
        }
    }
}

/// Author deprecation metadata copied from `#[deprecated]`.
///
/// Both fields are author-supplied static text. The description never derives
/// deprecation from naming or liveness analysis.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct Deprecation {
    since: Option<&'static str>,
    note: Option<&'static str>,
}

impl Deprecation {
    /// A bare `#[deprecated]` without `since` or `note`.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            since: None,
            note: None,
        }
    }

    /// Records the author `since` text.
    #[must_use]
    pub const fn since(mut self, since: &'static str) -> Self {
        self.since = Some(since);
        self
    }

    /// Records the author `note` text.
    #[must_use]
    pub const fn note(mut self, note: &'static str) -> Self {
        self.note = Some(note);
        self
    }

    /// Returns the author `since` text, when written.
    #[must_use]
    pub const fn since_text(&self) -> Option<&'static str> {
        self.since
    }

    /// Returns the author `note` text, when written.
    #[must_use]
    pub const fn note_text(&self) -> Option<&'static str> {
        self.note
    }
}

/// How a value reference was written, or its resolved owner.
///
/// Builders accept the DSL spellings ([`ReferenceQualifier::Local`],
/// [`ReferenceQualifier::Client`], [`ReferenceQualifier::Scope`]); a built
/// [`ContractDescription`](super::ContractDescription) contains only
/// [`ReferenceQualifier::Owner`] references naming the exact owner path.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ReferenceQualifier {
    /// A bare name, which binds only the value owner where it is written.
    Local,
    /// `client.name`.
    Client,
    /// `scope_ident.name` for one visible ancestor scope.
    Scope(&'static str),
    /// An exact resolved owner source path such as `CatalogApi::regional`.
    Owner(&'static str),
}

/// A reference to exactly one declared ordinary value.
///
/// A reference is structural metadata, never the referenced runtime value.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ValueRef {
    qualifier: ReferenceQualifier,
    name: &'static str,
}

impl ValueRef {
    /// A bare reference to a value of the owner where the reference is
    /// written (LANG-03).
    #[must_use]
    pub const fn local(name: &'static str) -> Self {
        Self {
            qualifier: ReferenceQualifier::Local,
            name,
        }
    }

    /// A `client.name` reference from a scope or endpoint.
    #[must_use]
    pub const fn client(name: &'static str) -> Self {
        Self {
            qualifier: ReferenceQualifier::Client,
            name,
        }
    }

    /// A `scope.name` reference to one visible ancestor scope.
    #[must_use]
    pub const fn scope(scope: &'static str, name: &'static str) -> Self {
        Self {
            qualifier: ReferenceQualifier::Scope(scope),
            name,
        }
    }

    /// An already resolved reference naming the exact owner source path.
    ///
    /// The owner must be the writing owner or one of its ancestors.
    #[must_use]
    pub const fn exact(owner_path: &'static str, name: &'static str) -> Self {
        Self {
            qualifier: ReferenceQualifier::Owner(owner_path),
            name,
        }
    }

    /// Returns how the reference was written or resolved.
    #[must_use]
    pub const fn qualifier(&self) -> ReferenceQualifier {
        self.qualifier
    }

    /// Returns the exact owner source path once resolved.
    ///
    /// Every reference inside a built contract description returns `Some`.
    #[must_use]
    pub const fn owner_path(&self) -> Option<&'static str> {
        match self.qualifier {
            ReferenceQualifier::Owner(path) => Some(path),
            _ => None,
        }
    }

    /// Returns the declared value name.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        self.name
    }
}

/// Concord-owned outer presence, independent of Rust type spelling.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ValuePresence {
    /// The declaration adds no outer presence layer.
    Required,
    /// The declaration adds exactly one outer `Option` layer (`name?: T`).
    Optional,
}

/// Observation classification of an ordinary public value (§6.4).
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum Sensitivity {
    /// The value may appear in credential-free public observation.
    Ordinary,
    /// `#[sensitive]`: sent on the wire but withheld from automatic
    /// observation, plans, errors and structural exports.
    Sensitive,
}

/// Semantic liveness of a declared value (VALUE-06).
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ValueLiveness {
    /// The value has a reachable semantic use in the declaration.
    Live,
    /// The value is used only by its owner's supported runtime binding
    /// projection.
    RuntimeProjectionOnly,
    /// An explicitly deprecated optional or initialized input retained as a
    /// no-op value. It has no semantic use.
    DeprecatedNoOp,
}

/// A stable initializer expression identity (§3.1 `stable_value`).
///
/// The description records the written identity, never an evaluated value.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum InitializerExpression {
    /// A Rust literal or unary-negative numeric literal, as written.
    Literal(&'static str),
    /// A Rust constant or associated-constant path, as written. Its value is
    /// an opaque dependency.
    Constant(&'static str),
    /// The literal initializer of a sensitive value. The written text is not
    /// retained; comparisons involving it are unresolved.
    Redacted,
}

/// The declaration initializer category of one value.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ValueInitializer {
    /// No initializer: the caller must supply the value (or it is absent).
    None,
    /// `= stable_value`, converted through the closed ordinary conversions
    /// once at the owner boundary.
    StableValue(InitializerExpression),
    /// `#[default]`: the payload's `Default` implementation, evaluated once at
    /// the owner boundary.
    TraitDefault,
}

/// How a value is supplied by callers (§4.3).
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ValueRole {
    /// A required, uninitialized scope or endpoint parameter: a positional
    /// constructor argument without setters.
    Constructor,
    /// A required, uninitialized root value supplied through the builder.
    BuilderRequired,
    /// An optional and/or initialized value controlled through setters.
    Configurable,
}

/// One generated value control from the §4.3 mutation matrix.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ValueControl {
    /// `set_x(impl IntoValue<T>)`.
    Set,
    /// `replace_x(Option<T>)`.
    Replace,
    /// `set_x_if_some(Option<T>)`.
    SetIfSome,
    /// `omit_x()`.
    Omit,
    /// `reset_x()`.
    Reset,
}

impl ValueControl {
    /// Returns the stable lowercase label used by structural exports.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Set => "set",
            Self::Replace => "replace",
            Self::SetIfSome => "set_if_some",
            Self::Omit => "omit",
            Self::Reset => "reset",
        }
    }
}

/// Author input describing one ordinary value declaration.
///
/// This is the generated-code bridge input for [`ValueDescription`]. It holds
/// only static declaration text; building a description never evaluates the
/// initializer, converts, encodes or clones any runtime value.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ValueSpec {
    pub(crate) name: &'static str,
    pub(crate) payload_type: &'static str,
    pub(crate) presence: ValuePresence,
    pub(crate) initializer: ValueInitializer,
    pub(crate) sensitivity: Sensitivity,
    pub(crate) liveness: ValueLiveness,
    pub(crate) docs: &'static [&'static str],
    pub(crate) deprecation: Option<Deprecation>,
}

impl ValueSpec {
    /// A required, uninitialized, ordinary, live value `name: payload_type`.
    #[must_use]
    pub const fn new(name: &'static str, payload_type: &'static str) -> Self {
        Self {
            name,
            payload_type,
            presence: ValuePresence::Required,
            initializer: ValueInitializer::None,
            sensitivity: Sensitivity::Ordinary,
            liveness: ValueLiveness::Live,
            docs: &[],
            deprecation: None,
        }
    }

    /// Marks the declaration `name?: T`.
    #[must_use]
    pub const fn optional(mut self) -> Self {
        self.presence = ValuePresence::Optional;
        self
    }

    /// Records `= literal` as written.
    #[must_use]
    pub const fn stable_literal(mut self, text: &'static str) -> Self {
        self.initializer = ValueInitializer::StableValue(InitializerExpression::Literal(text));
        self
    }

    /// Records `= CONSTANT_PATH` as written.
    #[must_use]
    pub const fn stable_constant(mut self, path: &'static str) -> Self {
        self.initializer = ValueInitializer::StableValue(InitializerExpression::Constant(path));
        self
    }

    /// Records `#[default]`.
    #[must_use]
    pub const fn trait_default(mut self) -> Self {
        self.initializer = ValueInitializer::TraitDefault;
        self
    }

    /// Records `#[sensitive]`.
    #[must_use]
    pub const fn sensitive(mut self) -> Self {
        self.sensitivity = Sensitivity::Sensitive;
        self
    }

    /// Records that the value is used only by its owner's runtime projection.
    #[must_use]
    pub const fn runtime_projection_only(mut self) -> Self {
        self.liveness = ValueLiveness::RuntimeProjectionOnly;
        self
    }

    /// Records an explicitly deprecated no-op value.
    ///
    /// The description build rejects this unless deprecation is also
    /// recorded and the value is not a required constructor input.
    #[must_use]
    pub const fn deprecated_no_op(mut self) -> Self {
        self.liveness = ValueLiveness::DeprecatedNoOp;
        self
    }

    /// Records author documentation lines in source order.
    #[must_use]
    pub const fn docs(mut self, docs: &'static [&'static str]) -> Self {
        self.docs = docs;
        self
    }

    /// Records author deprecation.
    #[must_use]
    pub const fn deprecated(mut self, deprecation: Deprecation) -> Self {
        self.deprecation = Some(deprecation);
        self
    }
}

/// One resolved, value-free ordinary value declaration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValueDescription {
    pub(crate) owner: &'static str,
    pub(crate) owner_kind: OwnerKind,
    pub(crate) spec: ValueSpec,
    pub(crate) declaration_index: usize,
    pub(crate) constructor_position: Option<usize>,
}

impl ValueDescription {
    /// Returns the exact owner source path.
    #[must_use]
    pub const fn owner(&self) -> &'static str {
        self.owner
    }

    /// Returns the declaring owner kind.
    #[must_use]
    pub const fn owner_kind(&self) -> OwnerKind {
        self.owner_kind
    }

    /// Returns the logical value name.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        self.spec.name
    }

    /// Returns a reference naming this exact value.
    #[must_use]
    pub const fn reference(&self) -> ValueRef {
        ValueRef::exact(self.owner, self.spec.name)
    }

    /// Returns the payload type exactly as written.
    #[must_use]
    pub const fn payload_type(&self) -> &'static str {
        self.spec.payload_type
    }

    /// Returns Concord-owned outer presence.
    #[must_use]
    pub const fn presence(&self) -> ValuePresence {
        self.spec.presence
    }

    /// Returns the initializer category and expression identity.
    #[must_use]
    pub const fn initializer(&self) -> ValueInitializer {
        self.spec.initializer
    }

    /// Returns the observation classification.
    #[must_use]
    pub const fn sensitivity(&self) -> Sensitivity {
        self.spec.sensitivity
    }

    /// Returns semantic liveness.
    #[must_use]
    pub const fn liveness(&self) -> ValueLiveness {
        self.spec.liveness
    }

    /// Returns the zero-based declaration order within the owner.
    #[must_use]
    pub const fn declaration_index(&self) -> usize {
        self.declaration_index
    }

    /// Returns the zero-based positional constructor argument index for
    /// [`ValueRole::Constructor`] values, counted with any required body in
    /// declaration order.
    #[must_use]
    pub const fn constructor_position(&self) -> Option<usize> {
        self.constructor_position
    }

    /// Returns how callers supply the value.
    #[must_use]
    pub const fn role(&self) -> ValueRole {
        value_role(self.owner_kind, self.spec.presence, self.spec.initializer)
    }

    /// Returns the generated value controls from the §4.3 matrix.
    #[must_use]
    pub const fn controls(&self) -> &'static [ValueControl] {
        value_controls(self.owner_kind, self.spec.presence, self.spec.initializer)
    }

    /// Returns author documentation lines in source order.
    #[must_use]
    pub const fn docs(&self) -> &'static [&'static str] {
        self.spec.docs
    }

    /// Returns author deprecation, when declared.
    #[must_use]
    pub const fn deprecation(&self) -> Option<Deprecation> {
        self.spec.deprecation
    }
}

pub(crate) const fn value_role(
    owner: OwnerKind,
    presence: ValuePresence,
    initializer: ValueInitializer,
) -> ValueRole {
    match (presence, initializer, owner) {
        (ValuePresence::Required, ValueInitializer::None, OwnerKind::Client) => {
            ValueRole::BuilderRequired
        }
        (ValuePresence::Required, ValueInitializer::None, _) => ValueRole::Constructor,
        _ => ValueRole::Configurable,
    }
}

pub(crate) const fn value_controls(
    owner: OwnerKind,
    presence: ValuePresence,
    initializer: ValueInitializer,
) -> &'static [ValueControl] {
    let initialized = !matches!(initializer, ValueInitializer::None);
    match (presence, initialized, owner) {
        (ValuePresence::Required, false, OwnerKind::Client) => {
            &[ValueControl::Set, ValueControl::SetIfSome]
        }
        (ValuePresence::Required, false, _) => &[],
        (ValuePresence::Required, true, _) => &[
            ValueControl::Set,
            ValueControl::SetIfSome,
            ValueControl::Reset,
        ],
        (ValuePresence::Optional, false, _) => &[
            ValueControl::Set,
            ValueControl::Replace,
            ValueControl::SetIfSome,
            ValueControl::Omit,
        ],
        (ValuePresence::Optional, true, _) => &[
            ValueControl::Set,
            ValueControl::Replace,
            ValueControl::SetIfSome,
            ValueControl::Omit,
            ValueControl::Reset,
        ],
    }
}
