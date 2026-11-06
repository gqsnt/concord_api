//! Name resolution, validation and per-endpoint composition.
//!
//! Everything that decides *which* contributions reach an endpoint's request happens here, at
//! compile time. Generated code only decides, with plain `if let`s, whether an optional value is
//! present.

use std::collections::{BTreeMap, HashMap};

use proc_macro2::Span;
use quote::{ToTokens, format_ident};
use syn::spanned::Spanned;
use syn::{Attribute, Expr, Ident, LitStr, Type, Visibility};

use crate::syntax::{self, Family, Format, Op, OwnerKind, Rhs, type_text};
use crate::template::{self, RawPiece};

pub struct Model {
    pub owners: Vec<Owner>,
    /// Native `HeaderName` paths written as header keys, one per occurrence.
    pub key_paths: Vec<KeyPath>,
}

/// A query or header key after resolution.
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub enum KeyName {
    /// A query key as written, or a header name as normalised by `http::HeaderName::from_bytes`.
    Lit(String),
    /// A native `HeaderName` constant or static (index into [`Model::key_paths`], one per
    /// occurrence). The macro cannot evaluate it: it equals no other key at compile time, whatever
    /// its tokens; the names are compared when the request is built.
    Path(usize),
}

impl KeyName {
    pub fn lit(&self) -> Option<&str> {
        match self {
            KeyName::Lit(s) => Some(s),
            KeyName::Path(_) => None,
        }
    }
}

/// A header key written as a Rust path.
pub struct KeyPath {
    pub path: syn::Path,
    /// The path's tokens, as written (for documentation and error labels).
    pub text: String,
    /// The owner whose header contribution holds this occurrence, set when the contribution is
    /// recorded. Each occurrence belongs to exactly one contribution of one owner (inheriting
    /// owners reach it through their chain, not as a copy); `None` only when the declaration
    /// failed before its contribution was recorded.
    pub owner: Option<usize>,
}

/// Whether two keys name the same header, as far as the macro can tell.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Rel {
    Same,
    Different,
    /// At least one is a native constant: decided by `HeaderName == HeaderName` at run time.
    Unknown,
}

pub fn relation(a: &KeyName, b: &KeyName) -> Rel {
    match (a, b) {
        (KeyName::Lit(x), KeyName::Lit(y)) => {
            if x == y {
                Rel::Same
            } else {
                Rel::Different
            }
        }
        (KeyName::Path(x), KeyName::Path(y)) if x == y => Rel::Same,
        _ => Rel::Unknown,
    }
}

/// Whether a value sent under `ka`, with the run-time conditions `set`, is already withheld
/// whenever `ka == kb` and a condition `t` of a contribution under `kb` holds: `t` with `kb`
/// replaced by `ka` (equivalent when `ka == kb`) is in `set`, or cannot hold.
fn implied(t: &Cond, ka: &KeyName, kb: &KeyName, set: &std::collections::BTreeSet<Cond>) -> bool {
    let swap = |k: &KeyName| if k == kb { ka.clone() } else { k.clone() };
    match t {
        Cond::Name(p) => match swap(&KeyName::Path(*p)) {
            KeyName::Path(q) => set.contains(&Cond::Name(q)),
            // `ka` is a literal: on the list, its values are withheld statically and never get
            // here; off the list, the condition cannot hold.
            KeyName::Lit(_) => true,
        },
        Cond::Same(x, y) => {
            let (x, y) = (swap(x), swap(y));
            match relation(&x, &y) {
                Rel::Same => false,
                Rel::Different => true,
                Rel::Unknown => {
                    let (x, y) = if x <= y { (x, y) } else { (y, x) };
                    set.contains(&Cond::Same(x, y))
                }
            }
        }
    }
}

/// A run-time condition under which a value is withheld from Concord's output (it can only arise
/// from native header names).
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum Cond {
    /// The native constant `key_paths[_]` is on the safety list.
    Name(usize),
    /// The two keys name the same header.
    Same(KeyName, KeyName),
}

// Few owners per declaration; boxing the larger variant buys nothing.
#[allow(clippy::large_enum_variant)]
pub enum Kind {
    Client {
        vis: Visibility,
        base: LitStr,
        /// `send P;`: the send policy of every generated `send()`.
        send: Option<Type>,
    },
    Scope,
    Endpoint {
        method: Ident,
        output: Option<Output>,
        /// `-> stream` without an item: `send()` returns the unread `Incoming` body.
        incoming: bool,
        body: Option<Body>,
    },
}

/// Where a body's or output's format (or a body's media type) comes from.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    /// The endpoint's own `request`/`response`/`format` statement.
    Endpoint,
    /// The `request`/`response` statement of this client or scope (the nearest on the chain).
    Default(usize),
}

pub struct Body {
    pub position: usize,
    pub docs: Vec<Attribute>,
    pub name: Ident,
    pub optional: bool,
    /// The payload type the caller supplies.
    pub ty: Type,
    /// Its format (`Encode<ty>`) and where it comes from.
    pub format: Format,
    pub origin: Origin,
    /// A declared media type (`request F "…";`, on the endpoint or the nearest owner that states
    /// the request format) and where it is written; `None`: the format's `Encode::CONTENT_TYPE`.
    pub media: Option<(LitStr, Origin)>,
}

/// A typed output `-> T` (format `Decode<ty>`) or `-> stream T` (format `Incremental<ty>`): its
/// format and where it comes from.
pub struct Output {
    pub ty: Type,
    pub format: Format,
    pub origin: Origin,
    /// `-> stream T`: opened incrementally (`Incremental`), not decoded.
    pub stream: bool,
}

/// A `request`/`response` default declared on a client or scope.
pub struct OwnerDefault {
    pub format: Format,
    /// `request F "…";`
    pub media: Option<LitStr>,
    /// The hidden type alias through which consuming endpoints name the format, so that a
    /// missing or feature-disabled format is reported once, at the default.
    pub alias: Ident,
    /// Endpoints whose `body x: T` / bare `-> T` resolve to this default, in declaration order.
    /// Without consumers no alias is emitted and the format is never referenced.
    pub consumers: Vec<usize>,
}

pub struct Owner {
    pub kind: Kind,
    pub docs: Vec<Attribute>,
    /// Own and inherited `#[cfg]`s (applied to every generated item of the owner).
    pub cfgs: Vec<Attribute>,
    /// Own `#[cfg]`s (applied to the parent's navigation method).
    pub own_cfgs: Vec<Attribute>,
    pub deprecated: Option<Attribute>,
    pub name: Ident,
    pub type_name: Ident,
    pub state_name: Ident,
    pub dotted: String,
    pub parent: Option<usize>,
    pub children: Vec<usize>,
    pub values: Vec<Value>,
    pub labels: Vec<Component>,
    pub segments: Vec<Component>,
    pub trailing_slash: bool,
    pub query: Vec<Contrib>,
    pub headers: Vec<Contrib>,
    pub composed: Option<Composed>,
    /// `request FORMAT;` declared here (clients and scopes).
    pub request: Option<OwnerDefault>,
    /// `response FORMAT;` declared here (clients and scopes).
    pub response: Option<OwnerDefault>,
}

impl Owner {
    /// The generated struct holding a client's required values (`{Client}Config`).
    pub fn config_name(&self) -> Ident {
        format_ident!("{}Config", self.type_name, span = self.type_name.span())
    }

    pub fn is_endpoint(&self) -> bool {
        matches!(self.kind, Kind::Endpoint { .. })
    }

    pub fn body(&self) -> Option<&Body> {
        match &self.kind {
            Kind::Endpoint { body, .. } => body.as_ref(),
            _ => None,
        }
    }
}

pub struct Value {
    pub docs: Vec<Attribute>,
    pub name: Ident,
    pub ty: Type,
    pub optional: bool,
    pub default: Option<Expr>,
    pub sensitive: bool,
    pub usage: Option<Usage>,
    /// Direct placement: family, explicit key, span, `each`.
    pub placement: Option<(Family, Option<syntax::Key>, Span, bool)>,
    /// Effective sensitivity: marked `#[sensitive]`, or feeding a query/header key that is
    /// sensitive on some chain (marked contribution or safety-list name). Withheld from every
    /// Concord-produced output, and makes every placement that reads it sensitive.
    pub debug_sensitive: bool,
    /// Run-time conditions under which the value is also withheld (native header names); any of
    /// them suffices. Only values read solely as header values have some: a value read as text
    /// with a condition is withheld statically (`debug_sensitive`).
    pub debug_when: Vec<Cond>,
    /// Read as text somewhere (path, subdomain, query, or a template hole), so it must be
    /// `Display`. A value read only as whole header values needs `ToHeaderValue` instead, and its
    /// generated `Debug` shows its header value.
    pub text: bool,
    /// Placed directly as a whole header value somewhere (`ToHeaderValue`).
    pub header: bool,
}

impl Value {
    /// Required and uninitialized: a positional argument.
    pub fn is_positional(&self) -> bool {
        !self.optional && self.default.is_none()
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Usage {
    Scalar,
    Each,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct ValueRef {
    pub owner: usize,
    pub index: usize,
}

pub enum Piece {
    Lit(String),
    Hole(ValueRef),
}

/// One path segment or host label.
pub struct Component {
    pub pieces: Vec<Piece>,
    pub text: String,
    pub sensitive: bool,
}

pub enum Source {
    Template { pieces: Vec<Piece> },
    Ref { value: ValueRef, each: bool },
}

pub struct Contrib {
    pub op: Op,
    pub key: KeyName,
    pub span: Span,
    pub source: Option<Source>,
    pub sensitive: bool,
    /// Every value it reads is always present.
    pub total: bool,
    /// `override ... else inherit`: absence keeps the inherited occurrences.
    pub else_inherit: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct ContribId {
    pub owner: usize,
    pub family: Family,
    pub index: usize,
}

pub enum Item {
    Emit(ContribId),
    /// `override` with an optional source: emit `src` if present, otherwise `fallback`.
    Override {
        src: ContribId,
        fallback: Vec<Item>,
    },
}

pub struct Slot {
    pub key: KeyName,
    pub sensitive: bool,
    pub items: Vec<Item>,
    /// Every contribution ever applied to the slot, overridden and removed ones included:
    /// sensitivity is sticky along the chain.
    pub history: Vec<ContribId>,
    /// The last contribution to the slot is a `remove` (for `accept`: a format's suggestion is
    /// then not sent).
    pub removed: bool,
}

pub struct Composed {
    /// Leftmost label first: (owner, label index).
    pub labels: Vec<(usize, usize)>,
    /// Root first: (owner, segment index).
    pub segments: Vec<(usize, usize)>,
    pub trailing_slash: bool,
    pub query: Vec<Slot>,
    pub headers: HeaderComposition,
}

pub enum HeaderComposition {
    /// Every header key on the chain is a literal name: composed by the macro, as slots.
    Static(Vec<Slot>),
    /// A native `HeaderName` constant is on the chain: its header contributions in composition
    /// order (root first, each owner's `remove`s first), composed at run time by
    /// `__private::HeaderPlan` on names alone.
    Dynamic(Vec<ContribId>),
}

/// Header fields owned by HTTP framing; reqwest/hyper manage them.
const RESERVED_HEADERS: &[&str] = &[
    "host",
    "content-length",
    "transfer-encoding",
    "connection",
    "te",
    "trailer",
    "upgrade",
    "keep-alive",
];

/// Query/header names whose values are withheld from Concord output even when not marked
/// `#[sensitive]` (compared after lowercasing and mapping `_` to `-`).
const SENSITIVE_NAMES: &[&str] = &[
    "authorization",
    "proxy-authorization",
    "cookie",
    "set-cookie",
    "x-api-key",
    "api-key",
    "apikey",
    "x-auth-token",
    "auth-token",
    "access-token",
    "refresh-token",
    "id-token",
    "token",
    "client-secret",
    "secret",
    "password",
    "signature",
    "session",
    "api-token",
    "x-api-token",
    "x-access-token",
    "private-token",
    "x-goog-api-key",
    "ocp-apim-subscription-key",
    "x-amz-security-token",
];

fn sensitive_name(key: &str) -> bool {
    let normalized = key.to_ascii_lowercase().replace('_', "-");
    SENSITIVE_NAMES.contains(&normalized.as_str())
}

/// The byte offset at which `http::HeaderValue::from_bytes` rejects `text`, if it does. Validity
/// is per byte, so the offset is found by asking about single bytes.
fn rejected_value(text: &str) -> Option<(usize, u8)> {
    if http::HeaderValue::from_bytes(text.as_bytes()).is_ok() {
        return None;
    }
    let offset = text
        .bytes()
        .position(|b| http::HeaderValue::from_bytes(&[b]).is_err())
        .unwrap_or(0);
    Some((offset, text.as_bytes().get(offset).copied().unwrap_or(0)))
}

/// A possibly huge user string, quoted and shortened for a message.
fn elided(text: &str) -> String {
    if text.chars().count() <= 64 {
        format!("{text:?}")
    } else {
        let head: String = text.chars().take(48).collect();
        format!("{head:?}… ({} bytes)", text.len())
    }
}

/// A structural fact the macro may or may not be able to decide.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Known {
    Yes(usize),
    No,
    Maybe,
}

#[derive(Default)]
struct Errors(Option<syn::Error>);

impl Errors {
    fn push(&mut self, e: syn::Error) {
        match &mut self.0 {
            Some(all) => all.combine(e),
            None => self.0 = Some(e),
        }
    }

    fn check(&mut self) -> syn::Result<()> {
        match self.0.take() {
            Some(e) => Err(e),
            None => Ok(()),
        }
    }
}

/// Levenshtein distance, for "did you mean" on value names.
fn edit_distance(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.chars().enumerate() {
        let mut prev = row[0];
        row[0] = i + 1;
        for (j, &cb) in b.iter().enumerate() {
            let cur = row[j + 1];
            row[j + 1] = (prev + usize::from(ca != cb)).min(row[j] + 1).min(cur + 1);
            prev = cur;
        }
    }
    row[b.len()]
}

fn plain(ident: &Ident) -> String {
    let s = ident.to_string();
    s.strip_prefix("r#").map(str::to_owned).unwrap_or(s)
}

fn pascal(ident: &Ident) -> String {
    plain(ident)
        .split('_')
        .filter(|s| !s.is_empty())
        .map(|s| {
            let mut c = s.chars();
            c.next()
                .map(|f| f.to_ascii_uppercase().to_string() + c.as_str())
                .unwrap_or_default()
        })
        .collect()
}

impl Model {
    /// The model, and the errors that leave it complete (a value that is never used): the
    /// declaration is still rejected, but the client is emitted so that code using it resolves
    /// while the author finishes the declaration.
    pub fn build(api: syntax::Api) -> syn::Result<(Model, Option<syn::Error>)> {
        let mut errors = Errors::default();
        let mut owners = Vec::new();
        let mut stmts = Vec::new();
        flatten(api.client, None, &mut owners, &mut stmts, &mut errors);
        errors.check()?;
        let mut model = Model {
            owners,
            key_paths: Vec::new(),
        };

        // Value-level checks.
        for owner in &model.owners {
            let mut seen: BTreeMap<String, &Ident> = BTreeMap::new();
            for value in &owner.values {
                let name = plain(&value.name);
                if name.starts_with("__") {
                    errors.push(syn::Error::new(
                        value.name.span(),
                        "names starting with `__` are reserved for generated code",
                    ));
                }
                if matches!(owner.kind, Kind::Client { .. }) && name == "http" {
                    errors.push(syn::Error::new(value.name.span(), "`http` is reserved on the client: it is the `reqwest::Client` argument of `new`"));
                }
                if let Some(first) = seen.insert(name.clone(), &value.name) {
                    let mut e = syn::Error::new(
                        value.name.span(),
                        format!("duplicate value `{name}` in `{}`", owner.dotted),
                    );
                    e.combine(syn::Error::new(first.span(), "first declared here"));
                    errors.push(e);
                }
            }
            if let Some(body) = owner.body() {
                if seen.contains_key(&plain(&body.name)) {
                    errors.push(syn::Error::new(
                        body.name.span(),
                        "the body has the same name as a value",
                    ));
                }
                if plain(&body.name).starts_with("__") {
                    errors.push(syn::Error::new(
                        body.name.span(),
                        "names starting with `__` are reserved for generated code",
                    ));
                }
            }
        }
        errors.check()?;

        // Statements and direct placements.
        for (index, stmts) in stmts.into_iter().enumerate() {
            model.resolve_owner(index, stmts, &mut errors);
        }
        errors.check()?;

        // Effective sensitivity: a value is withheld if it is marked or feeds a sensitive key on
        // some chain, and every path segment, host label and key that reads a withheld value is
        // itself sensitive. Slot membership does not depend on sensitivity, so the slots are
        // computed once and sensitivity is propagated with a worklist (linear in the number of
        // slot memberships and value reads).
        model.propagate_sensitivity();
        model.refresh_sensitivity();
        while model.native_conditions() {
            model.propagate_sensitivity();
            model.refresh_sensitivity();
        }

        // Composition along every chain; errors are reported for each owner's own statements.
        for index in 0..model.owners.len() {
            let composed = model.compose(index, &mut errors, &mut Vec::new());
            if model.owners[index].is_endpoint() {
                model.check_media_conflict(index, &composed, &mut errors);
                model.owners[index].composed = Some(composed);
            }
        }

        // `send value;`: the named client value is the send policy (its use; never on the wire).
        if let Some(index) = model.send_value() {
            let span = model.owners[0].values[index].name.span();
            if model.owners[0].values[index].optional {
                errors.push(syn::Error::new(
                    span,
                    "a send policy value must be required: `send` uses it for every request",
                ));
            }
            model.mark(
                ValueRef { owner: 0, index },
                Usage::Scalar,
                span,
                &mut errors,
            );
            // A policy usually holds credentials: generated `Debug` withholds it (and so needs
            // neither `Display` nor a header value from its type).
            model.owners[0].values[index].debug_sensitive = true;
        }

        // Liveness. An unused value changes nothing else in the model, so on its own it is
        // reported next to the full expansion instead of replacing it.
        let mut unused = Errors::default();
        for owner in &model.owners {
            for value in &owner.values {
                if value.usage.is_none() {
                    unused.push(syn::Error::new(
                        value.name.span(),
                        format!(
                            "value `{}` is never used; place it (`query`, `header(\"..\")`) or reference it from `path`, `subdomain`, `query` or `headers`",
                            plain(&value.name)
                        ),
                    ));
                }
            }
        }
        model.check_names(&mut errors);
        match (unused.check(), errors.check()) {
            (Err(mut first), Err(rest)) => {
                first.combine(rest);
                Err(first)
            }
            (Ok(()), Err(rest)) => Err(rest),
            (unused, Ok(())) => Ok((model, unused.err())),
        }
    }

    /// The owners from the client down to `index`.
    pub fn chain(&self, index: usize) -> Vec<usize> {
        let mut chain = vec![index];
        let mut cur = index;
        while let Some(parent) = self.owners[cur].parent {
            chain.push(parent);
            cur = parent;
        }
        chain.reverse();
        chain
    }

    pub fn value(&self, r: ValueRef) -> &Value {
        &self.owners[r.owner].values[r.index]
    }

    pub fn contrib(&self, id: ContribId) -> &Contrib {
        let owner = &self.owners[id.owner];
        match id.family {
            Family::Query => &owner.query[id.index],
            Family::Header => &owner.headers[id.index],
        }
    }

    fn resolve_ref(&self, at: usize, r: &syntax::Ref) -> syn::Result<ValueRef> {
        let name = plain(&r.name);
        let find = |owner: usize| {
            self.owners[owner]
                .values
                .iter()
                .position(|v| plain(&v.name) == name)
        };
        match &r.owner {
            None => match find(at) {
                Some(index) => Ok(ValueRef { owner: at, index }),
                None => {
                    let mut msg =
                        format!("`{name}` is not a value of `{}`", self.owners[at].dotted);
                    let mut cur = self.owners[at].parent;
                    let mut enclosing = false;
                    while let Some(p) = cur {
                        if find(p).is_some() {
                            msg.push_str(&format!(
                                "; to use the value of an enclosing owner, write `{}.{name}`",
                                self.qualifier(p)
                            ));
                            enclosing = true;
                            break;
                        }
                        cur = self.owners[p].parent;
                    }
                    let close = self.owners[at]
                        .values
                        .iter()
                        .map(|v| plain(&v.name))
                        .find(|v| edit_distance(v, &name) <= 2);
                    if let Some(close) = close.filter(|_| !enclosing) {
                        msg.push_str(&format!("; did you mean `{close}`?"));
                    } else if !enclosing {
                        // Keys and values share one token shape in blocks (`ACCEPT = JSON`).
                        msg.push_str(&format!(
                            "; the right of `=` and `{{..}}` holes name values declared in a \
                             parameter list (a Rust constant is not one: declare a value \
                             initialized from it, `{}: T = ..`)",
                            name.to_lowercase()
                        ));
                    }
                    Err(syn::Error::new(r.name.span(), msg))
                }
            },
            Some(qualifier) => {
                let q = plain(qualifier);
                let mut cur = self.owners[at].parent;
                while let Some(p) = cur {
                    if self.qualifier(p) == q {
                        return match find(p) {
                            Some(index) => Ok(ValueRef { owner: p, index }),
                            None => Err(syn::Error::new(
                                r.name.span(),
                                format!("`{}` has no value `{name}`", self.owners[p].dotted),
                            )),
                        };
                    }
                    cur = self.owners[p].parent;
                }
                if q == self.qualifier(at) {
                    return Err(syn::Error::new(
                        qualifier.span(),
                        format!("`{q}` is the current owner; write the bare name `{name}`"),
                    ));
                }
                Err(syn::Error::new(
                    qualifier.span(),
                    format!(
                        "`{q}` is not an enclosing owner of `{}`; qualify with `client` or the name of an enclosing scope",
                        self.owners[at].dotted
                    ),
                ))
            }
        }
    }

    fn qualifier(&self, owner: usize) -> String {
        match self.owners[owner].kind {
            Kind::Client { .. } => "client".to_owned(),
            _ => plain(&self.owners[owner].name),
        }
    }

    /// The client value a `send value;` statement names (a single identifier equal to a client
    /// value's name), if any; otherwise the statement names a type.
    pub fn send_value(&self) -> Option<usize> {
        let Kind::Client {
            send: Some(Type::Path(path)),
            ..
        } = &self.owners[0].kind
        else {
            return None;
        };
        let ident = path.path.get_ident()?;
        if path.qself.is_some() {
            return None;
        }
        self.owners[0]
            .values
            .iter()
            .position(|v| plain(&v.name) == plain(ident))
    }

    fn mark(&mut self, r: ValueRef, usage: Usage, span: Span, errors: &mut Errors) {
        let value = &mut self.owners[r.owner].values[r.index];
        match value.usage {
            Some(existing) if existing != usage => errors.push(syn::Error::new(
                span,
                format!(
                    "`{}` is used both with and without `each`",
                    plain(&value.name)
                ),
            )),
            _ => value.usage = Some(usage),
        }
    }

    fn pieces(
        &mut self,
        at: usize,
        raw: Vec<RawPiece>,
        usage_span: Span,
        errors: &mut Errors,
    ) -> Option<Vec<Piece>> {
        let mut out = Vec::new();
        for piece in raw {
            match piece {
                RawPiece::Lit(s) => out.push(Piece::Lit(s)),
                RawPiece::Hole(r) => match self.resolve_ref(at, &r) {
                    Ok(value) => {
                        self.mark(value, Usage::Scalar, usage_span, errors);
                        self.owners[value.owner].values[value.index].text = true;
                        out.push(Piece::Hole(value));
                    }
                    Err(e) => {
                        errors.push(e);
                        return None;
                    }
                },
            }
        }
        Some(out)
    }

    fn structural(
        &mut self,
        at: usize,
        pieces: Vec<RawPiece>,
        span: Span,
        what: &str,
        errors: &mut Errors,
    ) -> Option<Component> {
        let text = template::render(&pieces);
        let pieces = self.pieces(at, pieces, span, errors)?;
        let mut sensitive = false;
        for piece in &pieces {
            if let Piece::Hole(r) = piece {
                let value = self.value(*r);
                if value.optional {
                    errors.push(syn::Error::new(
                        span,
                        format!("`{}` is optional (`?`) and cannot be part of a {what}; there is no optional URL structure", plain(&value.name)),
                    ));
                }
                sensitive |= value.sensitive;
            }
        }
        Some(Component {
            pieces,
            text,
            sensitive,
        })
    }

    fn resolve_owner(&mut self, at: usize, stmts: syntax::Stmts, errors: &mut Errors) {
        let is_endpoint = self.owners[at].is_endpoint();

        if let Some(lit) = stmts.path {
            match template::parse(&lit) {
                Err(e) => errors.push(e),
                Ok(raw) => {
                    let parts = template::split(raw, '/');
                    let count = parts.len();
                    for (i, part) in parts.into_iter().enumerate() {
                        if part.is_empty() {
                            if i == count - 1 && i > 0 && is_endpoint {
                                self.owners[at].trailing_slash = true;
                                continue;
                            }
                            let msg = if i == 0 {
                                "path templates are relative: no leading `/`, and no empty template"
                                    .to_owned()
                            } else if i == count - 1 {
                                "a trailing `/` is only allowed on an endpoint's own path"
                                    .to_owned()
                            } else {
                                "empty path segment (`//`)".to_owned()
                            };
                            errors.push(syn::Error::new(lit.span(), msg));
                            continue;
                        }
                        let mut ok = true;
                        let mut literal_only = String::new();
                        let mut all_literal = true;
                        for piece in &part {
                            match piece {
                                RawPiece::Lit(s) => {
                                    literal_only.push_str(s);
                                    if let Some(bad) = s.chars().find(|c| !is_pchar(*c)) {
                                        errors.push(syn::Error::new(
                                            lit.span(),
                                            format!("{bad:?} is not allowed literally in a path template; literal path text must be RFC 3986 `pchar` without `%` (values in `{{..}}` are percent-encoded)"),
                                        ));
                                        ok = false;
                                    }
                                }
                                RawPiece::Hole(_) => all_literal = false,
                            }
                        }
                        if all_literal && (literal_only == "." || literal_only == "..") {
                            errors.push(syn::Error::new(
                                lit.span(),
                                "`.` and `..` path segments are not allowed",
                            ));
                            ok = false;
                        }
                        let component = if ok {
                            self.structural(at, part, lit.span(), "path", errors)
                        } else {
                            None
                        };
                        if let Some(c) = component {
                            self.owners[at].segments.push(c);
                        }
                    }
                }
            }
        }

        if let Some(lit) = stmts.subdomain {
            match template::parse(&lit) {
                Err(e) => errors.push(e),
                Ok(raw) => {
                    for part in template::split(raw, '.') {
                        if part.is_empty() {
                            errors.push(syn::Error::new(lit.span(), "empty subdomain label"));
                            continue;
                        }
                        let mut ok = true;
                        let mut all_literal = true;
                        let mut literal = String::new();
                        for piece in &part {
                            match piece {
                                RawPiece::Lit(s) => {
                                    literal.push_str(s);
                                    if !s.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
                                        errors.push(syn::Error::new(lit.span(), format!("{s:?} is not valid in a DNS label (ASCII letters, digits, `-`)")));
                                        ok = false;
                                    }
                                }
                                RawPiece::Hole(_) => all_literal = false,
                            }
                        }
                        if ok
                            && all_literal
                            && (literal.len() > 63
                                || literal.starts_with('-')
                                || literal.ends_with('-'))
                        {
                            errors.push(syn::Error::new(
                                lit.span(),
                                format!(
                                    "{literal:?} is not a DNS label (1-63 bytes, no edge hyphen)"
                                ),
                            ));
                            ok = false;
                        }
                        let component = if ok {
                            self.structural(at, part, lit.span(), "subdomain", errors)
                        } else {
                            None
                        };
                        if let Some(c) = component {
                            self.owners[at].labels.push(c);
                        }
                    }
                }
            }
        }

        // Direct placements, in declaration order, come before block statements.
        let direct: Vec<(usize, Family, syntax::Key, Span, bool)> = self.owners[at]
            .values
            .iter()
            .enumerate()
            .filter_map(|(index, value)| {
                let (family, key, span, each) = value.placement.clone()?;
                let key = key.unwrap_or_else(|| {
                    syntax::Key::Lit(LitStr::new(&plain(&value.name), value.name.span()))
                });
                Some((index, family, key, span, each))
            })
            .collect();
        for (index, family, key_written, key_span, each) in direct {
            let value = ValueRef { owner: at, index };
            self.mark(
                value,
                if each { Usage::Each } else { Usage::Scalar },
                key_span,
                errors,
            );
            match family {
                Family::Query => self.owners[at].values[index].text = true,
                Family::Header => self.owners[at].values[index].header = true,
            }
            if let Some(key) = self.key(family, &key_written, errors) {
                let v = self.value(value);
                let contrib = Contrib {
                    op: Op::Introduce,
                    key,
                    span: key_span,
                    sensitive: v.sensitive,
                    total: !v.optional,
                    else_inherit: false,
                    source: Some(Source::Ref { value, each }),
                };
                self.push_contrib(at, family, contrib);
            }
        }

        for (family, entries) in [
            (Family::Query, stmts.query),
            (Family::Header, stmts.headers),
        ] {
            for entry in entries.into_iter().flatten() {
                let Some(key) = self.key(family, &entry.key, errors) else {
                    continue;
                };
                let span = entry.key.span();
                let (source, sensitive, total) = match entry.value {
                    None => (None, false, true),
                    Some(Rhs::Ref { each, reference }) => match self.resolve_ref(at, &reference) {
                        Err(e) => {
                            errors.push(e);
                            continue;
                        }
                        Ok(value) => {
                            self.mark(
                                value,
                                if each { Usage::Each } else { Usage::Scalar },
                                reference.span(),
                                errors,
                            );
                            let v = &mut self.owners[value.owner].values[value.index];
                            match family {
                                Family::Query => v.text = true,
                                Family::Header => v.header = true,
                            }
                            let v = self.value(value);
                            (Some(Source::Ref { value, each }), v.sensitive, !v.optional)
                        }
                    },
                    Some(Rhs::Template(lit)) => {
                        let raw = match template::parse(&lit) {
                            Ok(raw) => raw,
                            Err(e) => {
                                errors.push(e);
                                continue;
                            }
                        };
                        if family == Family::Header {
                            // Each decoded literal fragment alone: header-value validity is per
                            // byte, so a hole can never repair an invalid fragment.
                            let rejected =
                                raw.iter().enumerate().find_map(|(i, piece)| match piece {
                                    RawPiece::Lit(s) => rejected_value(s).map(|at| (i, at)),
                                    RawPiece::Hole(_) => None,
                                });
                            if let Some((fragment, (offset, byte))) = rejected {
                                let whole = raw.len() == 1;
                                let place = if whole {
                                    format!("0x{byte:02X} at byte {offset}")
                                } else {
                                    format!(
                                        "0x{byte:02X} at byte {offset} of literal fragment {fragment}; no value can repair it"
                                    )
                                };
                                errors.push(syn::Error::new(
                                    lit.span(),
                                    format!("this header value contains a byte that `http::HeaderValue::from_bytes` rejects ({place})"),
                                ));
                                continue;
                            }
                        }
                        let Some(pieces) = self.pieces(at, raw, lit.span(), errors) else {
                            continue;
                        };
                        let mut sensitive = false;
                        let mut total = true;
                        for piece in &pieces {
                            if let Piece::Hole(r) = piece {
                                let v = self.value(*r);
                                sensitive |= v.sensitive;
                                total &= !v.optional;
                            }
                        }
                        (Some(Source::Template { pieces }), sensitive, total)
                    }
                };
                let op_span = if entry.op == Op::Introduce {
                    span
                } else {
                    entry.op_span
                };
                // (A missing `else inherit` is reported during composition, after the target check.)
                if entry.else_inherit && total {
                    errors.push(syn::Error::new(
                        op_span,
                        "`else inherit` never applies: this source is always present",
                    ));
                }
                let contrib = Contrib {
                    op: entry.op,
                    key,
                    span: op_span,
                    source,
                    sensitive,
                    total,
                    else_inherit: entry.else_inherit,
                };
                self.push_contrib(at, family, contrib);
            }
        }
    }

    fn push_contrib(&mut self, at: usize, family: Family, contrib: Contrib) {
        if let KeyName::Path(i) = contrib.key {
            debug_assert!(
                self.key_paths[i].owner.is_none(),
                "one contribution per occurrence"
            );
            self.key_paths[i].owner = Some(at);
        }
        match family {
            Family::Query => self.owners[at].query.push(contrib),
            Family::Header => self.owners[at].headers.push(contrib),
        }
    }

    fn key(&mut self, family: Family, key: &syntax::Key, errors: &mut Errors) -> Option<KeyName> {
        let span = key.span();
        match (family, key) {
            (Family::Query, syntax::Key::Lit(lit)) => {
                let key = lit.value();
                if key.is_empty() {
                    errors.push(syn::Error::new(span, "query keys cannot be empty"));
                    return None;
                }
                Some(KeyName::Lit(key))
            }
            (Family::Query, syntax::Key::Path(_)) => {
                errors.push(syn::Error::new(
                    span,
                    "query keys are quoted strings (`\"per_page\" = per_page;`); a path names a native `HeaderName` constant, for headers",
                ));
                None
            }
            (Family::Header, syntax::Key::Lit(lit)) => {
                let raw = lit.value();
                let Ok(name) = http::HeaderName::from_bytes(raw.as_bytes()) else {
                    errors.push(syn::Error::new(
                        span,
                        format!(
                            "{} is not a valid header name (rejected by `http::HeaderName::from_bytes`)",
                            elided(&raw)
                        ),
                    ));
                    return None;
                };
                let name = name.as_str().to_owned();
                if RESERVED_HEADERS.contains(&name.as_str()) {
                    errors.push(syn::Error::new(
                        span,
                        format!("`{name}` is managed by the HTTP transport (reqwest/hyper) and cannot be declared"),
                    ));
                    return None;
                }
                Some(KeyName::Lit(name))
            }
            (Family::Header, syntax::Key::Path(path)) => {
                let text = path
                    .to_token_stream()
                    .to_string()
                    .replace(" :: ", "::")
                    .replace(":: ", "::");
                // One entry per occurrence, never merged by tokens: equal tokens can resolve to
                // different constants (`$crate::KEY` from two crates' `macro_rules`, whose
                // hygiene a proc-macro cannot see). Occurrences compare by `HeaderName` when the
                // request is built.
                self.key_paths.push(KeyPath {
                    path: path.clone(),
                    text,
                    owner: None,
                });
                Some(KeyName::Path(self.key_paths.len() - 1))
            }
        }
    }

    /// A key as it is written in a declaration (a quoted literal, or a path), for suggestions.
    pub fn key_written(&self, key: &KeyName) -> String {
        match key {
            KeyName::Lit(s) => format!("{s:?}"),
            KeyName::Path(i) => self.key_paths[*i].text.clone(),
        }
    }

    /// A key as written, for messages and documentation: a header's normalised name or a native
    /// constant's path.
    pub fn key_text(&self, key: &KeyName) -> String {
        match key {
            KeyName::Lit(s) => s.clone(),
            KeyName::Path(i) => self.key_paths[*i].text.clone(),
        }
    }

    /// Recomputes segment, label and contribution sensitivity from values' effective sensitivity.
    fn refresh_sensitivity(&mut self) {
        for o in 0..self.owners.len() {
            let reads_withheld = |model: &Model, c: &Component| {
                c.pieces
                    .iter()
                    .any(|p| matches!(p, Piece::Hole(r) if model.value(*r).debug_sensitive))
            };
            let labels: Vec<bool> = self.owners[o]
                .labels
                .iter()
                .map(|c| reads_withheld(self, c))
                .collect();
            let segments: Vec<bool> = self.owners[o]
                .segments
                .iter()
                .map(|c| reads_withheld(self, c))
                .collect();
            for (c, s) in self.owners[o].labels.iter_mut().zip(labels) {
                c.sensitive = s;
            }
            for (c, s) in self.owners[o].segments.iter_mut().zip(segments) {
                c.sensitive = s;
            }
            for family in [Family::Query, Family::Header] {
                let count = match family {
                    Family::Query => self.owners[o].query.len(),
                    Family::Header => self.owners[o].headers.len(),
                };
                for index in 0..count {
                    let id = ContribId {
                        owner: o,
                        family,
                        index,
                    };
                    let s = self
                        .contrib_values(id)
                        .iter()
                        .any(|r| self.value(*r).debug_sensitive);
                    match family {
                        Family::Query => self.owners[o].query[index].sensitive = s,
                        Family::Header => self.owners[o].headers[index].sensitive = s,
                    }
                }
            }
        }
    }

    fn propagate_sensitivity(&mut self) {
        // Every key slot of every chain: (sensitive by name, contributions ever applied).
        let mut slots: Vec<(bool, Vec<ContribId>)> = Vec::new();
        let mut ignored = Errors::default();
        for index in 0..self.owners.len() {
            self.compose(index, &mut ignored, &mut slots);
        }
        let mut slots_of: HashMap<ContribId, Vec<usize>> = HashMap::new();
        let mut readers: HashMap<ValueRef, Vec<ContribId>> = HashMap::new();
        for (s, (_, history)) in slots.iter().enumerate() {
            for &id in history {
                let entry = slots_of.entry(id).or_default();
                if entry.last() != Some(&s) {
                    entry.push(s);
                    for r in self.contrib_values(id) {
                        readers.entry(r).or_default().push(id);
                    }
                }
            }
        }
        for list in readers.values_mut() {
            list.sort_by_key(|c| (c.owner, c.family == Family::Header, c.index));
            list.dedup_by_key(|c| (c.owner, c.family == Family::Header, c.index));
        }
        let mut sensitive = vec![false; slots.len()];
        let mut queue: Vec<usize> = (0..slots.len()).filter(|&s| slots[s].0).collect();
        let marked: Vec<ValueRef> = self
            .owners
            .iter()
            .enumerate()
            .flat_map(|(o, owner)| {
                owner
                    .values
                    .iter()
                    .enumerate()
                    .filter(|(_, v)| v.debug_sensitive)
                    .map(move |(i, _)| ValueRef { owner: o, index: i })
            })
            .collect();
        for r in marked {
            for id in readers.get(&r).into_iter().flatten() {
                queue.extend(slots_of.get(id).into_iter().flatten().copied());
            }
        }
        while let Some(s) = queue.pop() {
            if std::mem::replace(&mut sensitive[s], true) {
                continue;
            }
            for &id in &slots[s].1 {
                for r in self.contrib_values(id) {
                    let value = &mut self.owners[r.owner].values[r.index];
                    if !std::mem::replace(&mut value.debug_sensitive, true) {
                        for reader in readers.get(&r).into_iter().flatten() {
                            queue.extend(slots_of.get(reader).into_iter().flatten().copied());
                        }
                    }
                }
            }
        }
    }

    /// Values read by a contribution (for effective sensitivity).
    fn contrib_values(&self, id: ContribId) -> Vec<ValueRef> {
        match &self.contrib(id).source {
            Some(Source::Ref { value, .. }) => vec![*value],
            Some(Source::Template { pieces }) => pieces
                .iter()
                .filter_map(|p| match p {
                    Piece::Hole(r) => Some(*r),
                    Piece::Lit(_) => None,
                })
                .collect(),
            None => Vec::new(),
        }
    }

    /// Composes `at`'s chain. `slots_out` receives, per key slot, whether its name is on the
    /// safety list and every contribution applied to it.
    fn compose(
        &self,
        at: usize,
        errors: &mut Errors,
        slots_out: &mut Vec<(bool, Vec<ContribId>)>,
    ) -> Composed {
        let chain = self.chain(at);
        let mut labels = Vec::new();
        for &owner in chain.iter().rev() {
            for i in 0..self.owners[owner].labels.len() {
                labels.push((owner, i));
            }
        }
        let mut segments = Vec::new();
        for &owner in &chain {
            for i in 0..self.owners[owner].segments.len() {
                segments.push((owner, i));
            }
        }
        let query = self.compose_family(&chain, Family::Query, errors, slots_out);
        let native = chain.iter().any(|&o| {
            self.owners[o]
                .headers
                .iter()
                .any(|c| matches!(c.key, KeyName::Path(_)))
        });
        let headers = if native {
            HeaderComposition::Dynamic(self.compose_native(&chain, errors, slots_out))
        } else {
            HeaderComposition::Static(self.compose_family(
                &chain,
                Family::Header,
                errors,
                slots_out,
            ))
        };
        Composed {
            labels,
            segments,
            trailing_slash: self.owners[at].trailing_slash,
            query,
            headers,
        }
    }

    fn compose_family(
        &self,
        chain: &[usize],
        family: Family,
        errors: &mut Errors,
        slots_out: &mut Vec<(bool, Vec<ContribId>)>,
    ) -> Vec<Slot> {
        struct State {
            slot: Slot,
            live: bool,
            last_writer: String,
            /// The owner whose `remove` ended the key's ownership, if any (for diagnostics).
            removed_by: Option<String>,
            /// Every contribution applied to this key on the chain, surviving or not.
            history: Vec<ContribId>,
        }
        let last = *chain.last().unwrap();
        let mut states: Vec<State> = Vec::new();
        let mut position: HashMap<KeyName, usize> = HashMap::new();
        let noun = match family {
            Family::Query => "query key",
            Family::Header => "header",
        };
        for &owner in chain {
            let contribs = match family {
                Family::Query => &self.owners[owner].query,
                Family::Header => &self.owners[owner].headers,
            };
            // An owner's `remove` statements act on what it inherits, before its own
            // contributions (direct placements and block entries), so `remove "k"` + a direct
            // placement of `k` is an exact replacement.
            let order = (0..contribs.len())
                .filter(|&i| contribs[i].op == Op::Remove)
                .chain((0..contribs.len()).filter(|&i| contribs[i].op != Op::Remove));
            for index in order {
                let c = &contribs[index];
                let id = ContribId {
                    owner,
                    family,
                    index,
                };
                let report = owner == last;
                let pos = position.get(&c.key).copied();
                let writer = self.owners[owner].dotted.clone();
                match (c.op, pos) {
                    (Op::Introduce, Some(p)) if states[p].live => {
                        if report {
                            errors.push(syn::Error::new(
                                c.span,
                                format!(
                                    "{noun} `{}` is already set by `{}`; use `override` or `append` to change an inherited {noun}, or add `remove \"{}\";` to this owner's block to replace it exactly",
                                    self.key_text(&c.key), states[p].last_writer, self.key_text(&c.key)
                                ),
                            ));
                        }
                    }
                    (Op::Introduce | Op::Append, Some(p)) => {
                        let s = &mut states[p];
                        if c.op == Op::Introduce {
                            s.slot.items.clear();
                        }
                        s.slot.items.push(Item::Emit(id));
                        s.slot.sensitive |= c.sensitive;
                        s.live = true;
                        s.last_writer = writer;
                        s.removed_by = None;
                        s.history.push(id);
                    }
                    (Op::Introduce | Op::Append, None) => {
                        position.insert(c.key.clone(), states.len());
                        states.push(State {
                            slot: Slot {
                                key: c.key.clone(),
                                sensitive: c.sensitive || c.key.lit().is_some_and(sensitive_name),
                                items: vec![Item::Emit(id)],
                                history: Vec::new(),
                                removed: false,
                            },
                            live: true,
                            last_writer: writer,
                            removed_by: None,
                            history: vec![id],
                        })
                    }
                    (Op::Override | Op::Remove, Some(p)) if states[p].live => {
                        let s = &mut states[p];
                        s.slot.sensitive |= c.sensitive;
                        s.last_writer = writer;
                        s.history.push(id);
                        if c.op == Op::Remove {
                            s.slot.items.clear();
                            s.live = false;
                            s.removed_by = Some(self.owners[owner].dotted.clone());
                        } else if c.total {
                            s.slot.items = vec![Item::Emit(id)];
                        } else {
                            // An `override` whose source can be absent says what absence means.
                            if report && !c.else_inherit {
                                let k = &self.key_text(&c.key);
                                errors.push(syn::Error::new(
                                    c.span,
                                    format!(
                                        "this `override` reads an optional value, so it can be absent; say what absence means: `override \"{k}\" = ... else inherit;` keeps the inherited value, `remove \"{k}\"; \"{k}\" = ...;` sends nothing"
                                    ),
                                ));
                            }
                            let fallback = std::mem::take(&mut s.slot.items);
                            s.slot.items = vec![Item::Override { src: id, fallback }];
                        }
                    }
                    (Op::Override | Op::Remove, _) => {
                        if report {
                            let verb = if c.op == Op::Remove {
                                "remove"
                            } else {
                                "override"
                            };
                            let removed = pos.and_then(|p| states[p].removed_by.clone());
                            let msg = match removed {
                                Some(by) => format!(
                                    "cannot {verb} {noun} `{}`: `{by}` removes it; introduce it again with `\"{}\" = ...`",
                                    self.key_text(&c.key),
                                    self.key_text(&c.key)
                                ),
                                None => format!(
                                    "cannot {verb} {noun} `{}`: no enclosing owner sets it",
                                    self.key_text(&c.key)
                                ),
                            };
                            errors.push(syn::Error::new(c.span, msg));
                        }
                    }
                }
            }
        }
        for state in &states {
            slots_out.push((
                state.slot.key.lit().is_some_and(sensitive_name),
                state.history.clone(),
            ));
        }
        states
            .into_iter()
            .map(|s| Slot {
                history: s.history,
                removed: !s.live,
                ..s.slot
            })
            .collect()
    }

    /// Header composition of a chain that writes a native `HeaderName` constant. Returns its
    /// contributions in composition order (the run-time plan composes them on names). Errors the
    /// macro can prove (with literal names on both sides) are compile errors;
    /// the others are checked by the plan at `prepare()`. `slots_out` receives one slot per
    /// identity the macro knows (a lower bound for sensitivity propagation).
    fn compose_native(
        &self,
        chain: &[usize],
        errors: &mut Errors,
        slots_out: &mut Vec<(bool, Vec<ContribId>)>,
    ) -> Vec<ContribId> {
        let last = *chain.last().unwrap();
        let seq = self.header_seq(chain);
        for (i, &id) in seq.iter().enumerate() {
            if id.owner != last {
                continue;
            }
            let c = self.contrib(id);
            let key = self.key_text(&c.key);
            let written = self.key_written(&c.key);
            if c.op == Op::Override && !c.total && !c.else_inherit {
                errors.push(syn::Error::new(
                    c.span,
                    format!(
                        "this `override` reads an optional value, so it can be absent; say what absence means: `override {written} = ... else inherit;` keeps the inherited value, `remove {written}; {written} = ...;` sends nothing"
                    ),
                ));
            }
            match (c.op, self.set_before(&seq, i)) {
                (Op::Introduce, Known::Yes(j)) => errors.push(syn::Error::new(
                    c.span,
                    format!(
                        "header `{key}` is already set by `{}`; use `override` or `append` to change an inherited header, or add `remove {written};` to this owner's block to replace it exactly",
                        self.owners[seq[j].owner].dotted
                    ),
                )),
                (Op::Override | Op::Remove, Known::No) => {
                    let verb = if c.op == Op::Remove { "remove" } else { "override" };
                    errors.push(syn::Error::new(
                        c.span,
                        format!("cannot {verb} header `{key}`: no enclosing owner sets it"),
                    ));
                }
                _ => {}
            }
        }
        let mut classes: Vec<(KeyName, Vec<ContribId>)> = Vec::new();
        for &id in &seq {
            let key = &self.contrib(id).key;
            match classes.iter_mut().find(|(k, _)| k == key) {
                Some((_, history)) => history.push(id),
                None => classes.push((key.clone(), vec![id])),
            }
        }
        for (key, history) in classes {
            slots_out.push((key.lit().is_some_and(sensitive_name), history));
        }
        seq
    }

    /// A chain's header contributions in composition order: root first, each owner's `remove`s
    /// before its other contributions.
    fn header_seq(&self, chain: &[usize]) -> Vec<ContribId> {
        let mut seq = Vec::new();
        for &owner in chain {
            let contribs = &self.owners[owner].headers;
            let order = (0..contribs.len())
                .filter(|&i| contribs[i].op == Op::Remove)
                .chain((0..contribs.len()).filter(|&i| contribs[i].op != Op::Remove));
            for index in order {
                seq.push(ContribId {
                    owner,
                    family: Family::Header,
                    index,
                });
            }
        }
        seq
    }

    /// Classification that depends on native header names. The static propagation treats a native
    /// constant as a header of its own, not on the safety list (a lower bound). A value it does
    /// not withhold is also withheld at run time when a native constant it is sent under is on the
    /// safety list, or when its key turns out to name the same header as a sensitive
    /// contribution. Such a value that is also written as text is withheld statically instead
    /// (path, host and query redaction is static). Returns whether a value was withheld
    /// statically (propagation must then run again).
    fn native_conditions(&mut self) -> bool {
        let mut conds: HashMap<ValueRef, std::collections::BTreeSet<Cond>> = HashMap::new();
        let mut chains = Vec::new();
        // Every endpoint chain once the declaration has a native key: a value conditionally
        // withheld on one chain is withheld everywhere, also on chains with literal keys only.
        if self.key_paths.is_empty() {
            return false;
        }
        for index in 0..self.owners.len() {
            if self.owners[index].is_endpoint() {
                chains.push(self.header_seq(&self.chain(index)));
            }
        }
        let open = |model: &Model, id: ContribId| -> Vec<ValueRef> {
            model
                .contrib_values(id)
                .into_iter()
                .filter(|r| !model.value(*r).debug_sensitive)
                .collect()
        };
        for seq in &chains {
            for &a in seq {
                let ka = &self.contrib(a).key;
                if let KeyName::Path(p) = ka {
                    for v in open(self, a) {
                        conds.entry(v).or_default().insert(Cond::Name(*p));
                    }
                }
            }
        }
        // The pairs (a, b) of contributions sharing a chain, each once: endpoints of one scope
        // share most of their chain; soundness depends only on the closure over these pairs, though
        // the order of visits can leave out some redundant `Same` conditions (see `implied`). For each
        // `a`, statically sensitive contributions come first: their conditions are the ones the
        // others' conditions most often reduce to (see `implied`).
        let mut pairs: Vec<(ContribId, ContribId)> = Vec::new();
        let mut seen: std::collections::HashSet<(ContribId, ContribId)> = Default::default();
        for seq in &chains {
            for &a in seq {
                let order = seq
                    .iter()
                    .filter(|&&b| self.contrib(b).sensitive)
                    .chain(seq.iter().filter(|&&b| !self.contrib(b).sensitive));
                for &b in order {
                    if a != b && seen.insert((a, b)) {
                        pairs.push((a, b));
                    }
                }
            }
        }
        loop {
            let mut changed = false;
            for &(a, b) in &pairs {
                let ka = self.contrib(a).key.clone();
                let targets = open(self, a);
                if targets.is_empty() {
                    continue;
                }
                let kb = &self.contrib(b).key;
                let theirs: std::collections::BTreeSet<Cond> = self
                    .contrib_values(b)
                    .iter()
                    .flat_map(|r| conds.get(r).into_iter().flatten().cloned())
                    .collect();
                match relation(&ka, kb) {
                    Rel::Different => {}
                    // Same header: whatever makes the other contribution sensitive.
                    Rel::Same => {
                        for v in &targets {
                            let set = conds.entry(*v).or_default();
                            for c in &theirs {
                                changed |= set.insert(c.clone());
                            }
                        }
                    }
                    // Possibly the same header: `ka == kb` and the other is sensitive.
                    // A condition `t` of the other's is redundant for a value that
                    // already has `t` with `kb` replaced by `ka` (equal under `ka == kb`),
                    // so `Same(ka, kb)` is added only for a reason not already covered.
                    // This keeps the conditions linear in the keys of a chain.
                    Rel::Unknown => {
                        let same = if ka <= *kb {
                            Cond::Same(ka.clone(), kb.clone())
                        } else {
                            Cond::Same(kb.clone(), ka.clone())
                        };
                        let fixed = self.contrib(b).sensitive;
                        for v in &targets {
                            let set = conds.entry(*v).or_default();
                            if fixed || theirs.iter().any(|t| !implied(t, &ka, kb, set)) {
                                changed |= set.insert(same.clone());
                            }
                        }
                    }
                }
            }
            if !changed {
                break;
            }
        }
        let mut promoted = false;
        for (r, set) in conds {
            let value = &mut self.owners[r.owner].values[r.index];
            if set.is_empty() || value.debug_sensitive {
                continue;
            }
            if value.text {
                value.debug_sensitive = true;
                value.debug_when.clear();
                promoted = true;
            } else {
                value.debug_when = set.into_iter().collect();
            }
        }
        promoted
    }

    /// Whether the header named by `seq[i]` is set when `seq[i]` applies (the latest earlier
    /// contribution to the same header is not a `remove`), as far as the macro can tell.
    fn set_before(&self, seq: &[ContribId], i: usize) -> Known {
        let key = &self.contrib(seq[i]).key;
        let mut known = Known::No;
        for (j, &id) in seq[..i].iter().enumerate() {
            let c = self.contrib(id);
            let set = if c.op == Op::Remove {
                Known::No
            } else {
                Known::Yes(j)
            };
            known = match relation(key, &c.key) {
                Rel::Same => set,
                Rel::Different => known,
                // The same header or not: the verdict stands only if both agree.
                Rel::Unknown => match (known, set) {
                    (Known::No, Known::No) => Known::No,
                    // Set either way: keep the setter already proved.
                    (Known::Yes(i), Known::Yes(_)) => Known::Yes(i),
                    _ => Known::Maybe,
                },
            };
        }
        known
    }

    /// The nearest owner from `owner` (inclusive) up to the client that declares a `request`
    /// (`request == true`) or `response` default.
    pub fn nearest_default(&self, owner: usize, request: bool) -> Option<usize> {
        nearest(&self.owners, Some(owner), request)
    }

    /// "the client `Shop`" / "scope `Shop.catalog`".
    pub fn describe(&self, owner: usize) -> String {
        match self.owners[owner].kind {
            Kind::Client { .. } => format!("the client `{}`", self.owners[owner].dotted),
            _ => format!("scope `{}`", self.owners[owner].dotted),
        }
    }

    /// 8c: a media type declared for the body and a `content-type` header contribution that
    /// survives composition on the endpoint (conditional ones included) would both set
    /// `Content-Type`. A `remove CONTENT_TYPE` that drops every inherited contribution, or a
    /// `request` statement without a media type on the endpoint, resolves it.
    fn check_media_conflict(&self, endpoint: usize, composed: &Composed, errors: &mut Errors) {
        let Some(body) = self.owners[endpoint].body() else {
            return;
        };
        let Some((media, origin)) = &body.media else {
            return;
        };
        let content_type = KeyName::Lit("content-type".to_owned());
        let writer = match &composed.headers {
            HeaderComposition::Static(slots) => {
                let Some(slot) = slots
                    .iter()
                    .find(|s| s.key == content_type && !s.items.is_empty())
                else {
                    return;
                };
                match &slot.items[0] {
                    Item::Emit(id) | Item::Override { src: id, .. } => {
                        &self.owners[id.owner].dotted
                    }
                }
            }
            // With a native constant on the chain, a literal `content-type` contribution conflicts
            // for certain only if no later contribution could be the same header and replace or
            // remove it; otherwise the run-time plan decides.
            HeaderComposition::Dynamic(seq) => {
                let surviving = seq.iter().enumerate().find(|&(i, &id)| {
                    let c = self.contrib(id);
                    c.op != Op::Remove
                        && c.key == content_type
                        && seq[i + 1..].iter().all(|&later| {
                            let l = self.contrib(later);
                            relation(&c.key, &l.key) == Rel::Different || l.op == Op::Append
                        })
                });
                match surviving {
                    Some((_, id)) => &self.owners[id.owner].dotted,
                    None => return,
                }
            }
        };
        let name = plain(&body.name);
        let (span, from, fix) = match origin {
            Origin::Endpoint => (
                media.span(),
                "written in the endpoint request statement".to_owned(),
                "remove the media type from the endpoint request statement".to_owned(),
            ),
            Origin::Default(o) => (
                body.name.span(),
                format!("from the `request` default of {}", self.describe(*o)),
                format!(
                    "write `request {};` on this endpoint without a media type",
                    body.format.text()
                ),
            ),
        };
        errors.push(syn::Error::new(
            span,
            format!(
                "body `{name}` has the media type {:?} ({from}), and the `content-type` header set by `{writer}` reaches this endpoint: both would set `Content-Type`; keep one: {fix}, or add `remove \"content-type\";` to this endpoint's `headers` block",
                media.value()
            ),
        ));
    }

    fn check_names(&self, errors: &mut Errors) {
        // Generated type names.
        let mut types: BTreeMap<String, &Ident> = BTreeMap::new();
        let client = &self.owners[0];
        let config = client
            .values
            .iter()
            .any(Value::is_positional)
            .then(|| client.config_name().to_string());
        for owner in &self.owners {
            let key = plain(&owner.type_name);
            if config.as_deref() == Some(key.as_str()) {
                let mut e = syn::Error::new(
                    owner.name.span(),
                    format!(
                        "generated type name `{key}` is also the struct of the client's required values; rename this owner"
                    ),
                );
                e.combine(syn::Error::new(
                    client.name.span(),
                    "the client is declared here",
                ));
                errors.push(e);
                continue;
            }
            if let Some(first) = types.insert(key, &owner.name) {
                let mut e = syn::Error::new(
                    owner.name.span(),
                    format!("generated type name `{}` is used twice", owner.type_name),
                );
                e.combine(syn::Error::new(first.span(), "also generated here"));
                errors.push(e);
            }
        }
        // Methods per generated type.
        for owner in &self.owners {
            // name -> (declaration span, or None for built-ins; description)
            let mut methods: BTreeMap<String, (Option<Span>, String)> = BTreeMap::new();
            let mut add = |name: String,
                           span: Option<Span>,
                           origin: String,
                           errors: &mut Errors| {
                if let Some((first_span, first_origin)) = methods.get(&name) {
                    let primary = span.or(*first_span).unwrap_or_else(Span::call_site);
                    let mut e = syn::Error::new(
                        primary,
                        format!(
                            "`{}` would have two methods named `{name}`: {origin} and {first_origin}; rename one of them",
                            owner.type_name
                        ),
                    );
                    if let (Some(_), Some(first)) = (span, first_span) {
                        e.combine(syn::Error::new(
                            *first,
                            format!("{first_origin} is declared here"),
                        ));
                    }
                    errors.push(e);
                } else {
                    methods.insert(name, (span, origin));
                }
            };
            add("clone".into(), None, "`Clone::clone`".into(), errors);
            match owner.kind {
                Kind::Client { .. } => {
                    add("new".into(), None, "the constructor".into(), errors);
                    add(
                        "with_base_url".into(),
                        None,
                        "the built-in `with_base_url`".into(),
                        errors,
                    );
                    add(
                        "validate_header_names".into(),
                        None,
                        "the built-in `validate_header_names`".into(),
                        errors,
                    );
                }
                Kind::Endpoint { .. } => {
                    add("send".into(), None, "the built-in `send`".into(), errors);
                    add(
                        "prepare".into(),
                        None,
                        "the built-in `prepare`".into(),
                        errors,
                    );
                }
                Kind::Scope => {}
            }
            for value in &owner.values {
                let name = plain(&value.name);
                add(
                    format!("set_{name}"),
                    Some(value.name.span()),
                    format!("the control for value `{name}`"),
                    errors,
                );
                if value.optional {
                    add(
                        format!("maybe_{name}"),
                        Some(value.name.span()),
                        format!("the control for value `{name}`"),
                        errors,
                    );
                }
            }
            if let Some(body) = owner.body() {
                let name = plain(&body.name);
                add(
                    format!("set_{name}"),
                    Some(body.name.span()),
                    format!("the control for body `{name}`"),
                    errors,
                );
                if body.optional {
                    add(
                        format!("maybe_{name}"),
                        Some(body.name.span()),
                        format!("the control for body `{name}`"),
                        errors,
                    );
                }
            }
            for &child in &owner.children {
                let c = &self.owners[child];
                add(
                    plain(&c.name),
                    Some(c.name.span()),
                    format!("the navigation method for `{}`", c.dotted),
                    errors,
                );
            }
        }
    }
}

fn is_pchar(c: char) -> bool {
    c.is_ascii_alphanumeric() || "-._~!$&'()*+,;=:@".contains(c)
}

/// Accepts a strict subset of absolute URLs that the `url` crate always parses identically:
/// `http[s]://host[:port][/path]` where `host` is an ASCII DNS name (letters, digits, `-`, `.`), a
/// dotted IPv4 address or a bracketed IPv6 address, `port` is 1-65535 in decimal, and the path
/// consists of `/`-separated RFC 3986 `pchar` without `%`. Anything else is a declaration error,
/// so the generated constructor cannot fail at runtime.
fn validate_base(lit: &LitStr) -> syn::Result<()> {
    let text = lit.value();
    let err = |msg: &str| {
        Err(syn::Error::new(
            lit.span(),
            format!("invalid base URL {text:?}: {msg}"),
        ))
    };
    let rest = if let Some(rest) = text.strip_prefix("https://") {
        rest
    } else if let Some(rest) = text.strip_prefix("http://") {
        rest
    } else {
        return err("it must start with `http://` or `https://`");
    };
    if text.contains('?') {
        return err("it must not contain a query; declare query parameters with `query`");
    }
    if text.contains('#') {
        return err("it must not contain a fragment");
    }
    let (authority, path) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, ""),
    };
    if authority.contains('@') {
        return err("it must not contain user information");
    }
    let (host, port) = if let Some(after) = authority.strip_prefix('[') {
        let Some(end) = after.find(']') else {
            return err("unterminated IPv6 address");
        };
        let (v6, tail) = (&after[..end], &after[end + 1..]);
        if v6.is_empty()
            || !v6
                .chars()
                .all(|c| c.is_ascii_hexdigit() || c == ':' || c == '.')
            || v6.parse::<std::net::Ipv6Addr>().is_err()
        {
            return err("invalid IPv6 address");
        }
        match tail {
            "" => (authority, None),
            t => match t.strip_prefix(':') {
                Some(p) => (&authority[..authority.len() - t.len()], Some(p)),
                None => return err("unexpected text after the IPv6 address"),
            },
        }
    } else {
        match authority.rsplit_once(':') {
            Some((h, p)) => (h, Some(p)),
            None => (authority, None),
        }
    };
    if !host.starts_with('[') {
        let labels: Vec<&str> = host.split('.').collect();
        let valid = !host.is_empty()
            && host.len() <= 253
            && labels.iter().all(|l| {
                !l.is_empty()
                    && l.len() <= 63
                    && l.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
                    && !l.starts_with('-')
                    && !l.ends_with('-')
            });
        if !valid {
            return err(
                "the host must be an ASCII DNS name, an IPv4 address or a bracketed IPv6 address",
            );
        }
        // `xn--` labels go through IDNA validation in URL parsing and can be rejected there;
        // declare such hosts at runtime with `with_base_url`, which returns an error instead.
        if labels
            .iter()
            .any(|l| l.len() >= 4 && l[..4].eq_ignore_ascii_case("xn--"))
        {
            return err(
                "`xn--` (punycode) labels are not accepted in a declared base; pass such a host to `with_base_url` at runtime",
            );
        }
        let all_numeric = labels.iter().all(|l| l.chars().all(|c| c.is_ascii_digit()));
        if all_numeric && host.parse::<std::net::Ipv4Addr>().is_err() {
            return err("invalid IPv4 address");
        }
        // URL parsing treats a host whose last label is a decimal or `0x` hexadecimal number as
        // an IPv4 address; only plain dotted-quad IPv4 is accepted in that form.
        let numeric_label = |l: &str| {
            l.chars().all(|c| c.is_ascii_digit())
                || l.strip_prefix("0x")
                    .or_else(|| l.strip_prefix("0X"))
                    .is_some_and(|hex| hex.chars().all(|c| c.is_ascii_hexdigit()))
        };
        if !all_numeric && labels.last().is_some_and(|l| numeric_label(l)) {
            return err(
                "the last label of a host name cannot be numeric (it would be parsed as an IPv4 address)",
            );
        }
    }
    if let Some(port) = port {
        let valid = !port.is_empty()
            && port.len() <= 5
            && port.chars().all(|c| c.is_ascii_digit())
            && port.parse::<u32>().is_ok_and(|p| (1..=65535).contains(&p));
        if !valid {
            return err("the port must be a decimal number from 1 to 65535");
        }
    }
    if let Some(bad) = path.chars().find(|c| *c != '/' && !is_pchar(*c)) {
        return err(&format!(
            "{bad:?} is not allowed in the base path (RFC 3986 `pchar` without `%`)"
        ));
    }
    if path.split('/').any(|seg| seg == "." || seg == "..") {
        return err("the base path cannot contain `.` or `..` segments");
    }
    Ok(())
}

fn flatten(
    mut owner: syntax::Owner,
    parent: Option<(usize, &str, &str)>,
    out: &mut Vec<Owner>,
    stmts: &mut Vec<syntax::Stmts>,
    errors: &mut Errors,
) -> usize {
    let index = out.len();
    let (type_name, dotted) = match parent {
        None => (plain(&owner.name), plain(&owner.name)),
        Some((_, parent_type, parent_dotted)) => (
            format!("{parent_type}{}", pascal(&owner.name)),
            format!("{parent_dotted}.{}", plain(&owner.name)),
        ),
    };
    let span = owner.name.span();
    // The root keeps the declared identifier (a raw `r#type` stays raw); concatenated names below
    // the root are built from the plain spelling and are never keywords.
    let type_ident = match parent {
        None => owner.name.clone(),
        Some(_) => Ident::new(&type_name, span),
    };
    let default = |stmt: Option<syntax::FormatDefault>, direction: &str| {
        stmt.map(|d| OwnerDefault {
            // Spanned at the default (the user's context): an inheriting endpoint names it with
            // the span moved to its payload type, and rustc then points an unsupported payload
            // there (with the macro's own context it would point at the whole invocation).
            alias: format_ident!(
                "__Concord{}{}Format",
                type_name,
                direction,
                span = d.format.span()
            ),
            format: d.format,
            media: d.media,
            consumers: Vec::new(),
        })
    };
    let endpoint = matches!(owner.kind, OwnerKind::Endpoint { .. });
    let request = if endpoint {
        None
    } else {
        default(owner.stmts.request.take(), "Request")
    };
    let response = if endpoint {
        None
    } else {
        default(owner.stmts.response.take(), "Response")
    };
    let kind = match owner.kind {
        OwnerKind::Client { vis, base } => match base {
            Some(base) => {
                if let Err(e) = validate_base(&base) {
                    errors.push(e);
                }
                Kind::Client {
                    vis,
                    base,
                    send: owner.stmts.send.take().map(|(ty, _)| ty),
                }
            }
            None => {
                errors.push(syn::Error::new(
                    span,
                    "the client must declare `base \"https://...\";`",
                ));
                Kind::Client {
                    vis,
                    base: LitStr::new("http://invalid", span),
                    send: None,
                }
            }
        },
        OwnerKind::Scope => Kind::Scope,
        OwnerKind::Endpoint {
            method,
            output,
            body,
        } => {
            let ancestor = parent.map(|p| p.0);
            // `-> stream` without an item: no typed output, no format.
            let incoming = output.as_ref().is_some_and(|o| o.ty.is_none());
            let output = output.filter(|o| o.ty.is_some());
            // A statement with nothing to apply to is an error on an endpoint; `format` needs one
            // of its two directions.
            match (&owner.stmts.request, &owner.stmts.response) {
                (Some(d), _) if d.both => {
                    if body.is_none() && output.is_none() {
                        errors.push(syn::Error::new(
                            d.span,
                            if incoming {
                                "`format` statement has neither a body nor a typed output to apply to: `-> stream` without an item returns the unread body, and no format applies to it"
                            } else {
                                "`format` statement has neither a body nor a typed output (`-> T`) to apply to"
                            },
                        ));
                    }
                }
                (request, response) => {
                    if let (None, Some(d)) = (&body, request) {
                        errors.push(syn::Error::new(
                            d.span,
                            "`request` statement has no body to apply to",
                        ));
                    }
                    if let (None, Some(d)) = (&output, response) {
                        errors.push(syn::Error::new(
                            d.span,
                            if incoming {
                                "`response` statement has no typed output to apply to: `-> stream` without an item returns the unread body for a reader chosen at run time; to open it with a format, write `-> stream T`"
                            } else {
                                "`response` statement has no typed output (`-> T`) to apply to"
                            },
                        ));
                    }
                }
            }
            Kind::Endpoint {
                method,
                incoming,
                body: body.and_then(|b| {
                    endpoint_body(
                        b,
                        owner.stmts.request.as_ref(),
                        index,
                        ancestor,
                        out,
                        errors,
                    )
                }),
                output: output.and_then(|o| {
                    endpoint_output(
                        o,
                        owner.stmts.response.as_ref(),
                        index,
                        ancestor,
                        out,
                        errors,
                    )
                }),
            }
        }
    };
    if parent.is_some() && plain(&owner.name) == "client" {
        errors.push(syn::Error::new(
            span,
            "`client` is reserved: it qualifies references to client values",
        ));
    }
    let mut values = Vec::new();
    for p in owner.params {
        let placement = p.placement.map(|pl| (pl.family, pl.key, pl.span, pl.each));
        values.push(Value {
            placement,
            docs: p.docs,
            name: p.name,
            ty: p.ty,
            optional: p.optional,
            default: p.default,
            sensitive: p.sensitive,
            debug_sensitive: p.sensitive,
            debug_when: Vec::new(),
            usage: None,
            text: false,
            header: false,
        });
    }
    let mut cfgs: Vec<Attribute> = parent.map(|p| out[p.0].cfgs.clone()).unwrap_or_default();
    cfgs.extend(owner.cfgs.iter().cloned());
    out.push(Owner {
        kind,
        docs: owner.docs,
        cfgs,
        own_cfgs: owner.cfgs,
        deprecated: owner.deprecated,
        type_name: type_ident,
        state_name: format_ident!("__Concord{}State", type_name, span = span),
        name: owner.name,
        dotted: dotted.clone(),
        parent: parent.map(|p| p.0),
        children: Vec::new(),
        values,
        labels: Vec::new(),
        segments: Vec::new(),
        trailing_slash: false,
        query: Vec::new(),
        headers: Vec::new(),
        composed: None,
        request,
        response,
    });
    stmts.push(owner.stmts);

    // Scope names must be unique along a chain so that qualified references are unambiguous.
    let mut seen_children: BTreeMap<String, Span> = BTreeMap::new();
    for child in owner.children {
        let child_name = plain(&child.name);
        if let Some(first) = seen_children.insert(child_name.clone(), child.name.span()) {
            let mut e = syn::Error::new(
                child.name.span(),
                format!("`{dotted}` already has a child named `{child_name}`"),
            );
            e.combine(syn::Error::new(first, "first declared here"));
            errors.push(e);
        }
        if matches!(child.kind, OwnerKind::Scope) {
            let mut cur = Some(index);
            while let Some(a) = cur {
                if !matches!(out[a].kind, Kind::Client { .. }) && plain(&out[a].name) == child_name
                {
                    errors.push(syn::Error::new(
                        child.name.span(),
                        format!("scope `{child_name}` has the same name as an enclosing scope; qualified references would be ambiguous"),
                    ));
                }
                cur = out[a].parent;
            }
        }
        let child_index = flatten(
            child,
            Some((index, &type_name, &dotted)),
            out,
            stmts,
            errors,
        );
        out[index].children.push(child_index);
    }
    index
}

/// The nearest owner from `from` up to the client that declares a default for the direction.
fn nearest(out: &[Owner], from: Option<usize>, request: bool) -> Option<usize> {
    let mut cur = from;
    while let Some(o) = cur {
        let declared = if request {
            out[o].request.is_some()
        } else {
            out[o].response.is_some()
        };
        if declared {
            return Some(o);
        }
        cur = out[o].parent;
    }
    None
}

/// A body with its nearest request statement; the selected format and media travel together.
fn endpoint_body(
    b: syntax::BodyParam,
    own: Option<&syntax::FormatDefault>,
    endpoint: usize,
    ancestor: Option<usize>,
    out: &mut [Owner],
    errors: &mut Errors,
) -> Option<Body> {
    if b.format.is_some() || b.media.is_some() {
        let format = b
            .format
            .as_ref()
            .map(Format::text)
            .or_else(|| own.map(|d| d.format.text()))
            .or_else(|| {
                nearest(out, ancestor, true).map(|i| out[i].request.as_ref().unwrap().format.text())
            })
            .unwrap_or("F".to_owned());
        let media = b
            .media
            .as_ref()
            .map_or(String::new(), |m| format!(" {:?}", m.value()));
        let (ty, _, note) = syntax::advice(&b.ty, None);
        errors.push(syn::Error::new(b.name.span(), format!(
            "removed in-place body format: write `body {}{}: {ty}` and `request {format}{media};` on this endpoint (a format is a type path: `{format}` must name a format type in scope){note}",
            b.name, if b.optional { "?" } else { "" })));
        return None;
    }
    let (format, origin, media) = match own {
        Some(d) => (
            d.format.clone(),
            Origin::Endpoint,
            d.media.clone().map(|m| (m, Origin::Endpoint)),
        ),
        None => match nearest(out, ancestor, true) {
            Some(o) => {
                let d = out[o].request.as_mut().expect("nearest has a default");
                d.consumers.push(endpoint);
                let inherited = d.media.clone().map(|m| (m, Origin::Default(o)));
                (d.format.clone(), Origin::Default(o), inherited)
            }
            None => {
                let name = plain(&b.name);
                let message = format!(
                    "body `{name}` has no format: say `request F;` (`F` a type implementing `Encode`) on this endpoint, a scope or the client"
                );
                errors.push(syn::Error::new(b.ty.span(), message));
                return None;
            }
        },
    };
    Some(Body {
        position: b.position,
        docs: b.docs,
        name: b.name,
        optional: b.optional,
        ty: b.ty,
        format,
        origin,
        media,
    })
}

/// A typed output with its nearest response statement.
fn endpoint_output(
    o: syntax::Output,
    own: Option<&syntax::FormatDefault>,
    endpoint: usize,
    ancestor: Option<usize>,
    out: &mut [Owner],
    errors: &mut Errors,
) -> Option<Output> {
    let stream = o.stream.is_some();
    let ty = o.ty.expect("an item-less stream has no typed output");
    if let Some(format) = &o.format {
        // `-> A B`: two type paths. The removed spelling put the format first, but a swapped
        // `-> T F` reads the same, and no name tells a format from a payload.
        errors.push(syn::Error::new(
            ty.span(),
            format!(
                "removed in-place output format: `-> {} {}` names two types; write the payload alone (`-> T`) and the format as a statement (`response F;`) on this endpoint",
                format.text(),
                type_text(&ty)
            ),
        ));
        return None;
    }
    let (format, origin) = match own {
        Some(d) => (d.format.clone(), Origin::Endpoint),
        None => match nearest(out, ancestor, false) {
            Some(owner) => {
                let d = out[owner].response.as_mut().expect("nearest has a default");
                d.consumers.push(endpoint);
                (d.format.clone(), Origin::Default(owner))
            }
            None => {
                let text = type_text(&ty);
                let message = if stream {
                    format!(
                        "stream output `{text}` has no format: say `response F;` (`F` a type implementing `Incremental<{text}>`) on this endpoint, a scope or the client; `-> stream` without an item returns the unread body"
                    )
                } else {
                    let unit = if matches!(&ty, syn::Type::Tuple(t) if t.elems.is_empty()) {
                        " (`-> ()` is a typed output decoded with a format, such as JSON `null`; an endpoint that returns nothing omits `->`)"
                    } else {
                        "; omit `->` to drain"
                    };
                    format!(
                        "output `{text}` has no format: say `response F;` (`F` a type implementing `Decode`) on this endpoint, a scope or the client{unit}"
                    )
                };
                errors.push(syn::Error::new(ty.span(), message));
                return None;
            }
        },
    };
    Some(Output {
        ty,
        format,
        origin,
        stream,
    })
}

/// D11: the declared-base checker accepts a strict subset of what the runtime accepts, so the
/// generated `Base::from_static` can never panic. Checked against the real runtime parser on a
/// hand-written corpus (refinement core track, `exp/core/d11/cases.py`) and on deterministic
/// structured random inputs.
#[cfg(test)]
mod base_consistency {
    use super::{RESERVED_HEADERS, SENSITIVE_NAMES, validate_base};
    use concord_core::__private::Base;

    /// The macro classifies literal names, the runtime native constants: one list each, kept equal.
    #[test]
    fn name_lists_match_the_runtime() {
        assert_eq!(SENSITIVE_NAMES, concord_core::__private::SENSITIVE_NAMES);
        assert_eq!(RESERVED_HEADERS, concord_core::__private::RESERVED_NAMES);
    }
    use proc_macro2::Span;
    use syn::LitStr;

    fn static_ok(input: &str) -> bool {
        validate_base(&LitStr::new(input, Span::call_site())).is_ok()
    }

    fn check(input: &str) {
        if static_ok(input) {
            assert!(
                Base::parse(input).is_ok(),
                "accepted at compile time but rejected at runtime: {input:?}"
            );
        }
    }

    #[test]
    fn corpus() {
        let corpus = [
            "http://example.test",
            "http://example.test/",
            "https://example.test/v1",
            "https://example.test/v1/",
            "http://example.test//",
            "http://localhost:3000",
            "http://EXAMPLE.Test/V1",
            "HTTP://example.test",
            "Http://example.test",
            "http://example.test.",
            "http://example.test./v1",
            "http://example.test..",
            "http://.example.test",
            "http://127.0.0.1.",
            "http://example.test:80",
            "https://example.test:443/v1",
            "http://example.test:443",
            "http://[::1]:80",
            "http://example.test:",
            "http://example.test:0080",
            "http://example.test:08080",
            "http://example.test:65535",
            "http://example.test:65536",
            "http://example.test:0",
            "http://example.test:+80",
            "http://127.0.0.1:8080",
            "http://127.1",
            "http://2130706433",
            "http://0x7f.0.0.1",
            "http://01.02.03.04",
            "http://256.0.0.1",
            "http://1.2.3",
            "http://api.0x10",
            "http://api.123",
            "http://123.api",
            "http://[::1]",
            "http://[::1]:8080/v1",
            "http://[0:0:0:0:0:0:0:1]",
            "http://[::FFFF:127.0.0.1]",
            "http://[fe80::1%25eth0]",
            "http://[v1.x]",
            "http://[::1",
            "http://[::1]x",
            "http://[1:2:3:4:5:6:7:8:9]",
            "http://::1",
            "http://b\u{fc}cher.example",
            "http://xn--bcher-kva.example",
            "http://XN--BCHER-KVA.example",
            "http://xn--a.example",
            "http://a.b.c.xn--pokxncvks",
            "http://fa\u{df}.example",
            "http://\u{ff45}\u{ff58}\u{ff41}\u{ff4d}\u{ff50}\u{ff4c}\u{ff45}.test",
            "http://a\u{3002}b.test",
            "http://\u{4f8b}\u{3048}.\u{30c6}\u{30b9}\u{30c8}",
            "http://ab--cd.example",
            "http://x_y.example",
            "http://-lead.example",
            "http://trail-.example",
            "http://ex%41mple.test",
            "http://exa\nmple.test",
            "http://aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa.example",
            "http://bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb.bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb.bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb.bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb.tst",
            "http://bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb.bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb.bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb.bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb.tst",
            "http://example.test/a%20b",
            "http://example.test/a%2Fb",
            "http://example.test/caf%C3%A9",
            "http://example.test/caf\u{e9}",
            "http://example.test/a b",
            "http://example.test/a%zz",
            "http://example.test/a%",
            "http://example.test/x/%2e%2e/admin",
            "http://example.test/x/%2E/y",
            "http://example.test/a/./b",
            "http://example.test/a/../b",
            "http://example.test/..",
            "http://example.test/...",
            "http://example.test/a\\b",
            "http://example.test/a|b",
            "http://example.test/[x]",
            "http://example.test/{x}",
            "http://example.test/a^b",
            "http://example.test/a\"b",
            "http://example.test/a\tb",
            "http://example.test/a`b",
            "http://example.test/v1:beta@x!$&'()*+,;=",
            " http://example.test",
            "http://example.test ",
            "http:\\\\example.test",
            "http:example.test",
            "http:///example.test",
            "http://@example.test",
            "http://:@example.test",
            "http://user@example.test",
            "http://example.test/?",
            "http://example.test/#",
            "http://example.test?x",
            "ftp://example.test",
            "http://",
        ];
        let accepted = corpus.iter().filter(|i| static_ok(i)).count();
        assert!(
            accepted > 20,
            "the corpus must exercise accepted bases ({accepted})"
        );
        corpus.iter().for_each(|i| check(i));
    }

    /// xorshift64*: deterministic, no dependency.
    struct Rng(u64);

    impl Rng {
        fn next(&mut self) -> u64 {
            self.0 ^= self.0 >> 12;
            self.0 ^= self.0 << 25;
            self.0 ^= self.0 >> 27;
            self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
        }
        fn pick<'a>(&mut self, items: &[&'a str]) -> &'a str {
            items[(self.next() % items.len() as u64) as usize]
        }
    }

    #[test]
    fn random_structured_bases() {
        let schemes = [
            "http://", "https://", "HTTP://", "ftp://", "http:", "http:///",
        ];
        let label_chars = [
            "a", "z", "0", "9", "-", ".", "_", "A", "x", "n", "%", "\u{fc}", "[", ":",
        ];
        let hosts = [
            "127.0.0.1",
            "[::1]",
            "[::ffff:1.2.3.4]",
            "1.2.3",
            "0x7f.1",
            "localhost",
            "",
        ];
        let ports = ["", ":", ":0", ":1", ":80", ":65535", ":65536", ":080", ":x"];
        let path_chars = [
            "a", "/", "-", ".", "..", "%", "%2F", "%41", "~", "!", "$", "&", "'", "(", ")", "*",
            "+", ",", ";", "=", ":", "@", "?", "#", " ", "\\", "\u{e9}", "[", "]", "|",
        ];
        let mut rng = Rng(0x0C0F_FEE0_D11B_A5E5);
        let mut accepted = 0;
        for _ in 0..50_000 {
            let mut url = String::from(rng.pick(&schemes));
            // Half of the inputs are near-valid (mostly accepted), half adversarial.
            let near_valid = rng.next().is_multiple_of(2);
            if near_valid {
                url = String::from(rng.pick(&["http://", "https://"]));
                for i in 0..1 + rng.next() % 4 {
                    if i > 0 {
                        url.push('.');
                    }
                    url.push_str(rng.pick(&["api", "x-y", "a1", "b", "9z", "eu", "test"]));
                }
            } else if rng.next().is_multiple_of(4) {
                url.push_str(rng.pick(&hosts));
            } else {
                for _ in 0..1 + rng.next() % 12 {
                    url.push_str(rng.pick(&label_chars));
                }
            }
            url.push_str(rng.pick(&ports));
            for _ in 0..rng.next() % 10 {
                url.push_str(rng.pick(&path_chars));
            }
            if static_ok(&url) {
                accepted += 1;
            }
            check(&url);
        }
        assert!(
            accepted > 1_000,
            "the generator must reach accepted bases ({accepted})"
        );
    }
}
