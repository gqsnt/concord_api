//! Name resolution, validation and per-endpoint composition.
//!
//! Everything that decides *which* contributions reach an endpoint's request happens here, at
//! compile time. Generated code only decides, with plain `if let`s, whether an optional value is
//! present.

use std::collections::{BTreeMap, HashMap};

use proc_macro2::Span;
use quote::format_ident;
use syn::{Attribute, Expr, Ident, LitStr, Type, Visibility};

use crate::syntax::{self, Family, Op, OwnerKind, Rhs};
use crate::template::{self, RawPiece};

pub struct Model {
    pub owners: Vec<Owner>,
}

// Few owners per declaration; boxing the larger variant buys nothing.
#[allow(clippy::large_enum_variant)]
pub enum Kind {
    Client {
        vis: Visibility,
        base: LitStr,
    },
    Scope,
    Endpoint {
        method: Ident,
        output: Option<syntax::Payload>,
        body: Option<Body>,
    },
}

pub struct Body {
    pub position: usize,
    pub docs: Vec<Attribute>,
    pub name: Ident,
    pub optional: bool,
    /// The payload type the caller supplies.
    pub ty: Type,
    /// Its representation (`Encode<ty>`).
    pub repr: Type,
}

pub struct Owner {
    pub kind: Kind,
    pub docs: Vec<Attribute>,
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
    pub placement: Option<(Family, Option<String>, Span, bool)>,
    /// Effective sensitivity: marked `#[sensitive]`, or feeding a query/header key that is
    /// sensitive on some chain (marked contribution or safety-list name). Withheld from every
    /// Concord-produced output, and makes every placement that reads it sensitive.
    pub debug_sensitive: bool,
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
    pub key: String,
    pub span: Span,
    pub source: Option<Source>,
    pub sensitive: bool,
    /// Every value it reads is always present.
    pub total: bool,
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
    pub key: String,
    pub sensitive: bool,
    pub items: Vec<Item>,
}

pub struct Composed {
    /// Leftmost label first: (owner, label index).
    pub labels: Vec<(usize, usize)>,
    /// Root first: (owner, segment index).
    pub segments: Vec<(usize, usize)>,
    pub trailing_slash: bool,
    pub query: Vec<Slot>,
    pub headers: Vec<Slot>,
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
];

fn sensitive_name(key: &str) -> bool {
    let normalized = key.to_ascii_lowercase().replace('_', "-");
    SENSITIVE_NAMES.contains(&normalized.as_str())
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
    pub fn build(api: syntax::Api) -> syn::Result<Model> {
        let mut errors = Errors::default();
        let mut owners = Vec::new();
        let mut stmts = Vec::new();
        flatten(api.client, None, &mut owners, &mut stmts, &mut errors);
        errors.check()?;
        let mut model = Model { owners };

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

        // Composition along every chain; errors are reported for each owner's own statements.
        for index in 0..model.owners.len() {
            let composed = model.compose(index, &mut errors, &mut Vec::new());
            if model.owners[index].is_endpoint() {
                model.owners[index].composed = Some(composed);
            }
        }

        // Liveness.
        for owner in &model.owners {
            for value in &owner.values {
                if value.usage.is_none() {
                    errors.push(syn::Error::new(
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
        errors.check()?;
        Ok(model)
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
                    while let Some(p) = cur {
                        if find(p).is_some() {
                            msg.push_str(&format!(
                                "; to use the value of an enclosing owner, write `{}.{name}`",
                                self.qualifier(p)
                            ));
                            break;
                        }
                        cur = self.owners[p].parent;
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
        let direct: Vec<(usize, Family, String, Span, bool)> = self.owners[at]
            .values
            .iter()
            .enumerate()
            .filter_map(|(index, value)| {
                let (family, key, span, each) = value.placement.clone()?;
                Some((
                    index,
                    family,
                    key.unwrap_or_else(|| plain(&value.name)),
                    span,
                    each,
                ))
            })
            .collect();
        for (index, family, key_lit, key_span, each) in direct {
            let value = ValueRef { owner: at, index };
            self.mark(
                value,
                if each { Usage::Each } else { Usage::Scalar },
                key_span,
                errors,
            );
            if let Some(key) = self.key(family, &key_lit, key_span, errors) {
                let v = self.value(value);
                let contrib = Contrib {
                    op: Op::Introduce,
                    key,
                    span: key_span,
                    sensitive: v.sensitive,
                    total: !v.optional,
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
                let Some(key) = self.key(family, &entry.key.value(), entry.key.span(), errors)
                else {
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
                            let bad = raw.iter().find_map(|piece| match piece {
                                RawPiece::Lit(s) => {
                                    s.chars().find(|c| !(*c == '\t' || (' '..='~').contains(c)))
                                }
                                RawPiece::Hole(_) => None,
                            });
                            if let Some(bad) = bad {
                                errors.push(syn::Error::new(
                                    lit.span(),
                                    format!("{bad:?} is not allowed in a header value; header values are visible ASCII, space or tab"),
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
                let contrib = Contrib {
                    op: entry.op,
                    key,
                    span: op_span,
                    source,
                    sensitive,
                    total,
                };
                self.push_contrib(at, family, contrib);
            }
        }
    }

    fn push_contrib(&mut self, at: usize, family: Family, contrib: Contrib) {
        match family {
            Family::Query => self.owners[at].query.push(contrib),
            Family::Header => self.owners[at].headers.push(contrib),
        }
    }

    fn key(&self, family: Family, key: &str, span: Span, errors: &mut Errors) -> Option<String> {
        match family {
            Family::Query => {
                if key.is_empty() {
                    errors.push(syn::Error::new(span, "query keys cannot be empty"));
                    return None;
                }
                Some(key.to_owned())
            }
            Family::Header => {
                let lower = key.to_ascii_lowercase();
                let token = !lower.is_empty()
                    && lower
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&b));
                if !token {
                    errors.push(syn::Error::new(
                        span,
                        format!("{key:?} is not a valid header field name"),
                    ));
                    return None;
                }
                if RESERVED_HEADERS.contains(&lower.as_str()) {
                    errors.push(syn::Error::new(
                        span,
                        format!("`{lower}` is managed by the HTTP transport (reqwest/hyper) and cannot be declared"),
                    ));
                    return None;
                }
                Some(lower)
            }
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
        let headers = self.compose_family(&chain, Family::Header, errors, slots_out);
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
            /// Every contribution applied to this key on the chain, surviving or not.
            history: Vec<ContribId>,
        }
        let last = *chain.last().unwrap();
        let mut states: Vec<State> = Vec::new();
        let mut position: HashMap<String, usize> = HashMap::new();
        let noun = match family {
            Family::Query => "query key",
            Family::Header => "header",
        };
        for &owner in chain {
            let contribs = match family {
                Family::Query => &self.owners[owner].query,
                Family::Header => &self.owners[owner].headers,
            };
            for (index, c) in contribs.iter().enumerate() {
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
                                    "{noun} `{}` is already set by `{}`; use `override`, `append` or `remove` to change an inherited {noun}",
                                    c.key, states[p].last_writer
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
                        s.history.push(id);
                    }
                    (Op::Introduce | Op::Append, None) => {
                        position.insert(c.key.clone(), states.len());
                        states.push(State {
                            slot: Slot {
                                key: c.key.clone(),
                                sensitive: c.sensitive || sensitive_name(&c.key),
                                items: vec![Item::Emit(id)],
                            },
                            live: true,
                            last_writer: writer,
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
                        } else if c.total {
                            s.slot.items = vec![Item::Emit(id)];
                        } else {
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
                            errors.push(syn::Error::new(
                                c.span,
                                format!(
                                    "cannot {verb} {noun} `{}`: no enclosing owner sets it",
                                    c.key
                                ),
                            ));
                        }
                    }
                }
            }
        }
        for state in &states {
            slots_out.push((sensitive_name(&state.slot.key), state.history.clone()));
        }
        states.into_iter().map(|s| s.slot).collect()
    }

    fn check_names(&self, errors: &mut Errors) {
        // Generated type names.
        let mut types: BTreeMap<String, &Ident> = BTreeMap::new();
        for owner in &self.owners {
            if let Some(first) = types.insert(owner.type_name.to_string(), &owner.name) {
                let mut e = syn::Error::new(
                    owner.name.span(),
                    format!("generated type name `{}` is used twice", owner.type_name),
                );
                e.combine(syn::Error::new(first.span(), "also generated here"));
                errors.push(e);
            }
            let required = owner.values.iter().any(Value::is_positional);
            if matches!(owner.kind, Kind::Client { .. }) && required {
                let config = owner.config_name();
                if let Some(first) = types.insert(config.to_string(), &owner.name) {
                    let mut e = syn::Error::new(
                        first.span(),
                        format!(
                            "generated type name `{config}` is also the struct of the client's required values; rename this owner"
                        ),
                    );
                    e.combine(syn::Error::new(
                        owner.name.span(),
                        "the client is declared here",
                    ));
                    errors.push(e);
                }
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
    owner: syntax::Owner,
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
    let kind = match owner.kind {
        OwnerKind::Client { vis, base } => match base {
            Some(base) => {
                if let Err(e) = validate_base(&base) {
                    errors.push(e);
                }
                Kind::Client { vis, base }
            }
            None => {
                errors.push(syn::Error::new(
                    span,
                    "the client must declare `base \"https://...\";`",
                ));
                Kind::Client {
                    vis,
                    base: LitStr::new("http://invalid", span),
                }
            }
        },
        OwnerKind::Scope => Kind::Scope,
        OwnerKind::Endpoint {
            method,
            output,
            body,
        } => Kind::Endpoint {
            method,
            output,
            body: body.map(|b| Body {
                position: b.position,
                docs: b.docs,
                name: b.name,
                optional: b.optional,
                ty: b.payload.ty,
                repr: b.payload.repr,
            }),
        },
    };
    if parent.is_some() && plain(&owner.name) == "client" {
        errors.push(syn::Error::new(
            span,
            "`client` is reserved: it qualifies references to client values",
        ));
    }
    let mut values = Vec::new();
    for p in owner.params {
        let placement = p.placement.map(|pl| {
            (
                pl.family,
                pl.key.as_ref().map(LitStr::value),
                pl.span,
                pl.each,
            )
        });
        values.push(Value {
            placement,
            docs: p.docs,
            name: p.name,
            ty: p.ty,
            optional: p.optional,
            default: p.default,
            sensitive: p.sensitive,
            debug_sensitive: p.sensitive,
            usage: None,
        });
    }
    out.push(Owner {
        kind,
        docs: owner.docs,
        type_name: Ident::new(&type_name, span),
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
