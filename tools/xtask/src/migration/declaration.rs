//! Owner-graph resolution and v2.1 -> v3 declaration conversion.
//!
//! Every change is decided from the parsed v2.1 owner graph (client, scopes,
//! endpoints and their values) and recorded as an exact byte edit. Nothing in
//! this module rewrites declaration text by pattern matching.

use std::collections::{BTreeSet, HashMap, HashSet};

use super::Settings;
use super::edits::{EditSet, Rng, line_indent, removal_range};
use super::report::{Code, EndpointIdentity, PolicyIdentity, RawDiagnostic};
use super::rust_file::{Imports, ModuleScope, is_concord_crate};
use super::syntax::{
    Api, Atom, Child, ClauseMode, Client, Dimension, Emission, Endpoint, FormalDimension,
    HeaderClause, KeyClause, KeyOperation, KeyStatement, Name, PageValue, Pagination,
    RateAttachment, RatePolicy, Reference, RouteClause, Scope, Site, UserPath, ValueDecl, Window,
};

/// Declaration types that v2.1 resolved through `use concord::prelude::*`
/// inside its generated specification module. v3 resolves declaration types
/// in the invocation's own module, so these need an explicit import there.
const V2_PRELUDE_TYPES: &[&str] = &[
    "ApiError",
    "ApiResponse",
    "Binary",
    "ByteStream",
    "Bytes",
    "ClientBuildError",
    "Csv",
    "Discard",
    "Empty",
    "ItemStream",
    "Json",
    "Multipart",
    "NdJson",
    "RequestStream",
    "SecretString",
    "Text",
];

/// Read-only inputs for converting one declaration.
pub(crate) struct DeclarationInput<'a> {
    pub(crate) source: &'a str,
    pub(crate) file: &'a str,
    pub(crate) settings: &'a Settings,
    pub(crate) imports: &'a Imports,
    /// The module containing the invocation, when it is a module-level item.
    pub(crate) scope: Option<&'a ModuleScope>,
    /// Byte offset of the invocation's macro path.
    pub(crate) offset: usize,
}

/// Mutable outputs of one declaration conversion.
pub(crate) struct DeclarationOutput<'a> {
    pub(crate) edits: &'a mut EditSet,
    pub(crate) diagnostics: &'a mut Vec<RawDiagnostic>,
    pub(crate) identities: &'a mut Vec<EndpointIdentity>,
    /// Prelude names already imported per module of this file.
    pub(crate) prelude_imports: &'a mut HashMap<usize, BTreeSet<String>>,
}

pub(crate) fn convert(api: &Api, input: &DeclarationInput<'_>, output: DeclarationOutput<'_>) {
    let mut converter = Converter {
        source: input.source,
        file: input.file,
        settings: input.settings,
        imports: input.imports,
        client: &api.client.name,
        policies: &api.client.policies,
        edits: output.edits,
        diagnostics: output.diagnostics,
        identities: output.identities,
        frames: Vec::new(),
        rates: Vec::new(),
        placed: HashSet::new(),
        used: HashSet::new(),
    };
    converter.client(api);
    converter.liveness(api);
    converter.prelude_import(api, input, output.prelude_imports);
}

/// v3 Pascal case (CALL-01): capitalize each underscore-separated word of the
/// snake conversion.
pub(crate) fn v3_pascal_case(name: &str) -> String {
    v3_snake_case(name)
        .split('_')
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut characters = part.chars();
            characters.next().map_or_else(String::new, |first| {
                first.to_ascii_uppercase().to_string() + characters.as_str()
            })
        })
        .collect()
}

/// The core methods the v3 generator reserves on each generated surface (the
/// v3 macro's `CLIENT_METHODS`, `SCOPE_METHODS`, `BUILDER_METHODS` and
/// `REQUEST_METHODS`, CALL-01).
const V3_CLIENT_METHODS: &[&str] = &[
    "builder",
    "new",
    "unbound",
    "contract",
    "credentials",
    "request",
    "plan",
    "bind",
    "plan_runtime",
    "clone",
];
const V3_SCOPE_METHODS: &[&str] = &["bind", "plan_runtime", "clone"];
const V3_BUILDER_METHODS: &[&str] = &[
    "new",
    "build",
    "configure_http",
    "configure_runtime",
    "retry",
    "default",
];
const V3_REQUEST_METHODS: &[&str] = &[
    "timeout",
    "reset_timeout",
    "cancel_with",
    "with_options",
    "control_summary",
    "description",
    "plan",
    "response",
    "accepted_bytes",
    "pages",
    "collect_prefix",
    "collect_all",
    "into_future",
    "clone",
];

/// One name a v3 generated surface defines: the name, what defines it (for
/// diagnostics) and where it is declared.
type GeneratedName = (String, String, usize);

/// The value controls v3 generates for one declared value (the v3 macro's
/// `controls_for`): a required value of a scope or endpoint has none, a
/// client value always has `set_x` and `set_x_if_some`, an optional value
/// adds `replace_x` and `omit_x`, and an initialized value adds `reset_x`.
fn v3_value_controls(names: &mut Vec<GeneratedName>, value: &ValueDecl, root: bool) {
    let name = &value.name.logical;
    let (replace, omit, reset) = match (value.optional, value.initialized) {
        (false, false) if !root => return,
        (optional, initialized) => (optional, optional, initialized),
    };
    let what = |label: &str| format!("value `{name}` ({label})");
    names.push((format!("set_{name}"), what("its setter"), value.ident_start));
    if replace {
        names.push((
            format!("replace_{name}"),
            what("its exact replacement"),
            value.ident_start,
        ));
    }
    names.push((
        format!("set_{name}_if_some"),
        what("its conditional setter"),
        value.ident_start,
    ));
    if omit {
        names.push((
            format!("omit_{name}"),
            what("its omission control"),
            value.ident_start,
        ));
    }
    if reset {
        names.push((
            format!("reset_{name}"),
            what("its reset control"),
            value.ident_start,
        ));
    }
}

/// The navigation methods v3 generates on an owner handle for its children.
fn v3_child_methods(names: &mut Vec<GeneratedName>, children: &[Child]) {
    for child in children {
        let (kind, name) = match child {
            Child::Scope(scope) => ("scope", &scope.name),
            Child::Endpoint(endpoint) => ("endpoint", &endpoint.name),
        };
        names.push((
            v3_snake_case(&name.logical),
            format!("{kind} `{}` (its navigation method)", name.logical),
            name.range.start,
        ));
    }
}

/// The v3 generated type/module names defined in one owner's support module
/// (mirrors the macro's `generated_types`), with a description of each.
fn generated_names(
    kind: Kind,
    name: &Name,
    client: &Name,
    children: &[Child],
) -> Vec<(String, String)> {
    let display = match kind {
        Kind::Client => format!("client `{}`", name.logical),
        Kind::Scope => format!("scope `{}`", name.logical),
        Kind::Endpoint => return Vec::new(),
    };
    let mut names = vec![(
        "Unbound".to_owned(),
        format!("the unbound handle of {display}"),
    )];
    match kind {
        Kind::Client => {
            names.push((
                "GeneratedEndpoint".to_owned(),
                "the generated intent trait".to_owned(),
            ));
            names.push(("Credentials".to_owned(), "the credential facade".to_owned()));
            names.push((
                format!("{}Builder", client.logical),
                "the client builder".to_owned(),
            ));
        }
        Kind::Scope => names.push((
            v3_pascal_case(&name.logical),
            format!("the handle of {display}"),
        )),
        Kind::Endpoint => {}
    }
    for child in children {
        match child {
            Child::Endpoint(endpoint) => {
                let pascal = v3_pascal_case(&endpoint.name.logical);
                for suffix in ["Endpoint", "Request"] {
                    names.push((
                        format!("{pascal}{suffix}"),
                        format!("a generated type of endpoint `{}`", endpoint.name.logical),
                    ));
                }
            }
            Child::Scope(scope) => names.push((
                v3_snake_case(&scope.name.logical),
                format!("the support module of scope `{}`", scope.name.logical),
            )),
        }
    }
    names
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Kind {
    Client,
    Scope,
    Endpoint,
}

struct Frame<'a> {
    kind: Kind,
    name: &'a Name,
    values: &'a [ValueDecl],
    display: String,
    /// v3 generated names defined in this owner's support module.
    generated: Vec<(String, String)>,
}

struct Resolution<'a> {
    depth: usize,
    value: &'a ValueDecl,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Family {
    Query,
    Headers,
}

impl Family {
    fn normalize(self, key: &str) -> String {
        match self {
            Self::Query => key.to_owned(),
            Self::Headers => key.to_ascii_lowercase(),
        }
    }

    const fn label(self) -> &'static str {
        match self {
            Self::Query => "query",
            Self::Headers => "header",
        }
    }
}

/// Potential key owners of one query/header family along an owner chain,
/// plus the sticky sensitivity taint that no fence or override removes.
#[derive(Clone, Default)]
struct Owned {
    keys: Vec<(String, String)>,
    tainted: Vec<String>,
}

impl Owned {
    fn find(&self, key: &str) -> Option<&str> {
        self.keys
            .iter()
            .find(|(candidate, _)| candidate == key)
            .map(|(_, source)| source.as_str())
    }

    fn insert(&mut self, key: String, source: String) {
        self.remove(&key);
        self.keys.push((key, source));
    }

    fn remove(&mut self, key: &str) {
        self.keys.retain(|(candidate, _)| candidate != key);
    }

    fn clear(&mut self) {
        self.keys.clear();
    }

    fn taint(&mut self, key: &str) {
        if !self.is_tainted(key) {
            self.tainted.push(key.to_owned());
        }
    }

    fn is_tainted(&self, key: &str) -> bool {
        self.tainted.iter().any(|candidate| candidate == key)
    }
}

struct Converter<'a, 'o> {
    source: &'a str,
    file: &'a str,
    settings: &'a Settings,
    imports: &'a Imports,
    client: &'a Name,
    policies: &'a [RatePolicy],
    edits: &'o mut EditSet,
    diagnostics: &'o mut Vec<RawDiagnostic>,
    identities: &'o mut Vec<EndpointIdentity>,
    frames: Vec<Frame<'a>>,
    rates: Vec<(&'a RateAttachment, String)>,
    placed: HashSet<usize>,
    /// `ident_start` of every value with a v3 semantic use (VALUE-06).
    used: HashSet<usize>,
}

impl<'a> Converter<'a, '_> {
    fn client(&mut self, api: &'a Api) {
        let client = &api.client;
        self.frames.push(Frame {
            kind: Kind::Client,
            name: &client.name,
            values: &client.values,
            display: format!("client `{}`", client.name.logical),
            generated: generated_names(Kind::Client, &client.name, &client.name, &client.children),
        });
        self.check_paths(&client.paths, 1);
        let mut builder = Vec::new();
        for value in &client.values {
            v3_value_controls(&mut builder, value, true);
        }
        let display = format!("client `{}`", client.name.logical);
        self.name_collisions(&display, "builder", V3_BUILDER_METHODS, builder);
        let mut handle = Vec::new();
        v3_child_methods(&mut handle, &client.children);
        self.name_collisions(&display, "handle", V3_CLIENT_METHODS, handle);
        self.root_default_timing(client);
        for policy in &client.policies {
            self.policy(policy);
        }
        let (query, headers) = self.site(&client.site, Owned::default(), Owned::default());
        self.children(&client.children, &query, &headers);
        self.frames.pop();
    }

    /// MIGRATE-04 (§4.2, §25.3): v2.1 evaluated a root `#[default]` inside
    /// `build()`, only when the value was not overridden, and again for a
    /// `reset_*` value; v3 evaluates it exactly once when the builder is
    /// created and `reset_*` restores that retained payload. The source needs
    /// no edit, so the tool reports the timing change instead.
    fn root_default_timing(&mut self, client: &'a Client) {
        for value in client.values.iter().filter(|value| value.trait_default) {
            let name = &value.name.logical;
            self.diagnose(RawDiagnostic::new(
                Code::RootDefaultTiming,
                value.ident_start,
                format!(
                    "root value `{name}` of client `{}`: v2.1 ran its `Default::default()` inside `build()` only when `set_{name}` had not overridden it (and again after `reset_{name}`); v3 runs it exactly once when `builder()`/`new()` creates the builder, even if the value is overridden later, and `reset_{name}` restores that retained payload without re-running `Default`. Review defaults that read clocks, the environment or shared counters",
                    client.name.logical
                ),
            ));
        }
    }

    /// N1 (focused review of R3 B01): v3 rejects an owner whose generated
    /// names collide on one surface (`namespace` of `owner`), and generates
    /// names v2.1 did not (the conditional `set_x_if_some`, `replace_x`,
    /// runtime binding, request controls), so a v2.1 declaration can produce
    /// a collision v2.1 never had: `x?` and `x_if_some?` both produce
    /// `set_x_if_some`. The collision is reported at the later declaration;
    /// the conversion itself proceeds as for any other declaration.
    fn name_collisions(
        &mut self,
        owner: &str,
        namespace: &str,
        reserved: &[&str],
        names: Vec<GeneratedName>,
    ) {
        let mut seen = reserved
            .iter()
            .map(|name| {
                (
                    (*name).to_owned(),
                    format!("the generated core method `{name}`"),
                )
            })
            .collect::<Vec<_>>();
        for (name, what, offset) in names {
            if let Some((_, first)) = seen.iter().find(|(known, _)| *known == name) {
                let first = first.clone();
                self.diagnose(RawDiagnostic::new(
                    Code::GeneratedNameCollision,
                    offset,
                    format!(
                        "v3 generates `{name}` on the {namespace} of {owner} both for {first} and for {what}, and rejects this declaration (generated names are never suffixed or truncated; v3 also generates names v2.1 did not, such as `set_x_if_some` and `replace_x`, so a declaration v2.1 accepted may collide). Rename one of them (and its callers) to migrate this declaration"
                    ),
                ));
                continue;
            }
            seen.push((name, what));
        }
    }

    fn children(&mut self, children: &'a [Child], query: &Owned, headers: &Owned) {
        for child in children {
            match child {
                Child::Scope(scope) => self.scope(scope, query.clone(), headers.clone()),
                Child::Endpoint(endpoint) => {
                    self.endpoint(endpoint, query.clone(), headers.clone());
                }
            }
        }
    }

    fn scope(&mut self, scope: &'a Scope, query: Owned, headers: Owned) {
        let logical = scope.name.logical.as_str();
        if logical == "client" || logical == "endpoint" || logical.starts_with("__concord") {
            self.diagnose(
                RawDiagnostic::new(
                    Code::ReservedOwnerName,
                    scope.name.range.start,
                    format!(
                        "scope name `{logical}` is a reserved owner name in v3; renaming it changes navigation and derived endpoint IDs, so choose the new name and pin IDs explicitly"
                    ),
                )
                .suggest(format!("scope {logical}_scope(...) {{ ... }}")),
            );
        }
        let rate_depth = self.rates.len();
        self.frames.push(Frame {
            kind: Kind::Scope,
            name: &scope.name,
            values: &scope.params,
            display: format!("scope `{logical}`"),
            generated: generated_names(Kind::Scope, &scope.name, self.client, &scope.children),
        });
        self.check_paths(&scope.paths, self.frames.len());
        let mut handle = Vec::new();
        for value in &scope.params {
            v3_value_controls(&mut handle, value, false);
        }
        v3_child_methods(&mut handle, &scope.children);
        let display = format!("scope `{logical}`");
        self.name_collisions(&display, "handle", V3_SCOPE_METHODS, handle);
        let (query, headers) = self.site(&scope.site, query, headers);
        self.children(&scope.children, &query, &headers);
        self.frames.pop();
        self.rates.truncate(rate_depth);
    }

    fn endpoint(&mut self, endpoint: &'a Endpoint, query: Owned, headers: Owned) {
        let rate_depth = self.rates.len();
        self.frames.push(Frame {
            kind: Kind::Endpoint,
            name: &endpoint.name,
            values: &endpoint.params,
            display: format!("endpoint `{}`", endpoint.name.logical),
            generated: Vec::new(),
        });
        self.check_paths(&endpoint.paths, self.frames.len() - 1);
        let mut request = Vec::new();
        for value in &endpoint.params {
            v3_value_controls(&mut request, value, false);
        }
        if let Some(body) = endpoint.body.as_ref().filter(|body| body.optional) {
            let name = &body.name.logical;
            let offset = body.name.range.start;
            for (control, label) in [
                (format!("set_{name}"), "its setter"),
                (format!("replace_{name}"), "its exact replacement"),
                (format!("set_{name}_if_some"), "its conditional setter"),
                (format!("omit_{name}"), "its omission control"),
            ] {
                request.push((control, format!("request body `{name}` ({label})"), offset));
            }
        }
        let display = format!("endpoint `{}`", endpoint.name.logical);
        self.name_collisions(&display, "request and intent", V3_REQUEST_METHODS, request);
        self.site(&endpoint.site, query, headers);
        if let Some(pagination) = &endpoint.pagination {
            self.pagination(pagination);
        }
        self.identity(endpoint);
        self.frames.pop();
        self.rates.truncate(rate_depth);
    }

    fn site(&mut self, site: &'a Site, mut query: Owned, mut headers: Owned) -> (Owned, Owned) {
        if let Some(subdomain) = &site.subdomain {
            for atom in &subdomain.atoms {
                self.qualify_atom(atom);
                self.mark_atom(atom);
            }
        }
        if let Some(path) = &site.path {
            for atom in &path.atoms {
                self.mark_atom(atom);
            }
            self.path(path);
        }
        if let Some(clause) = &site.query {
            self.keys(Family::Query, clause, &mut query);
        }
        if let Some(clause) = &site.headers {
            self.keys(Family::Headers, clause, &mut headers);
        }
        let display = self.current().display.clone();
        for attachment in &site.rate_limits {
            self.rate_attachment(attachment);
            self.rates.push((attachment, display.clone()));
        }
        (query, headers)
    }

    fn current(&self) -> &Frame<'a> {
        self.frames.last().expect("an owner frame is active")
    }

    fn diagnose(&mut self, diagnostic: RawDiagnostic) {
        self.diagnostics.push(diagnostic);
    }

    fn record(&mut self, result: Result<(), String>, offset: usize) {
        if let Err(error) = result {
            self.diagnose(RawDiagnostic::new(Code::Internal, offset, error));
        }
    }

    fn lookup(&self, reference: &Reference) -> Result<Resolution<'a>, String> {
        let current = self.frames.len() - 1;
        let namespace = reference.namespace.logical.as_str();
        let depth = match namespace {
            "client" => 0,
            "endpoint" => {
                if self.frames[current].kind != Kind::Endpoint {
                    return Err(format!(
                        "`endpoint.{}` is used outside an endpoint",
                        reference.name.logical
                    ));
                }
                current
            }
            scope => self
                .frames
                .iter()
                .rposition(|frame| frame.kind == Kind::Scope && frame.name.logical == scope)
                .ok_or_else(|| format!("`{scope}` is not a visible scope on this owner chain"))?,
        };
        let frame = &self.frames[depth];
        frame
            .values
            .iter()
            .find(|value| value.name.logical == reference.name.logical)
            .map(|value| Resolution { depth, value })
            .ok_or_else(|| {
                format!(
                    "{} declares no ordinary value `{}`",
                    frame.display, reference.name.logical
                )
            })
    }

    fn resolve(&mut self, reference: &Reference) -> Option<Resolution<'a>> {
        match self.lookup(reference) {
            Ok(resolution) => Some(resolution),
            Err(message) => {
                self.diagnose(RawDiagnostic::new(
                    Code::UnresolvedReference,
                    reference.range.start,
                    format!(
                        "`{}.{}` does not resolve in the v2.1 owner graph: {message}; the reference was left unchanged",
                        reference.namespace.text, reference.name.text
                    ),
                ));
                None
            }
        }
    }

    fn is_current(&self, resolution: &Resolution<'_>) -> bool {
        resolution.depth + 1 == self.frames.len()
    }

    /// Records a v3 semantic use of the referenced value (VALUE-06).
    fn mark(&mut self, reference: &Reference) {
        if let Ok(resolution) = self.lookup(reference) {
            self.used.insert(resolution.value.ident_start);
        }
    }

    fn mark_atom(&mut self, atom: &Atom) {
        match atom {
            Atom::Ref(reference) => self.mark(reference),
            Atom::Adapted { inner, .. } => self.mark_atom(inner),
            Atom::Concat { parts, .. } => {
                for part in parts {
                    self.mark_atom(part);
                }
            }
            Atom::Str { .. } | Atom::Char { .. } | Atom::Bool { .. } | Atom::Int { .. } => {}
        }
    }

    fn mark_emission(&mut self, emission: &Emission) {
        match emission {
            Emission::One(atom) => self.mark_atom(atom),
            Emission::Each { reference, .. } => self.mark(reference),
        }
    }

    /// Rejects declaration paths that v3 would re-resolve inside its
    /// generated support modules: a leading segment equal to a generated
    /// name of the first `through` owner frames, or a relative `self::` /
    /// `super::` prefix. v2.1 resolved these paths in a separate module.
    fn check_paths(&mut self, paths: &[UserPath], through: usize) {
        for path in paths {
            let Some(first) = path.segments.first() else {
                continue;
            };
            let multi = path.segments.len() > 1;
            if path.leading_colon || (path.constant && !multi) {
                continue;
            }
            let rendered = path.rendered();
            let what = if path.constant {
                "constant path"
            } else {
                "type"
            };
            let reason = if multi && (first == "self" || first == "super") {
                Some(format!(
                    "starts with `{first}::`, which v3 would resolve relative to its generated support modules instead of this module"
                ))
            } else {
                self.frames[..through]
                    .iter()
                    .flat_map(|frame| frame.generated.iter())
                    .find(|(name, _)| name == first)
                    .map(|(_, generated)| {
                        format!(
                            "would resolve through {generated} inside the v3 generated support modules (v2.1 resolved declaration paths in a separate module)"
                        )
                    })
            };
            if let Some(reason) = reason {
                let tail = if first == "self" || first == "super" {
                    path.segments[1..].join("::")
                } else {
                    rendered.clone()
                };
                self.diagnose(
                    RawDiagnostic::new(
                        Code::GeneratedShadow,
                        path.range.start,
                        format!("{what} `{rendered}` {reason}; the tool cannot know this module's absolute path"),
                    )
                    .suggest(format!(
                        "crate::<path of this module>::{tail}  // or import it under a name no generated item uses"
                    )),
                );
            }
        }
    }

    /// Diagnoses values without any v3 semantic use (VALUE-06): v2.1 did not
    /// check value liveness; v3 rejects a dead required input and a dead
    /// optional/defaulted input unless it is an explicit deprecated no-op.
    fn liveness(&mut self, api: &Api) {
        fn owner_values<'v>(children: &'v [Child], output: &mut Vec<(&'v ValueDecl, String)>) {
            for child in children {
                match child {
                    Child::Scope(scope) => {
                        for value in &scope.params {
                            output.push((value, format!("scope `{}`", scope.name.logical)));
                        }
                        owner_values(&scope.children, output);
                    }
                    Child::Endpoint(endpoint) => {
                        for value in &endpoint.params {
                            output.push((value, format!("endpoint `{}`", endpoint.name.logical)));
                        }
                    }
                }
            }
        }
        let mut values = api
            .client
            .values
            .iter()
            .map(|value| (value, format!("client `{}`", api.client.name.logical)))
            .collect::<Vec<_>>();
        owner_values(&api.client.children, &mut values);
        for (value, owner) in values {
            if self.used.contains(&value.ident_start) {
                continue;
            }
            let required = !value.optional && !value.initialized;
            if !required && value.deprecated {
                continue;
            }
            let (message, suggestion) = if required {
                (
                    format!(
                        "required value `{}` of {owner} has no wire, route, rate-key or pagination use; v3 rejects a dead required input",
                        value.name.logical
                    ),
                    "remove the value (a caller-visible constructor change) or give it a real use"
                        .to_owned(),
                )
            } else {
                (
                    format!(
                        "value `{}` of {owner} has no wire, route, rate-key or pagination use; v3 rejects it unless it is an explicit deprecated no-op",
                        value.name.logical
                    ),
                    format!(
                        "#[deprecated(note = \"no-op since v3\")] {}  // or remove it",
                        value.name.text
                    ),
                )
            };
            self.diagnose(
                RawDiagnostic::new(Code::DeadValue, value.ident_start, message).suggest(suggestion),
            );
        }
    }

    /// Imports the v2.1 facade-prelude types that declaration types name but
    /// that the invocation's module does not already resolve.
    fn prelude_import(
        &mut self,
        api: &Api,
        input: &DeclarationInput<'_>,
        imported: &mut HashMap<usize, BTreeSet<String>>,
    ) {
        fn collect<'p>(children: &'p [Child], output: &mut Vec<&'p UserPath>) {
            for child in children {
                match child {
                    Child::Scope(scope) => {
                        output.extend(&scope.paths);
                        collect(&scope.children, output);
                    }
                    Child::Endpoint(endpoint) => output.extend(&endpoint.paths),
                }
            }
        }
        let mut paths = api.client.paths.iter().collect::<Vec<_>>();
        collect(&api.client.children, &mut paths);
        let mut needed = paths
            .iter()
            .filter(|path| !path.constant && !path.leading_colon && path.segments.len() == 1)
            .map(|path| path.segments[0].as_str())
            .filter(|name| V2_PRELUDE_TYPES.contains(name))
            .filter(|name| input.scope.is_none_or(|scope| !scope.resolves(name)))
            .map(str::to_owned)
            .collect::<BTreeSet<_>>();
        if needed.is_empty() {
            return;
        }
        let Some(scope) = input.scope else {
            let names = needed.into_iter().collect::<Vec<_>>();
            self.diagnose(
                RawDiagnostic::new(
                    Code::PreludeImportUnresolved,
                    input.offset,
                    format!(
                        "declaration types {} resolved through the v2.1 facade prelude; v3 resolves them in the invocation's scope, which is not a module the tool can edit",
                        names.iter().map(|name| format!("`{name}`")).collect::<Vec<_>>().join(", ")
                    ),
                )
                .suggest(use_line(&names)),
            );
            return;
        };
        let already = imported.entry(scope.module_id).or_default();
        needed.retain(|name| !already.contains(name));
        if needed.is_empty() {
            return;
        }
        already.extend(needed.iter().cloned());
        let names = needed.into_iter().collect::<Vec<_>>();
        let indent = line_indent(self.source, scope.item_start).to_owned();
        let text = format!("{}\n\n{indent}", use_line(&names));
        let result = self.edits.insert(scope.item_start, text);
        self.record(result, scope.item_start);
        self.diagnose(RawDiagnostic::new(
            Code::PreludeImport,
            scope.item_start,
            format!(
                "imported {} explicitly: v2.1 resolved declaration types through the facade prelude, v3 resolves them in this module",
                names.iter().map(|name| format!("`{name}`")).collect::<Vec<_>>().join(", ")
            ),
        ));
    }

    /// Returns the v3 spelling of a resolved reference: bare for the current
    /// owner, exact original qualification for an ancestor.
    fn reference_text(&mut self, reference: &Reference) -> Option<String> {
        let resolution = self.resolve(reference)?;
        Some(if self.is_current(&resolution) {
            reference.name.text.clone()
        } else {
            format!("{}.{}", reference.namespace.text, reference.name.text)
        })
    }

    /// Removes redundant current-owner qualification after resolution.
    fn qualify(&mut self, reference: &Reference) {
        let Some(resolution) = self.resolve(reference) else {
            return;
        };
        if self.is_current(&resolution) {
            let result = self
                .edits
                .replace(reference.range.clone(), reference.name.text.clone());
            self.record(result, reference.range.start);
        }
    }

    fn qualify_atom(&mut self, atom: &Atom) {
        match atom {
            Atom::Ref(reference) => self.qualify(reference),
            Atom::Adapted { inner, .. } => self.qualify_atom(inner),
            Atom::Concat { parts, .. } => {
                for part in parts {
                    self.qualify_atom(part);
                }
            }
            Atom::Str { .. } | Atom::Char { .. } | Atom::Bool { .. } | Atom::Int { .. } => {}
        }
    }

    fn emission_sensitive(&self, emission: &Emission) -> bool {
        fn atom_sensitive(converter: &Converter<'_, '_>, atom: &Atom) -> bool {
            match atom {
                Atom::Ref(reference) => converter
                    .lookup(reference)
                    .is_ok_and(|resolution| resolution.value.sensitive),
                Atom::Adapted { inner, .. } => atom_sensitive(converter, inner),
                Atom::Concat { parts, .. } => {
                    parts.iter().any(|part| atom_sensitive(converter, part))
                }
                Atom::Str { .. } | Atom::Char { .. } | Atom::Bool { .. } | Atom::Int { .. } => {
                    false
                }
            }
        }
        match emission {
            Emission::One(atom) => atom_sensitive(self, atom),
            Emission::Each { reference, .. } => self
                .lookup(reference)
                .is_ok_and(|resolution| resolution.value.sensitive),
        }
    }

    fn qualify_emission(&mut self, emission: &Emission) {
        match emission {
            Emission::One(atom) => self.qualify_atom(atom),
            Emission::Each { reference, .. } => self.qualify(reference),
        }
    }

    // ----- paths -----------------------------------------------------------

    fn path(&mut self, clause: &RouteClause) {
        let Some(list) = &clause.list else {
            return;
        };
        let mut segments = Vec::with_capacity(clause.atoms.len());
        let mut complete = true;
        for atom in &clause.atoms {
            match self.segment(atom, true) {
                Some(segment) => segments.push(segment),
                None => complete = false,
            }
        }
        if !complete {
            return;
        }
        let template = string_literal(&segments.join("/"));
        let result = self.edits.replace_subsuming(list.clone(), template);
        self.record(result, list.start);
    }

    fn segment(&mut self, atom: &Atom, whole_segment: bool) -> Option<String> {
        match atom {
            Atom::Str { value, range } => self.literal_segment(value, range, whole_segment),
            Atom::Char { value, range } => {
                self.literal_segment(&value.to_string(), range, whole_segment)
            }
            Atom::Bool { value, .. } => Some(value.to_string()),
            Atom::Int { text, .. } => Some(text.clone()),
            Atom::Ref(reference) => Some(format!("{{{}}}", self.reference_text(reference)?)),
            Atom::Adapted {
                inner,
                adapter,
                range,
            } => {
                let Atom::Ref(reference) = inner.as_ref() else {
                    self.diagnose(RawDiagnostic::new(
                        Code::PathAdaptedLiteral,
                        range.start,
                        "a v3 path template hole names a declared value; an adapter applied to a literal or `concat` cannot be expressed",
                    ).suggest("declare a value (or constant-initialized value) and adapt it: `path \"{value via Adapter}\";`"));
                    return None;
                };
                let adapter_text = normalize_whitespace(&self.source[adapter.clone()]);
                if adapter_text.contains('{') || adapter_text.contains('}') {
                    self.diagnose(
                        RawDiagnostic::new(
                            Code::PathAdapterBraces,
                            adapter.start,
                            format!(
                                "adapter type `{adapter_text}` contains braces, which cannot appear inside a template hole"
                            ),
                        )
                        .suggest(format!(
                            "type PathAdapter = {adapter_text};  // then: {{{} via PathAdapter}}",
                            reference.name.text
                        )),
                    );
                    return None;
                }
                let reference = self.reference_text(reference)?;
                Some(format!("{{{reference} via {adapter_text}}}"))
            }
            Atom::Concat { parts, .. } => {
                let mut text = String::new();
                let mut complete = true;
                for part in parts {
                    match self.segment(part, false) {
                        Some(part) => text.push_str(&part),
                        None => complete = false,
                    }
                }
                complete.then_some(text)
            }
        }
    }

    fn literal_segment(&mut self, value: &str, range: &Rng, whole_segment: bool) -> Option<String> {
        let invalid = if value.contains('/') || value.contains('\\') {
            Some("contains a path separator")
        } else if value.contains('?') || value.contains('#') {
            Some(
                "contains `?` or `#`, which v3 rejects in template text as likely query/fragment syntax",
            )
        } else if whole_segment && (value.is_empty() || value == "." || value == "..") {
            Some("is empty or a dot segment")
        } else {
            None
        };
        if let Some(reason) = invalid {
            self.diagnose(RawDiagnostic::new(
                Code::PathLiteral,
                range.start,
                format!("literal path segment {value:?} {reason}"),
            ).suggest("declare a value initialized to this text and place it in a hole: `path \"{segment}\";` (the value is percent-encoded as segment data)"));
            return None;
        }
        Some(value.replace('{', "{{").replace('}', "}}"))
    }

    // ----- query and headers -----------------------------------------------

    fn keys(&mut self, family: Family, clause: &'a KeyClause, owned: &mut Owned) {
        match clause.mode {
            ClauseMode::Off => {
                owned.clear();
                return;
            }
            ClauseMode::Replace => owned.clear(),
            ClauseMode::Modify => {}
        }
        let owner = self.current().display.clone();
        let mut introductions = Vec::with_capacity(clause.statements.len());
        for (index, statement) in clause.statements.iter().enumerate() {
            let key = family.normalize(&statement.key);
            if let Some(emission) = &statement.emission {
                // A later plain assignment (v3 `replace`) or `remove` of the
                // same key in this program unconditionally suppresses the
                // emission, so it is not a v3 semantic use (VALUE-06).
                let suppressed = clause.statements[index + 1..].iter().any(|later| {
                    family.normalize(&later.key) == key
                        && matches!(later.operation, KeyOperation::Assign | KeyOperation::Remove)
                });
                if !suppressed {
                    self.mark_emission(emission);
                }
                self.qualify_emission(emission);
                if self.emission_sensitive(emission) {
                    owned.taint(&key);
                } else if owned.is_tainted(&key) {
                    self.diagnose(RawDiagnostic::new(
                        Code::SensitivitySticky,
                        statement.range.start,
                        format!(
                            "{} {:?} carried a `#[sensitive]` value earlier on this owner chain; v3 keeps redacting it in plans, errors and observers even though this contribution is not sensitive (the real request is unchanged)",
                            family.label(),
                            statement.key
                        ),
                    ));
                }
            }
            let earlier = owned.find(&key).map(str::to_owned);
            let source = format!("{owner} {} statement", family.label());
            match statement.operation {
                KeyOperation::Assign => {
                    if let Some(earlier) = earlier {
                        let result = self.edits.insert(statement.key_range.start, "replace ");
                        self.record(result, statement.key_range.start);
                        self.diagnose(RawDiagnostic::new(
                            Code::KeyReplace,
                            statement.range.start,
                            format!(
                                "plain v2.1 {} assignment of {:?} clears an earlier potential owner ({earlier}); v3 spells that exact replacement `replace`",
                                family.label(),
                                statement.key
                            ),
                        ));
                        introductions.push(false);
                    } else {
                        introductions.push(true);
                    }
                    owned.insert(key, source);
                }
                KeyOperation::WhenPresent => {
                    if earlier.is_none() {
                        let suggestion = self
                            .edits
                            .render(self.source, statement.key_range.start..statement.range.end);
                        self.diagnose(RawDiagnostic::new(
                            Code::WhenPresentWithoutFallback,
                            statement.range.start,
                            format!(
                                "`when_present {:?}` has no earlier potential owner to fall back to; v3 rejects a conditional override without a fallback",
                                statement.key
                            ),
                        ).suggest(format!("{suggestion}  // plain introduction: same wire behavior in v2.1")));
                    }
                    introductions.push(false);
                    owned.insert(key, source);
                }
                KeyOperation::Append => {
                    introductions.push(false);
                    owned.insert(key, source);
                }
                KeyOperation::Remove => {
                    if earlier.is_none() {
                        self.diagnose(RawDiagnostic::new(
                            Code::RemoveWithoutOwner,
                            statement.range.start,
                            format!(
                                "`remove {:?}` targets a key with no earlier potential owner; it removed nothing in v2.1 and v3 rejects it",
                                statement.key
                            ),
                        ).suggest("delete this statement (or restore the owner it was meant to remove)"));
                    } else {
                        owned.remove(&key);
                    }
                    introductions.push(false);
                }
            }
        }
        if self.settings.direct_placement {
            self.direct_placement(family, clause, &introductions);
        }
    }

    /// Converts the leading run of simple plain introductions of local values
    /// into direct placements. Only a prefix is converted, and only while the
    /// values appear in declaration order, because v3 normalizes direct
    /// placements before explicit statements in value declaration order.
    fn direct_placement(&mut self, family: Family, clause: &'a KeyClause, introductions: &[bool]) {
        if clause.mode == ClauseMode::Off {
            return;
        }
        let mut converted: Vec<(&KeyStatement, &'a ValueDecl, Option<Rng>, bool)> = Vec::new();
        let mut last_position = None;
        for (statement, introduction) in clause.statements.iter().zip(introductions) {
            if !introduction {
                break;
            }
            let (reference, adapter, each) = match &statement.emission {
                Some(Emission::One(Atom::Ref(reference))) => (reference, None, false),
                Some(Emission::One(Atom::Adapted { inner, adapter, .. })) => match inner.as_ref() {
                    Atom::Ref(reference) => (reference, Some(adapter.clone()), false),
                    _ => break,
                },
                Some(Emission::Each { reference, adapter }) => (reference, adapter.clone(), true),
                _ => break,
            };
            let Ok(resolution) = self.lookup(reference) else {
                break;
            };
            if !self.is_current(&resolution)
                || self.placed.contains(&resolution.value.ident_start)
                || converted
                    .iter()
                    .any(|(_, value, _, _)| value.ident_start == resolution.value.ident_start)
                || last_position.is_some_and(|last| resolution.value.position <= last)
            {
                break;
            }
            last_position = Some(resolution.value.position);
            converted.push((statement, resolution.value, adapter, each));
        }
        if converted.is_empty() {
            return;
        }
        for (statement, value, adapter, each) in &converted {
            let key_source = &self.source[statement.key_range.clone()];
            let mut placement = match family {
                Family::Query if statement.key == value.name.logical => "query ".to_owned(),
                Family::Query => format!("query({key_source}) "),
                Family::Headers => format!("header({key_source}) "),
            };
            if *each {
                placement.push_str("each ");
            }
            let result = self.edits.insert(value.ident_start, placement);
            self.record(result, value.ident_start);
            if let Some(adapter) = adapter {
                let text = format!(" via {}", &self.source[adapter.clone()]);
                let result = self.edits.insert(value.type_end, text);
                self.record(result, value.type_end);
            }
            self.placed.insert(value.ident_start);
        }
        if converted.len() == clause.statements.len() && clause.mode == ClauseMode::Modify {
            let range = removal_range(self.source, clause.range.clone());
            let result = self.edits.replace_subsuming(range, "");
            self.record(result, clause.range.start);
        } else {
            for (statement, ..) in &converted {
                let range = removal_range(self.source, statement.range.clone());
                let result = self.edits.replace_subsuming(range, "");
                self.record(result, statement.range.start);
            }
        }
    }

    // ----- rate limits -----------------------------------------------------

    fn policy(&mut self, policy: &'a RatePolicy) {
        for clause in &policy.header {
            if let HeaderClause::Key { dimensions, .. } = clause {
                for dimension in dimensions {
                    if let FormalDimension::Formal {
                        name,
                        adapter,
                        range,
                    } = dimension
                    {
                        let label = format!("{} = ", string_literal(&name.logical));
                        let result = self.edits.insert(range.start, label);
                        self.record(result, range.start);
                        if let Some(adapter) = adapter {
                            self.rate_adapter(adapter);
                        }
                    }
                }
            }
        }
        self.reorder_header(&policy.header);
        self.windows(&policy.windows, header_id(&policy.header));
    }

    fn rate_attachment(&mut self, attachment: &'a RateAttachment) {
        match attachment {
            RateAttachment::Inline(rate) => {
                for clause in &rate.header {
                    if let HeaderClause::Key { dimensions, .. } = clause {
                        for dimension in dimensions {
                            if let Dimension::Value {
                                reference,
                                adapter,
                                range,
                            } = dimension
                            {
                                // The v2.1 inline label is the owner-qualified
                                // spelling; it is preserved byte-for-byte.
                                let label = format!(
                                    "{}.{}",
                                    reference.namespace.logical, reference.name.logical
                                );
                                let result = self
                                    .edits
                                    .insert(range.start, format!("{} = ", string_literal(&label)));
                                self.record(result, range.start);
                                self.mark(reference);
                                self.qualify(reference);
                                if let Some(adapter) = adapter {
                                    self.rate_adapter(adapter);
                                }
                            }
                        }
                    }
                }
                self.reorder_header(&rate.header);
                self.windows(&rate.windows, header_id(&rate.header));
            }
            RateAttachment::Named(named) => {
                if !self
                    .policies
                    .iter()
                    .any(|policy| policy.name.logical == named.policy.logical)
                {
                    self.diagnose(RawDiagnostic::new(
                        Code::RateUnknownPolicy,
                        named.range.start,
                        format!(
                            "rate policy `{}` is not defined in the client `rate_limits` block",
                            named.policy.logical
                        ),
                    ));
                }
                for (_, reference) in &named.bindings {
                    self.mark(reference);
                    self.qualify(reference);
                }
            }
        }
    }

    fn rate_adapter(&mut self, adapter: &Rng) {
        let text = normalize_whitespace(&self.source[adapter.clone()]);
        self.diagnose(
            RawDiagnostic::new(
                Code::RateKeyEncoder,
                adapter.start,
                format!(
                    "rate key adapter `{text}` must implement v3 `RateKeyEncoder` with a stable `ENCODING_ID`; the tool cannot prove its canonical bytes are unchanged"
                ),
            )
            .suggest(format!(
                "impl RateKeyEncoder<T> for {text} {{ const ENCODING_ID: &'static str = \"<stable-name>.v1\"; fn write_canonical(..) {{ .. }} }}"
            )),
        );
    }

    /// v3 fixes the header order to `id; key; cost;`. Clause texts (with their
    /// inner edits) are moved between the original clause slots so separators
    /// and comments stay in place.
    fn reorder_header<D>(&mut self, header: &[HeaderClause<D>]) {
        let in_order = header
            .windows(2)
            .all(|pair| pair[0].canonical_rank() <= pair[1].canonical_rank());
        if in_order || header.is_empty() {
            return;
        }
        let texts = header
            .iter()
            .map(|clause| self.edits.render(self.source, clause.range()))
            .collect::<Vec<_>>();
        let separators = header
            .windows(2)
            .map(|pair| self.source[pair[0].range().end..pair[1].range().start].to_owned())
            .collect::<Vec<_>>();
        let mut order = (0..header.len()).collect::<Vec<_>>();
        order.sort_by_key(|index| header[*index].canonical_rank());
        let mut text = String::new();
        for (slot, index) in order.iter().enumerate() {
            text.push_str(&texts[*index]);
            if let Some(separator) = separators.get(slot) {
                text.push_str(separator);
            }
        }
        let range = header[0].range().start..header[header.len() - 1].range().end;
        let result = self.edits.replace_subsuming(range.clone(), text);
        self.record(result, range.start);
    }

    fn windows(&mut self, windows: &[Window], id: Option<&str>) {
        for (index, window) in windows.iter().enumerate() {
            if windows[..index]
                .iter()
                .any(|earlier| earlier.duration_ms == window.duration_ms)
            {
                self.diagnose(RawDiagnostic::new(
                    Code::RateWindowDuplicate,
                    window.range.start,
                    format!(
                        "rate policy {} repeats a {} ms window duration; v3 rejects contradictory capacities under one duration, and choosing one changes admission",
                        id.map_or_else(|| "without an ID".to_owned(), |id| format!("{id:?}")),
                        window.duration_ms
                    ),
                ).suggest("keep exactly one `allow N per D;` for this duration (the tighter capacity preserves v2.1 admission)"));
            }
        }
    }

    // ----- pagination -------------------------------------------------------

    fn pagination(&mut self, pagination: &'a Pagination) {
        for field in &pagination.fields {
            if let PageValue::Live(reference) | PageValue::Reference(reference) = &field.value {
                self.mark(reference);
            }
        }
        let Some(builtin) = self.builtin_controller(pagination) else {
            let controller = normalize_whitespace(&self.source[pagination.controller.clone()]);
            let fields = pagination
                .fields
                .iter()
                .map(|field| format!("{}: ..", field.name.text))
                .collect::<Vec<_>>()
                .join(", ");
            self.diagnose(
                RawDiagnostic::new(
                    Code::PaginationCustomController,
                    pagination.controller.start,
                    format!(
                        "`{controller}` is not a built-in Concord controller; v3 requires it to expose a public `Init` type (separate from private state) and a whole `Cursor` mapping, which the tool cannot invent"
                    ),
                )
                .suggest(format!(
                    "paginate via {controller} {{ init {controller}Init {{ {fields} }}; cursor -> <endpoint slot>; }}"
                )),
            );
            return;
        };
        let mut values = Vec::new();
        for field in &pagination.fields {
            if !builtin.fields.contains(&field.name.logical.as_str()) {
                self.pagination_error(
                    field.name.range.start,
                    format!(
                        "`{}` is not a field of the v3 `{}` Init type (fields: {})",
                        field.name.logical,
                        builtin.init,
                        builtin.fields.join(", ")
                    ),
                );
                return;
            }
        }
        let mut cursor = None;
        for field_name in builtin.fields {
            let Some(field) = pagination
                .fields
                .iter()
                .find(|field| field.name.logical == *field_name)
            else {
                self.pagination_error(
                    pagination.controller.start,
                    format!(
                        "built-in `{}` requires `{field_name}`; v2.1 did not bind it",
                        builtin.init
                    ),
                );
                return;
            };
            let text = match &field.value {
                PageValue::Live(reference) => {
                    let Some(resolution) = self.resolve(reference) else {
                        return;
                    };
                    if !self.is_current(&resolution) {
                        self.pagination_error(
                            reference.range.start,
                            "a pagination destination must be an endpoint-owned slot".to_owned(),
                        );
                        return;
                    }
                    if *field_name == builtin.cursor {
                        cursor = Some(reference.name.text.clone());
                    }
                    reference.name.text.clone()
                }
                PageValue::Reference(reference) => {
                    let Some(text) = self.reference_text(reference) else {
                        return;
                    };
                    text
                }
                PageValue::Stable(range) => self.source[range.clone()].to_owned(),
            };
            values.push(format!("{field_name}: {text}"));
        }
        let Some(cursor) = cursor else {
            self.pagination_error(
                pagination.controller.start,
                format!(
                    "the continuation field `{}` of `{}` is not bound to an endpoint slot (`{} -> endpoint.slot`), so there is no v3 `cursor ->` destination",
                    builtin.cursor, builtin.init, builtin.cursor
                ),
            );
            return;
        };
        let prefix = pagination.last_segment.as_ref().map_or("", |last| {
            &self.source[pagination.controller.start..last.start]
        });
        let generics = match builtin.generics {
            InitGenerics::None => String::new(),
            InitGenerics::String => "<String>".to_owned(),
            InitGenerics::FromController => pagination
                .generics
                .as_ref()
                .map(|range| self.source[range.clone()].to_owned())
                .unwrap_or_default(),
        };
        let init = format!("{prefix}{}{generics}", builtin.init);
        let body = [
            format!("init {init} {{ {} }};", values.join(", ")),
            format!("cursor -> {cursor};"),
        ];
        let open = pagination.brace_open.end;
        let close = pagination.brace_close.start;
        let multiline = self.source[open..close].contains('\n');
        let text = if multiline {
            let close_indent = line_indent(self.source, close).to_owned();
            let field_indent = pagination.fields.first().map_or_else(
                || format!("{close_indent}    "),
                |field| line_indent(self.source, field.name.range.start).to_owned(),
            );
            format!(
                "\n{field_indent}{}\n{field_indent}{}\n{close_indent}",
                body[0], body[1]
            )
        } else {
            format!(" {} {} ", body[0], body[1])
        };
        let result = self.edits.replace_subsuming(open..close, text);
        self.record(result, open);
        if !builtin.path_imported(prefix, self.imports) {
            self.diagnose(RawDiagnostic::new(
                Code::PaginationInit,
                pagination.controller.start,
                format!(
                    "converted to public Init `{init}`; import `{}` next to the controller if it is not already in scope",
                    builtin.init
                ),
            ));
        } else {
            self.diagnose(RawDiagnostic::new(
                Code::PaginationInit,
                pagination.controller.start,
                format!(
                    "converted to public Init `{init}` with `cursor -> {cursor}` (the whole v3 Cursor maps to that slot)"
                ),
            ));
        }
    }

    fn pagination_error(&mut self, offset: usize, message: String) {
        self.diagnose(
            RawDiagnostic::new(Code::PaginationField, offset, message)
                .suggest("paginate via Controller { init ControllerInit { .. }; cursor -> slot; }"),
        );
    }

    fn builtin_controller(&self, pagination: &Pagination) -> Option<&'static Builtin> {
        let segments = &pagination.controller_segments;
        let last = segments.last()?;
        let builtin = BUILTINS.iter().find(|builtin| builtin.controller == last)?;
        if segments.len() > 1 {
            let first = segments.first()?;
            return is_concord_crate(first).then_some(builtin);
        }
        match self.imports.resolve(last) {
            Some(full) => full
                .first()
                .is_some_and(|first| is_concord_crate(first))
                .then_some(builtin),
            None if self.imports.defines_local_type(last) => None,
            None => self.imports.has_concord_glob().then_some(builtin),
        }
    }

    // ----- identity ----------------------------------------------------------

    fn identity(&mut self, endpoint: &'a Endpoint) {
        let scopes = self.frames[1..self.frames.len() - 1]
            .iter()
            .map(|frame| frame.name.logical.clone())
            .collect::<Vec<_>>();
        let tail = scopes
            .iter()
            .cloned()
            .chain(std::iter::once(endpoint.name.logical.clone()))
            .collect::<Vec<_>>()
            .join(".");
        let v2_derived = format!("{}.{tail}", v2_snake_case(&self.client.logical));
        let v3_derived = format!("{}.{tail}", v3_snake_case(&self.client.logical));
        let endpoint_dimension = self
            .rates
            .iter()
            .any(|(attachment, _)| self.uses_endpoint_dimension(attachment));
        let display_path = std::iter::once(self.client.logical.clone())
            .chain(scopes.iter().cloned())
            .chain(std::iter::once(endpoint.name.logical.clone()))
            .collect::<Vec<_>>()
            .join("::");
        let (v2_id, explicit) = endpoint
            .id
            .as_ref()
            .map_or((v2_derived.clone(), false), |(id, _)| (id.clone(), true));
        let mut pinned = false;
        let mut v3_id = v2_id.clone();
        if !explicit {
            let reason = if v3_derived != v2_derived {
                Some(format!(
                    "the v3 navigation derivation `{v3_derived}` differs from the v2.1 derivation"
                ))
            } else if !valid_endpoint_id(&v3_derived) {
                Some(format!(
                    "the derived ID `{v3_derived}` is not a valid v3 operational ID"
                ))
            } else if endpoint_dimension {
                Some(
                    "an effective rate policy uses the `endpoint` key dimension, which requires an explicit ID in v3"
                        .to_owned(),
                )
            } else {
                None
            };
            match reason {
                Some(reason) if valid_endpoint_id(&v2_id) => {
                    self.pin_id(endpoint, &v2_id);
                    pinned = true;
                    self.diagnose(RawDiagnostic::new(
                        Code::IdPin,
                        endpoint.name.range.start,
                        format!("pinned `id {v2_id:?}` because {reason}"),
                    ));
                }
                Some(reason) => {
                    v3_id = v3_derived.clone();
                    self.diagnose(RawDiagnostic::new(
                        Code::IdInvalid,
                        endpoint.name.range.start,
                        format!(
                            "endpoint needs an explicit ID because {reason}, but the v2.1 ID `{v2_id}` is not a valid v3 ID; choosing a new ID is a reviewed state/identity reset"
                        ),
                    ).suggest("id \"<reviewed.valid.id>\";"));
                }
                None => v3_id = v3_derived.clone(),
            }
        }
        let policies = self
            .rates
            .iter()
            .filter_map(|(attachment, owner)| self.policy_identity(attachment, owner))
            .collect();
        self.identities.push(EndpointIdentity {
            file: self.file.to_owned(),
            display_path,
            method: endpoint.method.clone(),
            v2_id,
            v3_id,
            explicit,
            pinned,
            policies,
        });
    }

    fn pin_id(&mut self, endpoint: &Endpoint, id: &str) {
        let clause = format!("id {};", string_literal(id));
        // Insert directly after `{`: a first clause may itself be removed by a
        // direct-placement conversion, so its range is not a stable anchor.
        let result = if let Some((open, close)) = &endpoint.clause_block {
            match endpoint.clause_ranges.first() {
                Some(first) if self.source[open.end..first.start].contains('\n') => {
                    let indent = line_indent(self.source, first.start).to_owned();
                    self.edits.insert(open.end, format!("\n{indent}{clause}"))
                }
                Some(_) => self.edits.insert(open.end, format!(" {clause}")),
                None => self
                    .edits
                    .replace(open.end..close.start, format!(" {clause} ")),
            }
        } else if let Some(semicolon) = &endpoint.semicolon {
            let indent = line_indent(self.source, endpoint.method_range.start).to_owned();
            self.edits.replace(
                semicolon.clone(),
                format!(" {{\n{indent}    {clause}\n{indent}}}"),
            )
        } else {
            Ok(())
        };
        self.record(result, endpoint.name.range.start);
    }

    fn named_policy(&self, name: &str) -> Option<&'a RatePolicy> {
        self.policies
            .iter()
            .find(|policy| policy.name.logical == name)
    }

    fn uses_endpoint_dimension(&self, attachment: &RateAttachment) -> bool {
        match attachment {
            RateAttachment::Inline(rate) => rate.header.iter().any(|clause| {
                matches!(clause, HeaderClause::Key { dimensions, .. }
                    if dimensions.iter().any(|dimension| matches!(dimension, Dimension::Endpoint)))
            }),
            RateAttachment::Named(named) => {
                self.named_policy(&named.policy.logical)
                    .is_some_and(|policy| {
                        policy.header.iter().any(|clause| {
                            matches!(clause, HeaderClause::Key { dimensions, .. }
                                if dimensions.iter().any(|dimension| matches!(dimension, FormalDimension::Endpoint)))
                        })
                    })
            }
        }
    }

    fn policy_identity(&self, attachment: &RateAttachment, owner: &str) -> Option<PolicyIdentity> {
        let windows = |windows: &[Window]| {
            windows
                .iter()
                .map(|window| (window.capacity, window.duration_ms))
                .collect::<Vec<_>>()
        };
        match attachment {
            RateAttachment::Inline(rate) => Some(PolicyIdentity {
                id: header_id(&rate.header)?.to_owned(),
                cost: header_cost(&rate.header).unwrap_or(1),
                windows: windows(&rate.windows),
                labels: header_dimensions(&rate.header)
                    .iter()
                    .map(|dimension| match dimension {
                        Dimension::Origin => "origin".to_owned(),
                        Dimension::Endpoint => "endpoint".to_owned(),
                        Dimension::Value { reference, .. } => {
                            format!("{}.{}", reference.namespace.logical, reference.name.logical)
                        }
                    })
                    .collect(),
                attachment: format!("{owner} inline"),
            }),
            RateAttachment::Named(named) => {
                let policy = self.named_policy(&named.policy.logical)?;
                Some(PolicyIdentity {
                    id: header_id(&policy.header)?.to_owned(),
                    cost: named.cost.or(header_cost(&policy.header)).unwrap_or(1),
                    windows: windows(&policy.windows),
                    labels: header_dimensions(&policy.header)
                        .iter()
                        .map(|dimension| match dimension {
                            FormalDimension::Origin => "origin".to_owned(),
                            FormalDimension::Endpoint => "endpoint".to_owned(),
                            FormalDimension::Formal { name, .. } => name.logical.clone(),
                        })
                        .collect(),
                    attachment: format!("{owner} policy `{}`", named.policy.logical),
                })
            }
        }
    }
}

fn header_id<D>(header: &[HeaderClause<D>]) -> Option<&str> {
    header.iter().find_map(|clause| match clause {
        HeaderClause::Id { value, .. } => Some(value.as_str()),
        _ => None,
    })
}

fn header_cost<D>(header: &[HeaderClause<D>]) -> Option<u32> {
    header.iter().find_map(|clause| match clause {
        HeaderClause::Cost { value, .. } => Some(*value),
        _ => None,
    })
}

fn header_dimensions<D>(header: &[HeaderClause<D>]) -> &[D] {
    header
        .iter()
        .find_map(|clause| match clause {
            HeaderClause::Key { dimensions, .. } => Some(dimensions.as_slice()),
            _ => None,
        })
        .unwrap_or(&[])
}

#[derive(Clone, Copy)]
enum InitGenerics {
    None,
    String,
    FromController,
}

/// A first-party v2.1 controller and its selected v3 Init/Cursor contract.
struct Builtin {
    controller: &'static str,
    init: &'static str,
    fields: &'static [&'static str],
    cursor: &'static str,
    generics: InitGenerics,
}

impl Builtin {
    fn path_imported(&self, prefix: &str, imports: &Imports) -> bool {
        !prefix.is_empty() || imports.resolve(self.init).is_some() || imports.has_concord_glob()
    }
}

const BUILTINS: &[Builtin] = &[
    Builtin {
        controller: "OffsetPagination",
        init: "OffsetInit",
        fields: &["offset", "page_size"],
        cursor: "offset",
        generics: InitGenerics::None,
    },
    Builtin {
        controller: "PagePagination",
        init: "PageInit",
        fields: &["page", "page_size"],
        cursor: "page",
        generics: InitGenerics::None,
    },
    Builtin {
        controller: "CursorPagination",
        init: "CursorInit",
        fields: &["cursor"],
        cursor: "cursor",
        generics: InitGenerics::FromController,
    },
    Builtin {
        controller: "NextHeaderPagination",
        init: "CursorInit",
        fields: &["cursor"],
        cursor: "cursor",
        generics: InitGenerics::String,
    },
    Builtin {
        controller: "LinkCursorPagination",
        init: "CursorInit",
        fields: &["cursor"],
        cursor: "cursor",
        generics: InitGenerics::String,
    },
];

/// `use concord::prelude::Name;` or `use concord::prelude::{A, B};`.
fn use_line(names: &[String]) -> String {
    match names {
        [single] => format!("use concord::prelude::{single};"),
        _ => format!("use concord::prelude::{{{}}};", names.join(", ")),
    }
}

/// Renders a cooked Rust string literal whose decoded value is `value`.
pub(crate) fn string_literal(value: &str) -> String {
    let mut output = String::with_capacity(value.len() + 2);
    output.push('"');
    for character in value.chars() {
        match character {
            '"' => output.push_str("\\\""),
            '\\' => output.push_str("\\\\"),
            '\n' => output.push_str("\\n"),
            '\r' => output.push_str("\\r"),
            '\t' => output.push_str("\\t"),
            character if character.is_control() => {
                output.push_str(&format!("\\u{{{:x}}}", u32::from(character)));
            }
            character => output.push(character),
        }
    }
    output.push('"');
    output
}

fn normalize_whitespace(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The v2.1 macro's generated-name snake case (Unicode case predicates).
pub(crate) fn v2_snake_case(name: &str) -> String {
    let name = name.strip_prefix("r#").unwrap_or(name);
    let characters = name.chars().collect::<Vec<_>>();
    let mut output = String::with_capacity(name.len() + 4);
    for (index, character) in characters.iter().copied().enumerate() {
        if character.is_uppercase() {
            let previous_is_lowercase = index > 0 && characters[index - 1].is_lowercase();
            let next_is_lowercase = characters
                .get(index + 1)
                .is_some_and(|next| next.is_lowercase());
            if index > 0 && (previous_is_lowercase || next_is_lowercase) {
                output.push('_');
            }
            output.extend(character.to_lowercase());
        } else {
            output.push(character);
        }
    }
    output
}

/// The v3 CALL-01 snake case: `_` before an uppercase letter that follows a
/// lowercase letter or digit, and before the last uppercase letter of an
/// acronym followed by a lowercase letter; then lowercase.
pub(crate) fn v3_snake_case(name: &str) -> String {
    let name = name.strip_prefix("r#").unwrap_or(name);
    let characters = name.chars().collect::<Vec<_>>();
    let mut output = String::with_capacity(name.len() + 4);
    for (index, character) in characters.iter().copied().enumerate() {
        if character.is_ascii_uppercase() && index > 0 {
            let previous = characters[index - 1];
            let next_is_lowercase = characters
                .get(index + 1)
                .is_some_and(char::is_ascii_lowercase);
            if previous.is_ascii_lowercase()
                || previous.is_ascii_digit()
                || (previous.is_ascii_uppercase() && next_is_lowercase)
            {
                output.push('_');
            }
        }
        output.push(character.to_ascii_lowercase());
    }
    output
}

/// The v3 operational endpoint ID grammar (IDENTITY-01).
pub(crate) fn valid_endpoint_id(id: &str) -> bool {
    (1..=128).contains(&id.len())
        && id.is_ascii()
        && id.split('.').all(|segment| {
            let bytes = segment.as_bytes();
            (1..=63).contains(&bytes.len())
                && bytes.iter().all(|byte| {
                    byte.is_ascii_lowercase()
                        || byte.is_ascii_digit()
                        || matches!(byte, b'_' | b'-')
                })
                && bytes
                    .first()
                    .is_some_and(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
                && bytes
                    .last()
                    .is_some_and(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snake_case_derivations_differ_only_where_the_rules_differ() {
        assert_eq!(v2_snake_case("CatalogApi"), "catalog_api");
        assert_eq!(v3_snake_case("CatalogApi"), "catalog_api");
        assert_eq!(v2_snake_case("HTTPClient"), "http_client");
        assert_eq!(v3_snake_case("HTTPClient"), "http_client");
        assert_eq!(v2_snake_case("RiotApi"), "riot_api");
        // Digit before an acronym: v3 inserts the separator, v2.1 did not.
        assert_eq!(v2_snake_case("Api2URL"), "api2url");
        assert_eq!(v3_snake_case("Api2URL"), "api2_url");
        // An underscore before a capital doubled the separator in v2.1.
        assert_eq!(v2_snake_case("Foo_Bar"), "foo__bar");
        assert_eq!(v3_snake_case("Foo_Bar"), "foo_bar");
    }

    #[test]
    fn endpoint_id_grammar_matches_identity_01() {
        assert!(valid_endpoint_id("catalog.item.get"));
        assert!(valid_endpoint_id(
            "riot_api.platform.champion_v3.champion_rotations"
        ));
        assert!(!valid_endpoint_id("api.Item"));
        assert!(!valid_endpoint_id("api._private"));
        assert!(!valid_endpoint_id("api..x"));
        assert!(!valid_endpoint_id(&format!("a.{}", "b".repeat(64))));
    }

    #[test]
    fn string_literals_round_trip_escapes() {
        assert_eq!(string_literal("items/{id}"), "\"items/{id}\"");
        assert_eq!(string_literal("a\"b\\c"), "\"a\\\"b\\\\c\"");
    }
}
