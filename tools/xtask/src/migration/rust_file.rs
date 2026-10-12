//! Rust-file parsing, import resolution and `api!` invocation discovery.

use std::collections::{HashMap, HashSet};

use proc_macro2::TokenStream;
use syn::visit::Visit;

/// A fully parsed Rust file plus the byte offset of the parsed text.
pub(crate) struct ParsedFile {
    pub(crate) file: syn::File,
    /// Offset added to every `proc_macro2` byte range (BOM/shebang prefix).
    pub(crate) base: usize,
}

/// The byte length of the BOM/shebang prefix that `syn::parse_file` strips.
pub(crate) fn stripped_prefix(source: &str) -> usize {
    let mut base = 0;
    let mut content = source;
    if let Some(rest) = content.strip_prefix('\u{feff}') {
        base += '\u{feff}'.len_utf8();
        content = rest;
    }
    if content.starts_with("#!") && !content[2..].trim_start().starts_with('[') {
        base += content.find('\n').unwrap_or(content.len());
    }
    base
}

/// The deepest nesting the tool examines: delimiters (`(`, `[`, `{`) plus the
/// generic-argument brackets (`<`) open at a point. Parsing and every
/// syntax-tree walk recurse per nesting level, so a deeper file is reported
/// (`C3-MIGRATE-NESTING-LIMIT`) instead of being examined; the migration runs
/// on a stack sized for this bound (see `migrate`).
pub(crate) const MAX_NESTING: usize = 256;

/// Why a Rust file was not examined.
pub(crate) enum ParseFailure {
    /// The text is not valid Rust.
    Syntax(syn::Error),
    /// Delimiters and generic brackets nest deeper than [`MAX_NESTING`];
    /// `offset` is the opening delimiter or `<` (in the original text) that
    /// exceeds the bound.
    Nesting { offset: usize },
}

/// Parses a complete Rust file exactly like `syn::parse_file`, but records
/// the prefix that was stripped so spans map back to original byte offsets.
/// A file nested deeper than [`MAX_NESTING`] is refused before `syn` recurses
/// into it (the lexer used for the check is iterative).
pub(crate) fn parse_rust(source: &str) -> Result<ParsedFile, ParseFailure> {
    let base = stripped_prefix(source);
    if let Some(offset) = excessive_nesting(&source[base..]) {
        return Err(ParseFailure::Nesting {
            offset: offset + base,
        });
    }
    let file = syn::parse_str::<syn::File>(&source[base..]).map_err(ParseFailure::Syntax)?;
    Ok(ParsedFile { file, base })
}

/// One delimiter level of [`excessive_nesting`]: its remaining tokens and
/// the generic brackets open in it.
struct NestingLevel {
    tokens: proc_macro2::token_stream::IntoIter,
    open: usize,
}

/// The generic brackets one operator (a run of joint punctuation, fed one
/// character at a time) opens and closes, and whether it ends the current run
/// of brackets. Every `<` opens except the one of `<=`; every `>` closes
/// except those of `->`, `=>` and `>=`; an operator that never occurs inside
/// generic arguments (with `;`, `|`, `.`, `%`, `^`, `==` or `!=`) ends the
/// run, so comparisons chained with `||` or in separate statements do not add
/// up.
#[derive(Default)]
struct Operator {
    /// The operator's first punctuation, for the offset of a report.
    first: Option<proc_macro2::Punct>,
    previous: Option<char>,
    /// A `<` or `>` whose meaning depends on the next character.
    pending: Option<char>,
    opens: usize,
    closes: usize,
    reset: bool,
}

impl Operator {
    fn feed(&mut self, punct: &proc_macro2::Punct) {
        let character = punct.as_char();
        let joint = punct.spacing() == proc_macro2::Spacing::Joint;
        if self.first.is_none() {
            self.first = Some(punct.clone());
        }
        match self.pending.take() {
            Some('<') if character != '=' => self.opens += 1,
            Some('>') if character != '=' => self.closes += 1,
            _ => {}
        }
        match character {
            ';' | '|' | '.' | '%' | '^' => self.reset = true,
            '=' if matches!(self.previous, Some('=' | '!')) => self.reset = true,
            '<' if joint => self.pending = Some('<'),
            '<' => self.opens += 1,
            '>' if matches!(self.previous, Some('-' | '=')) => {}
            '>' if joint => self.pending = Some('>'),
            '>' => self.closes += 1,
            _ => {}
        }
        self.previous = Some(character);
    }
}

/// The offset of the first opening delimiter or generic bracket nested
/// deeper than [`MAX_NESTING`], found without recursion. A lexing error is
/// left to `syn`, which reports it before building any tree.
///
/// A generic bracket is not a token tree, so the open `<` of each delimiter
/// level are counted (see [`Operator`]); the count is an upper bound of the
/// generic nesting within one statement, which `syn` parses (and drops)
/// recursively.
fn excessive_nesting(text: &str) -> Option<usize> {
    use proc_macro2::{Spacing, TokenTree};
    let tokens = text.parse::<TokenStream>().ok()?;
    let mut stack = vec![NestingLevel {
        tokens: tokens.into_iter(),
        open: 0,
    }];
    let mut angles = 0usize;
    let mut operator = Operator::default();
    loop {
        let depth = stack.len();
        let Some(level) = stack.last_mut() else {
            break;
        };
        let tree = level.tokens.next();
        if let Some(TokenTree::Punct(punct)) = &tree {
            operator.feed(punct);
            if punct.spacing() == Spacing::Joint {
                continue;
            }
        }
        if let Some(first) = operator.first.take() {
            let done = std::mem::take(&mut operator);
            let opens = done.opens + usize::from(done.pending == Some('<'));
            let closes = done.closes + usize::from(done.pending == Some('>'));
            if done.reset {
                angles -= level.open;
                level.open = 0;
            }
            let closing = closes.min(level.open);
            level.open = level.open - closing + opens;
            angles = angles - closing + opens;
            if depth + angles > MAX_NESTING {
                return Some(first.span().byte_range().start);
            }
        }
        let open = level.open;
        match tree {
            Some(TokenTree::Group(group)) => {
                if depth + angles > MAX_NESTING {
                    return Some(group.span_open().byte_range().start);
                }
                stack.push(NestingLevel {
                    tokens: group.stream().into_iter(),
                    open: 0,
                });
            }
            Some(_) => {}
            None => {
                angles -= open;
                stack.pop();
            }
        }
    }
    None
}

/// Names brought into scope by `use` items anywhere in one file, and how
/// each macro invocation that may be Concord's `api!` resolves.
#[derive(Clone, Debug, Default)]
pub(crate) struct Imports {
    names: HashMap<String, Vec<String>>,
    globs: Vec<Vec<String>>,
    local_types: HashSet<String>,
    /// Offset of the first path segment (in the parsed text) of every macro
    /// invocation named `api`, or named by an import of Concord's `api` ->
    /// its resolution.
    api_macros: HashMap<usize, ApiMacro>,
    /// Whether the file defines a `macro_rules! api` anywhere.
    defines_api_macro_rules: bool,
    /// Whether a bare `api!` with nothing of that name imported in this file
    /// is Concord's (see [`ApiMacro::Bare`]): `Err` with the reason, or `None`
    /// until its crate is examined (not proved).
    bare_api: Option<Result<(), String>>,
}

/// How a macro invocation that may be Concord's `api!` resolves.
#[derive(Clone, Debug)]
pub(crate) enum ApiMacro {
    /// Proved to be Concord's `api!`.
    Concord,
    /// A bare `api!` with nothing of that name imported in its file. Edition
    /// 2024 has no prelude `api!`: it is Concord's only through
    /// `#[macro_use] extern crate concord` at its crate root, and a
    /// crate-local `macro_rules! api` (in textual scope, possibly through
    /// `#[macro_use] mod`) or another `#[macro_use]` crate may be the macro
    /// instead; the crate decides (see [`Imports::prove_bare_api`]).
    Bare { source_stable: bool },
    /// Not proved to be Concord's `api!` (the reason); an invocation named
    /// `api` is reported and not migrated.
    Unresolved { named_api: bool, reason: String },
}

impl Imports {
    pub(crate) fn collect(file: &syn::File) -> Self {
        Self::collect_with_namespace(file, None, None, None, None)
    }

    pub(crate) fn collect_with_namespace(
        file: &syn::File,
        namespace: Option<&NamespaceProof<'_>>,
        source_origin: Option<&NamespaceOriginProof<'_>>,
        own_library: Option<&str>,
        crate_origin: Option<&ApiCrateProof<'_>>,
    ) -> Self {
        let mut imports = Self::default();
        let mut collector = ImportCollector {
            imports: &mut imports,
        };
        collector.visit_file(file);
        let (api_macros, defines_api_macro_rules) =
            ApiScopes::classify(file, namespace, source_origin, own_library, crate_origin);
        imports.api_macros = api_macros;
        imports.defines_api_macro_rules = defines_api_macro_rules;
        imports
    }

    /// Whether the file defines a `macro_rules! api` anywhere.
    pub(crate) fn defines_api_macro_rules(&self) -> bool {
        self.defines_api_macro_rules
    }

    /// Whether the file invokes a bare `api!` with nothing of that name
    /// imported (see [`ApiMacro::Bare`]).
    pub(crate) fn has_bare_api(&self) -> bool {
        self.api_macros
            .values()
            .any(|resolution| matches!(resolution, ApiMacro::Bare { .. }))
    }

    /// Records whether the crate proves that a bare `api!` of this file is
    /// Concord's (`Err` with the reason when it does not).
    pub(crate) fn prove_bare_api(&mut self, proof: Result<(), String>) {
        self.bare_api = Some(proof);
    }

    /// The `api`-named invocations and explicit possible aliases not proved to be Concord's
    /// `api!`: `(offset in the parsed text, reason)`, in source order.
    pub(crate) fn unresolved_api_macros(&self) -> Vec<(usize, &str)> {
        let mut unresolved = self
            .api_macros
            .iter()
            .filter_map(|(offset, resolution)| match (resolution, &self.bare_api) {
                (
                    ApiMacro::Unresolved {
                        named_api: true,
                        reason,
                    },
                    _,
                ) => Some((*offset, reason.as_str())),
                (ApiMacro::Bare { .. }, Some(Err(reason))) => Some((*offset, reason.as_str())),
                (ApiMacro::Bare { .. }, None) => Some((*offset, UNPROVED_BARE_API)),
                (
                    ApiMacro::Bare {
                        source_stable: false,
                    },
                    Some(Ok(())),
                ) => Some((*offset, UNSTABLE_BARE_API)),
                _ => None,
            })
            .collect::<Vec<_>>();
        unresolved.sort_unstable();
        unresolved
    }

    /// Returns the full path imported under `name`, if any.
    pub(crate) fn resolve(&self, name: &str) -> Option<&[String]> {
        self.names.get(name).map(Vec::as_slice)
    }

    /// Whether any `use` item imports from a Concord crate.
    pub(crate) fn uses_concord(&self) -> bool {
        self.names
            .values()
            .chain(self.globs.iter())
            .any(|path| path.first().is_some_and(|first| is_concord_crate(first)))
    }

    pub(crate) fn has_concord_glob(&self) -> bool {
        self.globs
            .iter()
            .any(|path| path.first().is_some_and(|first| is_concord_crate(first)))
    }

    pub(crate) fn defines_local_type(&self, name: &str) -> bool {
        self.local_types.contains(name)
    }

    /// Whether a macro invocation of this file (identified by its path) is
    /// proved to be Concord's declaration macro; see [`ApiScopes`].
    pub(crate) fn is_api_macro(&self, path: &syn::Path) -> bool {
        path.segments.first().is_some_and(|segment| {
            match self
                .api_macros
                .get(&segment.ident.span().byte_range().start)
            {
                Some(ApiMacro::Concord) => true,
                Some(ApiMacro::Bare { source_stable }) => {
                    *source_stable && matches!(self.bare_api, Some(Ok(())))
                }
                _ => false,
            }
        })
    }
}

/// Why a bare `api!` is not Concord's before its crate is examined.
const UNPROVED_BARE_API: &str = "a bare `api!` with nothing of that name imported in this file names Concord's macro only through `#[macro_use] extern crate concord` at its crate root (edition 2024 has no prelude `api!`), which the tool could not examine";
const UNSTABLE_BARE_API: &str = "the source namespace of a bare `api!` may be supplied or replaced by an unknown expansion; macro-use provenance is not proved";

/// Whether a bare `api!` in a crate whose root has `root_items` is proved to
/// be Concord's: the root imports Concord's macros with an unconditional
/// `#[macro_use] extern crate concord` whose item cannot be replaced by an
/// attribute, no other crate's macros with
/// `#[macro_use]` (the standard library, `log` and `tracing` export no
/// `api!`), and no file of the crate defines a `macro_rules! api`
/// (`local_rules` names the first such file). `root` names the root file.
pub(crate) fn bare_api_proof(
    root_items: &[syn::Item],
    root: &str,
    local_rules: Option<&str>,
) -> Result<(), String> {
    if let Some(file) = local_rules {
        return Err(format!(
            "the crate defines a `macro_rules! api` (in `{file}`), which may be the macro invoked here (it is in textual scope after its definition, and in later modules through `#[macro_use] mod`)"
        ));
    }
    let mut proved = false;
    let mut conditional = false;
    for item in root_items {
        let syn::Item::ExternCrate(extern_crate) = item else {
            continue;
        };
        let Some(applies) = super::resolve::attribute_applies(&extern_crate.attrs, "macro_use")
        else {
            continue;
        };
        let name = extern_crate.ident.to_string();
        let effects = super::resolve::attribute_effects(&extern_crate.attrs);
        if effects.may_be_macro() {
            return Err(format!(
                "the macro-use extern item at the crate root `{root}` has an attribute that may replace it or supply another `api!`; its macro origin is not proved"
            ));
        }
        let gated = effects.gated;
        match name.as_str() {
            "concord" | "concord_macros" if applies && !gated => proved = true,
            "concord" | "concord_macros" => conditional = true,
            "std" | "core" | "alloc" | "log" | "tracing" => {}
            other => {
                return Err(format!(
                    "the crate root `{root}` also imports the macros of `{other}` with `#[macro_use]`, which may export another `api!`"
                ));
            }
        }
    }
    if proved {
        Ok(())
    } else if conditional {
        Err(format!(
            "`#[macro_use]` on `extern crate concord` at the crate root `{root}` depends on `#[cfg]`/`#[cfg_attr]`, so a bare `api!` with nothing of that name imported in this file may name another macro"
        ))
    } else {
        Err(format!(
            "a bare `api!` with nothing of that name imported in this file is not in the edition 2024 prelude: it names Concord's macro only through `#[macro_use] extern crate concord` at the crate root (`{root}`), which is absent"
        ))
    }
}

/// One module or block of a file, for resolving `api!` macro paths.
#[derive(Default)]
struct ApiScope {
    /// The enclosing scope (a block's parent, a module's parent module).
    parent: Option<usize>,
    /// Parsed-file byte offset of the module name/block opening; None at root.
    anchor: Option<usize>,
    /// Own-library extern aliases. Package identity still needs manifest proof.
    extern_self: HashSet<String>,
    /// Explicit extern aliases also affect leading :: paths.
    extern_aliases: HashMap<String, String>,
    /// Whether this is a module (else a block).
    module: bool,
    /// Inline child modules by name.
    children: HashMap<String, usize>,
    /// Imported name -> `(leading ::, source path)` of each `use` of it.
    imports: HashMap<String, Vec<(bool, Vec<String>)>>,
    /// Glob imports: `(leading ::, module path)`.
    globs: Vec<(bool, Vec<String>)>,
    /// Names of the `macro_rules!` macros defined here.
    macro_rules: HashSet<String>,
    /// Names of the modules (inline or file) declared here.
    modules: HashSet<String>,
}

/// The answer of one `api!` path lookup.
enum ApiAnswer {
    Concord,
    /// Resolves (or may resolve) to another macro: why.
    Other(String),
    /// Nothing of that name is imported here.
    NotFound,
}

/// File-local resolution of macro paths that may name Concord's `api!`.
///
/// A path is followed the way the compiler resolves it within one file: a
/// `macro_rules!` of that name in an enclosing scope, then explicit imports
/// and globs of the innermost block up to the module (following `self::`,
/// `super::` and inline-module paths inside the file), then — only for a bare
/// `api!` with nothing of that name imported — the `#[macro_use]`/prelude
/// spelling. A path to another crate, a `crate::` path, another crate's glob
/// or an ambiguity is not proved to be Concord's macro.
pub(crate) type NamespaceProof<'a> = dyn Fn(Option<usize>, bool, &[String], &str) -> bool + 'a;
#[derive(Clone, Copy)]
pub(crate) enum SourceNamespace {
    Crate,
    /// Leading invocation prefix, resolved against the actual Cargo edition.
    AbsoluteCrate,
    /// A use/glob prefix retains both its written scope and edition semantics.
    ImportCrate {
        leading: bool,
    },
    Macro,
    /// A strong macro import's source path: lexical stability, without the
    /// crate-wide absence proof required by bare and weak-glob lookup.
    MacroPath,
}
pub(crate) type NamespaceOriginProof<'a> =
    dyn Fn(Option<usize>, &str, SourceNamespace) -> bool + 'a;

#[derive(Clone, Copy, Eq, PartialEq)]
pub(crate) enum ApiCrateOrigin {
    AssumedExternal,
    CanonicalOwnRoot,
    Denied,
}

#[derive(Clone, Copy)]
pub(crate) enum ApiPathMode {
    Relative,
    Absolute,
    Import { leading: bool },
}

pub(crate) type ApiCrateProof<'a> = dyn Fn(Option<usize>, &str, ApiPathMode) -> ApiCrateOrigin + 'a;

struct ApiScopes<'a> {
    namespace: Option<&'a NamespaceProof<'a>>,
    source_origin: Option<&'a NamespaceOriginProof<'a>>,
    own_library: Option<&'a str>,
    crate_origin: Option<&'a ApiCrateProof<'a>>,
    scopes: Vec<ApiScope>,
    stack: Vec<usize>,
    /// `(first segment offset, leading ::, segments, scope)` of every macro
    /// invocation.
    invocations: Vec<(usize, bool, Vec<String>, usize)>,
}

const API_DEPTH: usize = 16;

fn api_ident(ident: &syn::Ident) -> String {
    let text = ident.to_string();
    text.strip_prefix("r#").unwrap_or(&text).to_owned()
}

impl ApiScopes<'_> {
    /// Every candidate invocation's resolution, and whether the file defines
    /// a `macro_rules! api`.
    fn classify(
        file: &syn::File,
        namespace: Option<&NamespaceProof<'_>>,
        source_origin: Option<&NamespaceOriginProof<'_>>,
        own_library: Option<&str>,
        crate_origin: Option<&ApiCrateProof<'_>>,
    ) -> (HashMap<usize, ApiMacro>, bool) {
        let mut walker = ApiScopes {
            namespace,
            source_origin,
            own_library,
            crate_origin,
            scopes: vec![ApiScope {
                module: true,
                ..ApiScope::default()
            }],
            stack: vec![0],
            invocations: Vec::new(),
        };
        walker.visit_file(file);
        let renamed = walker
            .scopes
            .iter()
            .flat_map(|scope| scope.imports.iter())
            .filter(|(_, paths)| {
                paths.iter().any(|(_, path)| {
                    path.last().is_some_and(|last| last == "api")
                        && path.first().is_some_and(|first| is_concord_crate(first))
                })
            })
            .map(|(name, _)| name.clone())
            .collect::<HashSet<_>>();
        let mut resolved = HashMap::new();
        for (offset, leading, segments, scope) in &walker.invocations {
            let named_api = segments.last().is_some_and(|last| last == "api");
            let candidate =
                named_api || matches!(segments.as_slice(), [single] if renamed.contains(single));
            if !candidate {
                continue;
            }
            let shadowed = match segments.as_slice() {
                [first, _, ..] if !*leading && is_concord_crate(first) => {
                    walker.shadowed_crate(*scope, first)
                }
                _ => None,
            };
            let answer = match shadowed {
                Some(reason) => ApiAnswer::Other(reason),
                None => walker.resolve(*scope, *leading, segments, 0),
            };
            let resolution = match answer {
                ApiAnswer::Concord => ApiMacro::Concord,
                // A bare name with nothing of that name imported: Concord's
                // only through `#[macro_use] extern crate concord`, which the
                // crate decides.
                ApiAnswer::NotFound if segments.len() == 1 && named_api && !*leading => {
                    let source_stable = !walker.source_origin.is_some_and(|proof| {
                        !proof(walker.scopes[*scope].anchor, "api", SourceNamespace::Macro)
                    });
                    ApiMacro::Bare { source_stable }
                }
                ApiAnswer::NotFound => ApiMacro::Unresolved {
                    named_api: true,
                    reason: "the path does not name an import of Concord's `api!` in this file"
                        .to_owned(),
                },
                ApiAnswer::Other(reason) => ApiMacro::Unresolved {
                    named_api: true,
                    reason,
                },
            };
            resolved.insert(*offset, resolution);
        }
        let defines_api = walker
            .scopes
            .iter()
            .any(|scope| scope.macro_rules.contains("api"));
        (resolved, defines_api)
    }

    fn top(&self) -> usize {
        *self.stack.last().expect("the file scope is never popped")
    }

    /// Why the first segment `name` of a macro path (a Concord crate name)
    /// may not name that crate where the path is written, if it may not: a
    /// module declared in the enclosing blocks or module, an import binding
    /// that name to something else, or a glob import of anything but a
    /// Concord crate (which may bring a module of that name into scope)
    /// takes precedence over the extern prelude.
    fn shadowed_crate(&self, scope: usize, name: &str) -> Option<String> {
        let mut current = scope;
        loop {
            let table = &self.scopes[current];
            if table.modules.contains(name) {
                return Some(format!(
                    "a module `{name}` declared where the invocation is takes precedence over the `{name}` crate in its path"
                ));
            }
            if let Some(paths) = table.imports.get(name) {
                if paths.iter().any(|(leading, path)| {
                    path.as_slice() != [name.to_owned()]
                        && !(*leading
                            && path.as_slice() == ["self".to_owned()]
                            && table.extern_self.contains(name)
                            && self.own_library == Some(name))
                }) {
                    return Some(format!(
                        "an import named `{name}` where the invocation is takes precedence over the `{name}` crate in its path"
                    ));
                }
            }
            if self
                .source_origin
                .is_some_and(|proof| !proof(table.anchor, name, SourceNamespace::Crate))
            {
                return Some(format!(
                    "the source namespace of `{name}` may be supplied or shadowed by an unknown item, statement, attribute or derive expansion; macro ownership is not proved"
                ));
            }
            if let Some((_, glob)) = table.globs.iter().find(|(leading, glob)| {
                !glob.first().is_some_and(|first| is_concord_crate(first))
                    && !self
                        .namespace
                        .is_some_and(|proof| proof(table.anchor, *leading, glob, name))
            }) {
                return Some(format!(
                    "the glob import `{}::*` where the invocation is may bring a module `{name}` into scope, which would take precedence over the `{name}` crate in its path",
                    glob.join("::")
                ));
            }
            if table.module {
                return None;
            }
            current = table.parent.expect("a block has an enclosing scope");
        }
    }

    fn module_of(&self, mut scope: usize) -> usize {
        while !self.scopes[scope].module {
            scope = self.scopes[scope]
                .parent
                .expect("a block has an enclosing scope");
        }
        scope
    }

    fn push(&mut self, module: bool, name: Option<String>, anchor: usize) -> usize {
        let parent = self.top();
        let id = self.scopes.len();
        self.scopes.push(ApiScope {
            parent: Some(parent),
            anchor: Some(anchor),
            module,
            ..ApiScope::default()
        });
        if let Some(name) = name {
            self.scopes[parent].children.insert(name, id);
        }
        self.stack.push(id);
        id
    }

    fn use_tree(&mut self, leading: bool, prefix: &mut Vec<String>, tree: &syn::UseTree) {
        let scope = self.top();
        match tree {
            syn::UseTree::Path(path) => {
                prefix.push(api_ident(&path.ident));
                self.use_tree(leading, prefix, &path.tree);
                prefix.pop();
            }
            syn::UseTree::Name(name) => {
                let ident = api_ident(&name.ident);
                let (key, path) = if ident == "self" {
                    let Some(last) = prefix.last().cloned() else {
                        return;
                    };
                    (last, prefix.clone())
                } else {
                    let mut path = prefix.clone();
                    path.push(ident.clone());
                    (ident, path)
                };
                self.scopes[scope]
                    .imports
                    .entry(key)
                    .or_default()
                    .push((leading, path));
            }
            syn::UseTree::Rename(rename) => {
                let mut path = prefix.clone();
                if rename.ident != "self" {
                    path.push(api_ident(&rename.ident));
                }
                self.scopes[scope]
                    .imports
                    .entry(api_ident(&rename.rename))
                    .or_default()
                    .push((leading, path));
            }
            syn::UseTree::Glob(_) => self.scopes[scope].globs.push((leading, prefix.clone())),
            syn::UseTree::Group(group) => {
                for item in &group.items {
                    self.use_tree(leading, prefix, item);
                }
            }
        }
    }

    /// An actual self alias is distinct from a package being eligible to
    /// provide one. A local submodule's export has no facade authority.
    fn own_self_alias(&self, mut scope: usize, name: &str) -> bool {
        loop {
            let table = &self.scopes[scope];
            if let Some(actual) = table.extern_aliases.get(name) {
                return actual == "self" && self.own_library == Some(name);
            }
            let Some(parent) = table.parent else {
                return false;
            };
            scope = parent;
        }
    }

    /// Only the external facade root/prelude and known package roots export
    /// api. Own-self source proof currently establishes the root alone.
    fn api_export_path(&self, scope: usize, path: &[String], mode: ApiPathMode) -> bool {
        let Some(first) = path.first() else {
            return false;
        };
        let origin = self.crate_origin.map_or_else(
            || {
                if self.own_self_alias(scope, first) {
                    ApiCrateOrigin::CanonicalOwnRoot
                } else {
                    ApiCrateOrigin::AssumedExternal
                }
            },
            |proof| proof(self.scopes[scope].anchor, first, mode),
        );
        match origin {
            ApiCrateOrigin::Denied => return false,
            ApiCrateOrigin::CanonicalOwnRoot => {
                return matches!(path, [krate, name] if is_concord_crate(krate) && name == "api");
            }
            ApiCrateOrigin::AssumedExternal => {}
        }
        match path {
            [krate, name] => is_concord_crate(krate) && name == "api",
            [krate, module, name] => {
                krate == "concord"
                    && module == "prelude"
                    && name == "api"
                    && !self.own_self_alias(scope, krate)
            }
            _ => false,
        }
    }

    /// Resolves a macro path used in `scope`.
    fn resolve(&self, scope: usize, leading: bool, segments: &[String], depth: usize) -> ApiAnswer {
        self.resolve_with_mode(
            scope,
            leading,
            segments,
            depth,
            if leading {
                ApiPathMode::Absolute
            } else {
                ApiPathMode::Relative
            },
        )
    }

    /// Retain a use path's original scope and rooted meaning through lookup.
    fn resolve_with_mode(
        &self,
        scope: usize,
        leading: bool,
        segments: &[String],
        depth: usize,
        mode: ApiPathMode,
    ) -> ApiAnswer {
        if depth > API_DEPTH {
            return ApiAnswer::Other("its imports nest too deeply to be followed".to_owned());
        }
        let Some((last, prefix)) = segments.split_last() else {
            return ApiAnswer::Other("the macro path is empty".to_owned());
        };
        let Some(first) = segments.first() else {
            return ApiAnswer::NotFound;
        };
        if is_concord_crate(first) {
            if leading {
                if self.source_origin.is_some_and(|proof| {
                    !proof(
                        self.scopes[scope].anchor,
                        first,
                        SourceNamespace::AbsoluteCrate,
                    )
                }) {
                    return ApiAnswer::Other("the absolute crate prefix is not proved to name Concord under this package's edition".to_owned());
                }
                let mut current = Some(scope);
                while let Some(id) = current {
                    let table = &self.scopes[id];
                    if let Some(actual) = table.extern_aliases.get(first) {
                        if actual != first
                            && !(actual == "self" && self.own_library == Some(first.as_str()))
                        {
                            return ApiAnswer::Other(format!(
                                "the extern alias `{first}` is not proved to name its Concord package"
                            ));
                        }
                    }
                    if self
                        .source_origin
                        .is_some_and(|proof| !proof(table.anchor, first, SourceNamespace::Crate))
                    {
                        return ApiAnswer::Other(format!(
                            "the source namespace of `{first}` may contain an unknown expansion or extern alias"
                        ));
                    }
                    current = table.parent;
                }
            }
            if !leading {
                if let Some(reason) = self.shadowed_crate(scope, first) {
                    return ApiAnswer::Other(reason);
                }
            }
            return if prefix.is_empty() {
                ApiAnswer::Other(format!("`{first}` names a crate, not a macro"))
            } else if self.api_export_path(scope, segments, mode) {
                ApiAnswer::Concord
            } else {
                ApiAnswer::Other(format!("`{}` is not Concord's `api!`", segments.join("::")))
            };
        }
        if leading {
            return ApiAnswer::Other(format!(
                "`::{}!` names a macro of another crate",
                segments.join("::")
            ));
        }
        if prefix.is_empty() {
            return self.resolve_name(scope, last, depth);
        }
        match self.navigate(self.module_of(scope), prefix) {
            Ok(module) => match self.lookup_in(module, last, depth) {
                ApiAnswer::NotFound => ApiAnswer::Other(format!(
                    "`{}` is not imported in the module `{}` names",
                    last,
                    prefix.join("::")
                )),
                other => other,
            },
            Err(reason) => ApiAnswer::Other(reason),
        }
    }

    /// A single-segment macro name: an enclosing `macro_rules!`, then the
    /// imports of the innermost block up to the module.
    fn resolve_name(&self, scope: usize, name: &str, depth: usize) -> ApiAnswer {
        let mut current = Some(scope);
        while let Some(id) = current {
            if self.scopes[id].macro_rules.contains(name) {
                return ApiAnswer::Other(format!(
                    "a `macro_rules! {name}` defined in this file may be the macro invoked here"
                ));
            }
            current = self.scopes[id].parent;
        }
        let mut current = scope;
        loop {
            match self.lookup_in(current, name, depth) {
                ApiAnswer::NotFound => {}
                other => return other,
            }
            if self.scopes[current].module {
                return ApiAnswer::NotFound;
            }
            current = self.scopes[current]
                .parent
                .expect("a block has an enclosing scope");
        }
    }

    /// `name` among the explicit imports, else the globs, of one scope.
    fn lookup_in(&self, scope: usize, name: &str, depth: usize) -> ApiAnswer {
        let table = &self.scopes[scope];
        if let Some(paths) = table.imports.get(name) {
            let mut answer = ApiAnswer::NotFound;
            for (leading, path) in paths {
                match self.resolve_import(scope, *leading, path, depth + 1) {
                    ApiAnswer::Concord => answer = ApiAnswer::Concord,
                    ApiAnswer::NotFound => {}
                    other => return other,
                }
            }
            return answer;
        }
        let mut answer = ApiAnswer::NotFound;
        for (leading, path) in &table.globs {
            match self.resolve_glob(scope, *leading, path, name, depth + 1) {
                ApiAnswer::Concord => answer = ApiAnswer::Concord,
                ApiAnswer::NotFound => {}
                other => return other,
            }
        }
        answer
    }

    /// The macro a `use` path imports (relative to the module of `scope`).
    fn resolve_import(
        &self,
        scope: usize,
        leading: bool,
        path: &[String],
        depth: usize,
    ) -> ApiAnswer {
        if let Some(first) = path.first().filter(|first| is_concord_crate(first)) {
            if self.source_origin.is_some_and(|proof| {
                !proof(
                    self.scopes[scope].anchor,
                    first,
                    SourceNamespace::ImportCrate { leading },
                )
            }) {
                return ApiAnswer::Other("the import crate prefix is not proved to name Concord under this package's edition".to_owned());
            }
        }
        if path.first().is_some_and(|first| is_concord_crate(first))
            && !self.api_export_path(scope, path, ApiPathMode::Import { leading })
        {
            return ApiAnswer::Other("this import path has no proved Concord api export at the scope where it is written".to_owned());
        }
        // Check at the scope where `use` is written, before module_of drops
        // block-local declarations. The recursive resolver repeats this guard.
        if !leading {
            if let Some(first) = path.first().filter(|first| is_concord_crate(first)) {
                if let Some(reason) = self.shadowed_crate(scope, first) {
                    return ApiAnswer::Other(reason);
                }
            }
        }
        match path {
            [single] if !leading && !is_concord_crate(single) => {
                ApiAnswer::Other(format!("the import `{single}` is not followed to a macro"))
            }
            _ => {
                self.resolve_with_mode(scope, leading, path, depth, ApiPathMode::Import { leading })
            }
        }
    }

    /// What a glob import provides under `name`.
    fn resolve_glob(
        &self,
        scope: usize,
        leading: bool,
        path: &[String],
        name: &str,
        depth: usize,
    ) -> ApiAnswer {
        let Some(first) = path.first() else {
            return ApiAnswer::Other("a glob import of an empty path".to_owned());
        };
        if is_concord_crate(first) {
            if self.source_origin.is_some_and(|proof| {
                !proof(
                    self.scopes[scope].anchor,
                    first,
                    SourceNamespace::ImportCrate { leading },
                )
            }) {
                return ApiAnswer::Other("the glob crate prefix is not proved to name Concord under this package's edition".to_owned());
            }
            if leading {
                let mut current = Some(scope);
                while let Some(id) = current {
                    let table = &self.scopes[id];
                    if let Some(actual) = table.extern_aliases.get(first) {
                        if actual != first
                            && !(actual == "self" && self.own_library == Some(first.as_str()))
                        {
                            return ApiAnswer::Other(format!(
                                "the extern alias `{first}` is not proved to name its Concord package"
                            ));
                        }
                    }
                    if self
                        .source_origin
                        .is_some_and(|proof| !proof(table.anchor, first, SourceNamespace::Crate))
                    {
                        return ApiAnswer::Other(format!(
                            "the source namespace of `{first}` may contain an unknown expansion or extern alias"
                        ));
                    }
                    current = table.parent;
                }
            }
            if !leading {
                if let Some(reason) = self.shadowed_crate(scope, first) {
                    return ApiAnswer::Other(reason);
                }
            }
            // Concord's facade root and prelude export `api`; its other
            // modules export no macro.
            if self.own_self_alias(scope, first) && path.len() != 1 {
                return ApiAnswer::Other("an own-self alias only has a proved canonical api export at its root; local submodule exports are not facade provenance".to_owned());
            }
            let mut exported = path.to_vec();
            exported.push(name.to_owned());
            let facade = first == "concord"
                && self.api_export_path(scope, &exported, ApiPathMode::Import { leading });
            return if facade && name == "api" {
                if self.source_origin.is_some_and(|proof| {
                    !proof(self.scopes[scope].anchor, name, SourceNamespace::Macro)
                }) {
                    ApiAnswer::Other("a weak Concord glob cannot prove this macro name when an unread or expanding source may supply a stronger crate-wide macro binding".to_owned())
                } else {
                    ApiAnswer::Concord
                }
            } else {
                ApiAnswer::NotFound
            };
        }
        if leading || !matches!(first.as_str(), "self" | "super") && !self.is_child(scope, first) {
            return ApiAnswer::Other(format!(
                "the glob import `{}::*` may provide another `{name}!`",
                path.join("::")
            ));
        }
        match self.navigate(self.module_of(scope), path) {
            Ok(module) => self.lookup_in(module, name, depth),
            Err(reason) => ApiAnswer::Other(reason),
        }
    }

    fn is_child(&self, scope: usize, name: &str) -> bool {
        self.scopes[self.module_of(scope)]
            .children
            .contains_key(name)
    }

    /// The inline module a module path names from `module`, inside the file.
    fn navigate(&self, module: usize, path: &[String]) -> Result<usize, String> {
        let mut current = module;
        for segment in path {
            current = match segment.as_str() {
                "self" => current,
                "super" => self.scopes[current]
                    .parent
                    .map(|parent| self.module_of(parent))
                    .ok_or_else(|| {
                        format!(
                            "`{}` leaves this file, which the tool does not follow for `api!`",
                            path.join("::")
                        )
                    })?,
                "crate" => {
                    return Err(format!(
                        "`{}` is a `crate::` path, which the tool does not follow for `api!`",
                        path.join("::")
                    ));
                }
                name => *self.scopes[current].children.get(name).ok_or_else(|| {
                    format!(
                        "`{}` does not name an inline module of this file (another crate's `api!`?)",
                        path.join("::")
                    )
                })?,
            };
        }
        Ok(current)
    }
}

impl<'ast> Visit<'ast> for ApiScopes<'_> {
    fn visit_item_extern_crate(&mut self, node: &'ast syn::ItemExternCrate) {
        let scope = self.top();
        let name = node
            .rename
            .as_ref()
            .map_or_else(|| api_ident(&node.ident), |(_, rename)| api_ident(rename));
        self.scopes[scope]
            .imports
            .entry(name.clone())
            .or_default()
            .push((true, vec![api_ident(&node.ident)]));
        self.scopes[scope]
            .extern_aliases
            .insert(name.clone(), api_ident(&node.ident));
        if node.ident == "self" {
            // Only package-level extern_crate_reasons may accept this alias.
            self.scopes[scope].extern_self.insert(name);
        }
    }

    fn visit_item_mod(&mut self, node: &'ast syn::ItemMod) {
        let scope = self.top();
        self.scopes[scope].modules.insert(api_ident(&node.ident));
        if node.content.is_none() {
            return;
        }
        self.push(
            true,
            Some(api_ident(&node.ident)),
            node.ident.span().byte_range().start,
        );
        syn::visit::visit_item_mod(self, node);
        self.stack.pop();
    }

    fn visit_block(&mut self, node: &'ast syn::Block) {
        self.push(false, None, node.brace_token.span.open().byte_range().start);
        syn::visit::visit_block(self, node);
        self.stack.pop();
    }

    fn visit_item_use(&mut self, node: &'ast syn::ItemUse) {
        self.use_tree(node.leading_colon.is_some(), &mut Vec::new(), &node.tree);
    }

    fn visit_item_macro(&mut self, node: &'ast syn::ItemMacro) {
        if let Some(ident) = &node.ident {
            let scope = self.top();
            self.scopes[scope].macro_rules.insert(api_ident(ident));
        }
        syn::visit::visit_item_macro(self, node);
    }

    fn visit_macro(&mut self, node: &'ast syn::Macro) {
        if node.path.is_ident("macro_rules") {
            if let Some(proc_macro2::TokenTree::Ident(name)) =
                node.tokens.clone().into_iter().next()
            {
                let scope = self.top();
                self.scopes[scope].macro_rules.insert(api_ident(&name));
            }
            return;
        }
        let Some(first) = node.path.segments.first() else {
            return;
        };
        let segments = node
            .path
            .segments
            .iter()
            .map(|segment| api_ident(&segment.ident))
            .collect();
        self.invocations.push((
            first.ident.span().byte_range().start,
            node.path.leading_colon.is_some(),
            segments,
            self.top(),
        ));
    }
}

pub(crate) fn is_concord_crate(name: &str) -> bool {
    matches!(
        name,
        "concord" | "concord_core" | "concord_extras" | "concord_macros"
    )
}

struct ImportCollector<'a> {
    imports: &'a mut Imports,
}

impl ImportCollector<'_> {
    fn tree(&mut self, prefix: &mut Vec<String>, tree: &syn::UseTree) {
        match tree {
            syn::UseTree::Path(path) => {
                prefix.push(path.ident.to_string());
                self.tree(prefix, &path.tree);
                prefix.pop();
            }
            syn::UseTree::Name(name) => {
                let ident = name.ident.to_string();
                let mut full = prefix.clone();
                if ident == "self" {
                    if let Some(last) = prefix.last().cloned() {
                        self.imports.names.insert(last, full);
                    }
                } else {
                    full.push(ident.clone());
                    self.imports.names.insert(ident, full);
                }
            }
            syn::UseTree::Rename(rename) => {
                let mut full = prefix.clone();
                full.push(rename.ident.to_string());
                self.imports.names.insert(rename.rename.to_string(), full);
            }
            syn::UseTree::Glob(_) => self.imports.globs.push(prefix.clone()),
            syn::UseTree::Group(group) => {
                for item in &group.items {
                    self.tree(prefix, item);
                }
            }
        }
    }
}

impl<'ast> Visit<'ast> for ImportCollector<'_> {
    fn visit_item_use(&mut self, node: &'ast syn::ItemUse) {
        let mut prefix = Vec::new();
        self.tree(&mut prefix, &node.tree);
    }

    fn visit_item_struct(&mut self, node: &'ast syn::ItemStruct) {
        self.imports.local_types.insert(node.ident.to_string());
        syn::visit::visit_item_struct(self, node);
    }

    fn visit_item_enum(&mut self, node: &'ast syn::ItemEnum) {
        self.imports.local_types.insert(node.ident.to_string());
        syn::visit::visit_item_enum(self, node);
    }

    fn visit_item_type(&mut self, node: &'ast syn::ItemType) {
        self.imports.local_types.insert(node.ident.to_string());
        syn::visit::visit_item_type(self, node);
    }
}

/// One `api!` invocation found in a file.
pub(crate) struct ApiInvocation {
    pub(crate) tokens: TokenStream,
    /// Byte offset of the macro path.
    pub(crate) offset: usize,
    /// The module whose item list contains the invocation, when it is a
    /// module-level item (not inside a function body or other block).
    pub(crate) scope: Option<ModuleScope>,
}

/// Names visible in the module that contains one `api!` item, and where a
/// new `use` item can be inserted into that module.
#[derive(Clone, Debug, Default)]
pub(crate) struct ModuleScope {
    /// Names brought in by explicit (non-glob) `use` items of the module.
    pub(crate) imported: HashSet<String>,
    /// Glob `use` paths of the module.
    pub(crate) globs: Vec<Vec<String>>,
    /// Names of items defined directly in the module.
    pub(crate) defined: HashSet<String>,
    /// Identifies the module within the file (offset of its first item).
    pub(crate) module_id: usize,
    /// Byte offset of the invocation item (including its outer attributes).
    pub(crate) item_start: usize,
}

impl ModuleScope {
    /// Whether a bare type name is already resolvable in the module through
    /// an explicit import, a local item or a glob that certainly exports it.
    pub(crate) fn resolves(&self, name: &str) -> bool {
        self.imported.contains(name)
            || self.defined.contains(name)
            || self.globs.iter().any(|glob| {
                match glob
                    .iter()
                    .map(String::as_str)
                    .collect::<Vec<_>>()
                    .as_slice()
                {
                    ["concord", "prelude"] => true,
                    ["concord"] => CONCORD_ROOT_TYPES.contains(&name),
                    _ => false,
                }
            })
    }
}

/// Declaration-relevant types exported at the `concord` facade root.
const CONCORD_ROOT_TYPES: &[&str] = &[
    "ApiError",
    "ApiResponse",
    "Binary",
    "ByteStream",
    "Bytes",
    "ClientBuildError",
    "Discard",
    "Empty",
    "ItemStream",
    "Json",
    "RequestStream",
    "SecretString",
    "Text",
];

pub(crate) fn find_api_invocations(parsed: &ParsedFile, imports: &Imports) -> Vec<ApiInvocation> {
    let mut scopes = std::collections::HashMap::new();
    collect_module_scopes(&parsed.file.items, parsed.base, imports, &mut scopes);
    let mut finder = ApiFinder {
        imports,
        base: parsed.base,
        found: Vec::new(),
    };
    finder.visit_file(&parsed.file);
    for invocation in &mut finder.found {
        invocation.scope = scopes.remove(&invocation.offset);
    }
    finder.found
}

fn token_range(base: usize, node: &impl quote::ToTokens) -> std::ops::Range<usize> {
    let mut spans = node.to_token_stream().into_iter().map(|token| token.span());
    let Some(first) = spans.next() else {
        return base..base;
    };
    let last = spans.last().unwrap_or(first);
    (first.byte_range().start + base)..(last.byte_range().end + base)
}

fn use_names(prefix: &mut Vec<String>, tree: &syn::UseTree, scope: &mut ModuleScope) {
    match tree {
        syn::UseTree::Path(path) => {
            prefix.push(path.ident.to_string());
            use_names(prefix, &path.tree, scope);
            prefix.pop();
        }
        syn::UseTree::Name(name) => {
            if name.ident == "self" {
                if let Some(last) = prefix.last() {
                    scope.imported.insert(last.clone());
                }
            } else {
                scope.imported.insert(name.ident.to_string());
            }
        }
        syn::UseTree::Rename(rename) => {
            scope.imported.insert(rename.rename.to_string());
        }
        syn::UseTree::Glob(_) => scope.globs.push(prefix.clone()),
        syn::UseTree::Group(group) => {
            for item in &group.items {
                use_names(prefix, item, scope);
            }
        }
    }
}

fn collect_module_scopes(
    items: &[syn::Item],
    base: usize,
    imports: &Imports,
    output: &mut std::collections::HashMap<usize, ModuleScope>,
) {
    let mut module = ModuleScope {
        module_id: items
            .first()
            .map_or(base, |item| token_range(base, item).start),
        ..ModuleScope::default()
    };
    for item in items {
        let name = match item {
            syn::Item::Struct(item) => Some(&item.ident),
            syn::Item::Enum(item) => Some(&item.ident),
            syn::Item::Type(item) => Some(&item.ident),
            syn::Item::Trait(item) => Some(&item.ident),
            syn::Item::Union(item) => Some(&item.ident),
            syn::Item::Mod(item) => Some(&item.ident),
            _ => None,
        };
        if let Some(name) = name {
            module.defined.insert(name.to_string());
        }
        if let syn::Item::Use(item) = item {
            use_names(&mut Vec::new(), &item.tree, &mut module);
        }
    }
    for item in items {
        match item {
            syn::Item::Macro(item) if imports.is_api_macro(&item.mac.path) => {
                let offset = item
                    .mac
                    .path
                    .segments
                    .first()
                    .map_or(0, |segment| segment.ident.span().byte_range().start)
                    + base;
                let mut scope = module.clone();
                scope.item_start = token_range(base, item).start;
                output.insert(offset, scope);
            }
            syn::Item::Mod(item) => {
                if let Some((_, items)) = &item.content {
                    collect_module_scopes(items, base, imports, output);
                }
            }
            _ => {}
        }
    }
}

struct ApiFinder<'a> {
    imports: &'a Imports,
    base: usize,
    found: Vec<ApiInvocation>,
}

impl<'ast> Visit<'ast> for ApiFinder<'_> {
    fn visit_macro(&mut self, node: &'ast syn::Macro) {
        if self.imports.is_api_macro(&node.path) {
            let offset = node
                .path
                .segments
                .first()
                .map_or(0, |segment| segment.ident.span().byte_range().start)
                + self.base;
            self.found.push(ApiInvocation {
                tokens: node.tokens.clone(),
                offset,
                scope: None,
            });
        }
        syn::visit::visit_macro(self, node);
    }
}
