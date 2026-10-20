//! Symbolic route, DNS and query/header operation IR (§5, §6).

use super::values::ValueRef;

/// The Rust literal category of a literal atom.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum LiteralKind {
    /// A cooked or raw string literal (decoded text is recorded).
    String,
    /// A character literal.
    Char,
    /// `true` or `false`.
    Bool,
    /// An integer literal, possibly unary-negative.
    Integer,
    /// A floating-point literal.
    Float,
}

impl LiteralKind {
    /// Returns the stable lowercase label used by structural exports.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::String => "string",
            Self::Char => "char",
            Self::Bool => "bool",
            Self::Integer => "integer",
            Self::Float => "float",
        }
    }
}

/// The retained text of a literal atom.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum LiteralText {
    /// Ordinary nonsensitive declared literal text.
    Text(&'static str),
    /// A literal emitted under a sensitive key. The text is not retained;
    /// the classification and structural location remain (§18.2).
    Redacted,
}

/// One symbolic scalar atom expression (§5.1).
///
/// Nested concatenations are flattened by the description build, so a built
/// description never contains a `Concat` directly inside another `Concat`.
#[non_exhaustive]
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum AtomExpression {
    /// A literal atom.
    Literal {
        /// Rust literal category.
        kind: LiteralKind,
        /// Retained or redacted text.
        text: LiteralText,
    },
    /// One exact value reference, optionally through an `AtomEncoder` adapter.
    Value {
        /// The referenced value.
        value: ValueRef,
        /// The adapter Rust type spelling from `via`, when written.
        adapter: Option<&'static str>,
    },
    /// `concat [...]`: one scalar atom made of ordered total parts.
    Concat(Box<[AtomExpression]>),
}

impl AtomExpression {
    /// A string literal atom.
    #[must_use]
    pub const fn string(text: &'static str) -> Self {
        Self::literal(LiteralKind::String, text)
    }

    /// A literal atom of the given category, recorded as written.
    #[must_use]
    pub const fn literal(kind: LiteralKind, text: &'static str) -> Self {
        Self::Literal {
            kind,
            text: LiteralText::Text(text),
        }
    }

    /// A direct value atom using the value's `RequestAtom` role.
    #[must_use]
    pub const fn value(value: ValueRef) -> Self {
        Self::Value {
            value,
            adapter: None,
        }
    }

    /// A value atom through an explicit `AtomEncoder` adapter type.
    #[must_use]
    pub const fn value_via(value: ValueRef, adapter: &'static str) -> Self {
        Self::Value {
            value,
            adapter: Some(adapter),
        }
    }

    /// `concat [parts...]`.
    #[must_use]
    pub fn concat(parts: Vec<AtomExpression>) -> Self {
        Self::Concat(parts.into_boxed_slice())
    }

    pub(crate) fn visit_refs(&self, visit: &mut impl FnMut(&ValueRef)) {
        match self {
            Self::Literal { .. } => {}
            Self::Value { value, .. } => visit(value),
            Self::Concat(parts) => {
                for part in parts {
                    part.visit_refs(visit);
                }
            }
        }
    }

    pub(crate) fn visit_refs_mut(&mut self, visit: &mut impl FnMut(&mut ValueRef)) {
        match self {
            Self::Literal { .. } => {}
            Self::Value { value, .. } => visit(value),
            Self::Concat(parts) => {
                for part in parts.iter_mut() {
                    part.visit_refs_mut(visit);
                }
            }
        }
    }

    pub(crate) fn visit_adapters(&self, visit: &mut impl FnMut(&'static str)) {
        match self {
            Self::Literal { .. } => {}
            Self::Value { adapter, .. } => {
                if let Some(adapter) = adapter {
                    visit(adapter);
                }
            }
            Self::Concat(parts) => {
                for part in parts {
                    part.visit_adapters(visit);
                }
            }
        }
    }

    pub(crate) fn redact_literals(&mut self) -> usize {
        match self {
            Self::Literal { text, .. } => {
                *text = LiteralText::Redacted;
                1
            }
            Self::Value { .. } => 0,
            Self::Concat(parts) => parts.iter_mut().map(Self::redact_literals).sum(),
        }
    }

    pub(crate) fn flatten(self) -> Self {
        match self {
            Self::Concat(parts) => {
                let mut flat = Vec::with_capacity(parts.len());
                for part in parts.into_vec() {
                    match part.flatten() {
                        Self::Concat(nested) => flat.extend(nested.into_vec()),
                        other => flat.push(other),
                    }
                }
                Self::Concat(flat.into_boxed_slice())
            }
            other => other,
        }
    }
}

/// One part of a relative path segment (§5.3).
#[non_exhaustive]
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum TemplatePart {
    /// Decoded literal data (brace escapes already decoded).
    Literal(&'static str),
    /// `{reference}` or `{reference via Adapter}`.
    Hole {
        /// The referenced total value.
        value: ValueRef,
        /// The adapter Rust type spelling, when written.
        adapter: Option<&'static str>,
    },
}

impl TemplatePart {
    /// A decoded literal part.
    #[must_use]
    pub const fn literal(text: &'static str) -> Self {
        Self::Literal(text)
    }

    /// A `{reference}` hole.
    #[must_use]
    pub const fn hole(value: ValueRef) -> Self {
        Self::Hole {
            value,
            adapter: None,
        }
    }

    /// A `{reference via Adapter}` hole.
    #[must_use]
    pub const fn hole_via(value: ValueRef, adapter: &'static str) -> Self {
        Self::Hole {
            value,
            adapter: Some(adapter),
        }
    }
}

/// One generated path segment: a bounded concatenated atom of its parts.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct PathSegment {
    parts: Box<[TemplatePart]>,
}

impl PathSegment {
    /// A segment made of ordered parts.
    #[must_use]
    pub fn new(parts: Vec<TemplatePart>) -> Self {
        Self {
            parts: parts.into_boxed_slice(),
        }
    }

    /// A literal-only segment.
    #[must_use]
    pub fn literal(text: &'static str) -> Self {
        Self::new(vec![TemplatePart::literal(text)])
    }

    /// A segment made of one hole.
    #[must_use]
    pub fn hole(value: ValueRef) -> Self {
        Self::new(vec![TemplatePart::hole(value)])
    }

    /// Returns the ordered parts.
    #[must_use]
    pub fn parts(&self) -> &[TemplatePart] {
        &self.parts
    }

    pub(crate) fn parts_mut(&mut self) -> &mut [TemplatePart] {
        &mut self.parts
    }
}

/// A relative path template: its written text and its segment IR.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct PathTemplate {
    text: &'static str,
    segments: Box<[PathSegment]>,
}

impl PathTemplate {
    /// A template whose decoded source text is `text` and whose IR is
    /// `segments`. The description build validates segment structure; it does
    /// not reparse `text`.
    #[must_use]
    pub fn new(text: &'static str, segments: Vec<PathSegment>) -> Self {
        Self {
            text,
            segments: segments.into_boxed_slice(),
        }
    }

    /// Returns the template source text as written.
    #[must_use]
    pub const fn text(&self) -> &'static str {
        self.text
    }

    /// Returns the ordered segments.
    #[must_use]
    pub fn segments(&self) -> &[PathSegment] {
        &self.segments
    }

    pub(crate) fn segments_mut(&mut self) -> &mut [PathSegment] {
        &mut self.segments
    }
}

/// One owner's local `path` clause (PATH-02).
#[non_exhaustive]
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum PathOperation {
    /// `path "x"`: append to inherited generated segments.
    Append(PathTemplate),
    /// `path replace "x"`: drop inherited generated segments, then append.
    Replace(PathTemplate),
    /// `path off`: drop inherited generated segments.
    Off,
}

impl PathOperation {
    pub(crate) fn template_mut(&mut self) -> Option<&mut PathTemplate> {
        match self {
            Self::Append(template) | Self::Replace(template) => Some(template),
            Self::Off => None,
        }
    }

    pub(crate) fn template(&self) -> Option<&PathTemplate> {
        match self {
            Self::Append(template) | Self::Replace(template) => Some(template),
            Self::Off => None,
        }
    }
}

/// One owner's local `subdomain` clause (ORIGIN-02).
#[non_exhaustive]
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum SubdomainOperation {
    /// `subdomain [...]`: prepend labels to inherited generated labels.
    Prepend(Box<[AtomExpression]>),
    /// `subdomain replace [...]`: drop inherited generated labels first.
    Replace(Box<[AtomExpression]>),
    /// `subdomain off`: drop inherited generated labels.
    Off,
}

impl SubdomainOperation {
    /// `subdomain [labels...]`.
    #[must_use]
    pub fn prepend(labels: Vec<AtomExpression>) -> Self {
        Self::Prepend(labels.into_boxed_slice())
    }

    /// `subdomain replace [labels...]`.
    #[must_use]
    pub fn replace(labels: Vec<AtomExpression>) -> Self {
        Self::Replace(labels.into_boxed_slice())
    }

    pub(crate) fn labels(&self) -> &[AtomExpression] {
        match self {
            Self::Prepend(labels) | Self::Replace(labels) => labels,
            Self::Off => &[],
        }
    }

    pub(crate) fn labels_mut(&mut self) -> &mut [AtomExpression] {
        match self {
            Self::Prepend(labels) | Self::Replace(labels) => labels,
            Self::Off => &mut [],
        }
    }
}

/// A public head family.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum HeadFamily {
    /// Ordered query pairs; keys are case-sensitive.
    Query,
    /// Public headers; names compare ASCII case-insensitively.
    Header,
}

impl HeadFamily {
    /// Returns the stable lowercase label used by structural exports.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Query => "query",
            Self::Header => "header",
        }
    }
}

/// An owner's family-level mode (§6.1).
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum FamilyMode {
    /// Start from the inherited symbolic program.
    #[default]
    Inherit,
    /// `query replace { ... }` / `headers replace { ... }`: clear the inherited
    /// emission program before local statements.
    Replace,
    /// `query off;` / `headers off;`: clear the family with no local emission.
    Off,
}

impl FamilyMode {
    /// Returns the stable lowercase label used by structural exports.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Inherit => "inherit",
            Self::Replace => "replace",
            Self::Off => "off",
        }
    }
}

/// A keyed patch operation (§6.2).
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum KeyOperationKind {
    /// `"k" = e`: plain introduction; no prior potential owner may exist.
    Introduce,
    /// `replace "k" = e`: remove previous occurrences, emit here.
    Replace,
    /// `when_present "k" = e`: replace only when the outer-optional source is
    /// present; otherwise retain prior occurrences.
    WhenPresent,
    /// `append "k" = e`: append occurrences.
    Append,
    /// `remove "k"`: remove all earlier occurrences.
    Remove,
}

impl KeyOperationKind {
    /// Returns the stable lowercase label used by structural exports.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Introduce => "introduce",
            Self::Replace => "replace",
            Self::WhenPresent => "when_present",
            Self::Append => "append",
            Self::Remove => "remove",
        }
    }
}

/// Where a keyed operation was written.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum OperationSite {
    /// A direct `query`/`header(...)` parameter or value placement.
    DirectPlacement,
    /// A statement in an explicit `query { }` / `headers { }` block.
    Block,
}

impl OperationSite {
    /// Returns the stable lowercase label used by structural exports.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::DirectPlacement => "direct",
            Self::Block => "block",
        }
    }
}

/// The emission of one keyed operation.
#[non_exhaustive]
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum Emission {
    /// One scalar occurrence. A top-level value reference to an optional value
    /// makes the occurrence conditional on outer presence.
    Scalar(AtomExpression),
    /// `each reference [via Adapter]`: zero or more occurrences in the
    /// value's stable order; the adapter applies per item.
    Each {
        /// The ordered-collection value.
        value: ValueRef,
        /// Per-item adapter type spelling, when written.
        adapter: Option<&'static str>,
    },
}

impl Emission {
    /// A scalar emission of an atom expression.
    #[must_use]
    pub const fn scalar(atom: AtomExpression) -> Self {
        Self::Scalar(atom)
    }

    /// A scalar emission of one value reference.
    #[must_use]
    pub const fn value(value: ValueRef) -> Self {
        Self::Scalar(AtomExpression::value(value))
    }

    /// A string literal scalar emission.
    #[must_use]
    pub const fn literal(text: &'static str) -> Self {
        Self::Scalar(AtomExpression::string(text))
    }

    /// `each reference`.
    #[must_use]
    pub const fn each(value: ValueRef) -> Self {
        Self::Each {
            value,
            adapter: None,
        }
    }

    /// `each reference via Adapter`.
    #[must_use]
    pub const fn each_via(value: ValueRef, adapter: &'static str) -> Self {
        Self::Each {
            value,
            adapter: Some(adapter),
        }
    }

    /// Returns the top-level presence source: the directly referenced value of
    /// a scalar value atom or an `each` emission.
    #[must_use]
    pub const fn presence_source(&self) -> Option<&ValueRef> {
        match self {
            Self::Scalar(AtomExpression::Value { value, .. }) | Self::Each { value, .. } => {
                Some(value)
            }
            Self::Scalar(_) => None,
        }
    }

    pub(crate) fn visit_refs(&self, visit: &mut impl FnMut(&ValueRef)) {
        match self {
            Self::Scalar(atom) => atom.visit_refs(visit),
            Self::Each { value, .. } => visit(value),
        }
    }

    pub(crate) fn visit_adapters(&self, visit: &mut impl FnMut(&'static str)) {
        match self {
            Self::Scalar(atom) => atom.visit_adapters(visit),
            Self::Each { adapter, .. } => {
                if let Some(adapter) = adapter {
                    visit(adapter);
                }
            }
        }
    }
}

/// One keyed query/header operation as written at one owner.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct KeyOperation {
    kind: KeyOperationKind,
    key: &'static str,
    emission: Option<Emission>,
    site: OperationSite,
}

impl KeyOperation {
    /// A direct placement (`query name: T`, `header("K") name: T`): a plain
    /// introduction written as a parameter placement.
    #[must_use]
    pub const fn direct(key: &'static str, emission: Emission) -> Self {
        Self {
            kind: KeyOperationKind::Introduce,
            key,
            emission: Some(emission),
            site: OperationSite::DirectPlacement,
        }
    }

    /// Block statement `"k" = e;`.
    #[must_use]
    pub const fn introduce(key: &'static str, emission: Emission) -> Self {
        Self::block(KeyOperationKind::Introduce, key, emission)
    }

    /// Block statement `replace "k" = e;`.
    #[must_use]
    pub const fn replace(key: &'static str, emission: Emission) -> Self {
        Self::block(KeyOperationKind::Replace, key, emission)
    }

    /// Block statement `when_present "k" = e;`.
    #[must_use]
    pub const fn when_present(key: &'static str, emission: Emission) -> Self {
        Self::block(KeyOperationKind::WhenPresent, key, emission)
    }

    /// Block statement `append "k" = e;`.
    #[must_use]
    pub const fn append(key: &'static str, emission: Emission) -> Self {
        Self::block(KeyOperationKind::Append, key, emission)
    }

    /// Block statement `remove "k";`.
    #[must_use]
    pub const fn remove(key: &'static str) -> Self {
        Self {
            kind: KeyOperationKind::Remove,
            key,
            emission: None,
            site: OperationSite::Block,
        }
    }

    const fn block(kind: KeyOperationKind, key: &'static str, emission: Emission) -> Self {
        Self {
            kind,
            key,
            emission: Some(emission),
            site: OperationSite::Block,
        }
    }

    /// Returns the operation kind.
    #[must_use]
    pub const fn kind(&self) -> KeyOperationKind {
        self.kind
    }

    /// Returns the static target key exactly as written.
    #[must_use]
    pub const fn key(&self) -> &'static str {
        self.key
    }

    /// Returns the emission (absent for `remove`).
    #[must_use]
    pub const fn emission(&self) -> Option<&Emission> {
        self.emission.as_ref()
    }

    /// Returns where the operation was written.
    #[must_use]
    pub const fn site(&self) -> OperationSite {
        self.site
    }

    pub(crate) fn emission_mut(&mut self) -> Option<&mut Emission> {
        self.emission.as_mut()
    }
}

/// An owner's representation default for one direction (§9.1).
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum RepresentationSelection {
    /// `request T;` / `response T;`: select a representation type.
    Type(&'static str),
    /// `request explicit;` / `response explicit;`: a fence removing the
    /// inherited default for descendants.
    ExplicitFence,
}
