//! Generated-surface snapshot (test only): the public inherent members the
//! generator actually emits for representative declarations.
//!
//! The migration tool (`tools/xtask`, `migrate-v3`) keeps its own table of the
//! names v3 generates that v2.1 did not, to report application trait methods
//! those names would shadow (`C3-MIGRATE-GENERATED-NAME-SHADOW`). This test
//! expands the migrated parity fixture
//! (`tools/xtask/fixtures/generated_surface/surface_v3.rs.txt`) through the
//! real generator, extracts every `pub fn` of every inherent `impl` block per
//! generated type, and compares the result with the committed snapshot
//! `tools/xtask/fixtures/generated_surface/v3_members.txt`. The xtask test
//! `migration::tests::surface_parity` then checks the migrator's table against
//! that snapshot and the frozen v2.1 member list. Emitting a new public member
//! therefore fails this test until the snapshot is regenerated
//! (`CONCORD_BLESS_SURFACE=1`), and then fails the xtask parity test until the
//! migrator's table accounts for it.

use std::fmt::Write as _;
use std::path::PathBuf;

use proc_macro2::TokenStream as TokenStream2;
use quote::{ToTokens, quote};

/// One public inherent member of one generated type.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) struct Member {
    /// The type's path inside the expansion (`surface::region::ItemRequest`).
    pub(crate) owner: String,
    pub(crate) name: String,
    /// `self`, `&self`, `&mut self` or `-` (an associated function).
    pub(crate) receiver: String,
    /// Arguments besides the receiver.
    pub(crate) arity: usize,
    /// `#[doc(hidden)]` on the function.
    pub(crate) hidden: bool,
}

fn is_hidden(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|attribute| {
        attribute.path().is_ident("doc")
            && attribute
                .meta
                .to_token_stream()
                .to_string()
                .replace(' ', "")
                .contains("doc(hidden)")
    })
}

fn type_name(ty: &syn::Type) -> Option<String> {
    match ty {
        syn::Type::Path(path) if path.qself.is_none() => path
            .path
            .segments
            .iter()
            .map(|segment| segment.ident.to_string())
            .reduce(|left, right| format!("{left}::{right}")),
        _ => None,
    }
}

/// `module::path` with the `self`/`super` segments of `path` resolved.
fn normalize(module: &[String], path: &str) -> String {
    let mut segments = module.to_vec();
    for segment in path.split("::") {
        match segment {
            "self" => {}
            "super" => {
                segments.pop();
            }
            other => segments.push(other.to_owned()),
        }
    }
    segments.join("::")
}

fn collect(
    items: &[syn::Item],
    module: &[String],
    members: &mut Vec<Member>,
    exports: &mut Vec<String>,
) {
    for item in items {
        match item {
            syn::Item::Mod(module_item) => {
                if let Some((_, nested)) = &module_item.content {
                    let mut path = module.to_vec();
                    path.push(module_item.ident.to_string());
                    collect(nested, &path, members, exports);
                }
            }
            syn::Item::Impl(implementation) if implementation.trait_.is_none() => {
                let Some(name) = type_name(&implementation.self_ty) else {
                    continue;
                };
                let owner = normalize(module, &name);
                for impl_item in &implementation.items {
                    let syn::ImplItem::Fn(function) = impl_item else {
                        continue;
                    };
                    if !matches!(function.vis, syn::Visibility::Public(_)) {
                        continue;
                    }
                    let mut receiver = "-".to_owned();
                    let mut arity = 0;
                    for input in &function.sig.inputs {
                        match input {
                            syn::FnArg::Receiver(value) => {
                                receiver = match &value.kind {
                                    syn::ReceiverKind::Reference(_, _, Some(_)) => {
                                        "&mut self".to_owned()
                                    }
                                    syn::ReceiverKind::Reference(_, _, None) => "&self".to_owned(),
                                    syn::ReceiverKind::Value => "self".to_owned(),
                                    syn::ReceiverKind::Typed(_, ty) => {
                                        format!("self:{}", ty.to_token_stream())
                                    }
                                    _ => "self:other".to_owned(),
                                };
                            }
                            syn::FnArg::Typed(_) => arity += 1,
                        }
                    }
                    members.push(Member {
                        owner: owner.clone(),
                        name: function.sig.ident.to_string(),
                        receiver,
                        arity,
                        hidden: is_hidden(&function.attrs) || is_hidden(&implementation.attrs),
                    });
                }
            }
            syn::Item::Const(constant) => {
                // Generated code nests items in `const _: () = { .. };`
                // blocks; their inherent impls still extend the named types.
                if let syn::Expr::Block(block) = &*constant.expr {
                    let nested = block
                        .block
                        .stmts
                        .iter()
                        .filter_map(|statement| match statement {
                            syn::Stmt::Item(item) => Some(item.clone()),
                            _ => None,
                        })
                        .collect::<Vec<_>>();
                    collect(&nested, module, members, exports);
                }
            }
            syn::Item::Use(import) if matches!(import.vis, syn::Visibility::Public(_)) => {
                let mut paths = Vec::new();
                use_paths(&import.tree, String::new(), &mut paths);
                for (target, alias) in paths {
                    exports.push(format!(
                        "export {} = {}",
                        normalize(module, &alias),
                        normalize(module, &target)
                    ));
                }
            }
            syn::Item::Type(alias) if matches!(alias.vis, syn::Visibility::Public(_)) => {
                if let Some(target) = type_name(&alias.ty) {
                    exports.push(format!(
                        "export {} = {}",
                        normalize(module, &alias.ident.to_string()),
                        normalize(module, &target)
                    ));
                }
            }
            _ => {}
        }
    }
}

/// `(target path, visible name)` of every leaf of a `use` tree (`*` for a
/// glob).
fn use_paths(tree: &syn::UseTree, prefix: String, paths: &mut Vec<(String, String)>) {
    let join = |name: &str| {
        if prefix.is_empty() {
            name.to_owned()
        } else {
            format!("{prefix}::{name}")
        }
    };
    match tree {
        syn::UseTree::Path(path) => use_paths(&path.tree, join(&path.ident.to_string()), paths),
        syn::UseTree::Name(name) => {
            paths.push((join(&name.ident.to_string()), name.ident.to_string()))
        }
        syn::UseTree::Rename(rename) => {
            paths.push((join(&rename.ident.to_string()), rename.rename.to_string()));
        }
        syn::UseTree::Glob(_) => paths.push((join("*"), "*".to_owned())),
        syn::UseTree::Group(group) => {
            for item in &group.items {
                use_paths(item, prefix.clone(), paths);
            }
        }
    }
}

/// Every public inherent member of the expansion `tokens`.
/// Every public inherent member of the expansion `tokens`, and every public
/// re-export or type alias (`export <visible path> = <target path>`).
pub(crate) fn members(tokens: TokenStream2) -> (Vec<Member>, Vec<String>) {
    let file: syn::File = syn::parse2(tokens).expect("generated code parses as a file");
    let mut members = Vec::new();
    let mut exports = Vec::new();
    collect(&file.items, &[], &mut members, &mut exports);
    members.sort();
    members.dedup();
    // Re-exports inside hidden (`__`) modules are not public surface.
    exports.retain(|line| !line.contains("__"));
    exports.sort();
    exports.dedup();
    (members, exports)
}

/// Expands every `api!` invocation of the Rust source `source`.
pub(crate) fn expand_source(source: &str) -> Vec<TokenStream2> {
    let file = syn::parse_file(source).expect("fixture parses");
    file.items
        .iter()
        .filter_map(|item| match item {
            syn::Item::Macro(invocation)
                if invocation
                    .mac
                    .path
                    .segments
                    .last()
                    .is_some_and(|segment| segment.ident == "api") =>
            {
                Some(invocation.mac.tokens.clone())
            }
            _ => None,
        })
        .map(|declaration| {
            crate::expand_with_facade(declaration, &quote!(::concord))
                .unwrap_or_else(|error| panic!("fixture declaration expands: {error}"))
        })
        .collect()
}

/// The snapshot text: one `owner member receiver arity [hidden]` line each.
pub(crate) fn render(members: &[Member]) -> String {
    let mut text = String::new();
    for member in members {
        let _ = writeln!(
            text,
            "{} {} {} {}{}",
            member.owner,
            member.name,
            member.receiver.replace(' ', "_"),
            member.arity,
            if member.hidden { " hidden" } else { "" }
        );
    }
    text
}

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tools/xtask/fixtures/generated_surface")
}

#[test]
fn generated_public_members_match_the_migrator_snapshot() {
    let source = std::fs::read_to_string(fixtures().join("surface_v3.rs.txt"))
        .expect("migrated parity fixture");
    let mut all = Vec::new();
    let mut exports = Vec::new();
    for expansion in expand_source(&source) {
        let (found, exported) = members(expansion);
        all.extend(found);
        exports.extend(exported);
    }
    all.sort();
    all.dedup();
    exports.sort();
    exports.dedup();
    let rendered = format!(
        "# Public inherent members the v3 generator emits for\n# tools/xtask/fixtures/generated_surface/surface_v3.rs.txt\n# (owner member receiver arity [hidden]; then public re-exports and type\n# aliases). Regenerate with\n# CONCORD_BLESS_SURFACE=1 cargo test -p concord-macros generated_public_members\n{}{}",
        render(&all),
        exports
            .iter()
            .map(|line| format!("{line}\n"))
            .collect::<String>()
    );
    let snapshot = fixtures().join("v3_members.txt");
    if std::env::var_os("CONCORD_BLESS_SURFACE").is_some() {
        std::fs::write(&snapshot, &rendered).expect("write snapshot");
        return;
    }
    let committed = std::fs::read_to_string(&snapshot).unwrap_or_default();
    assert!(
        committed == rendered,
        "the generator's public member set changed; regenerate {} with CONCORD_BLESS_SURFACE=1 and then make the migrator's v3-only name table (`tools/xtask/src/migration/callers.rs`, `v3_core_names`/`v3_value_names`) account for every new member (`migration::tests::surface_parity`).\n--- committed\n{committed}\n--- generated\n{rendered}",
        snapshot.display()
    );
}

/// Every templated `pub fn #name` emission site of `src/emit` as
/// `(file, template variable)`, and the parity-fixture declaration element
/// that exercises it. A site the fixture does not reach would add members the
/// snapshot never sees, so a new site fails
/// [`every_emitted_member_site_is_exercised_by_the_fixture`] until the fixture
/// exercises it and this list names it.
const TEMPLATED_SITES: &[(&str, &str, &str)] = &[
    ("client.rs", "clear", "credential facade: any credential"),
    ("client.rs", "replace", "credential facade: any credential"),
    (
        "client.rs",
        "setter",
        "client values tenant/locale/revision/trace",
    ),
    (
        "client.rs",
        "if_some",
        "client values tenant/locale/revision/trace",
    ),
    ("client.rs", "method", "secret bootstrap_key"),
    ("client.rs", "method", "custom credential delegated"),
    ("endpoint.rs", "method", "bound endpoint navigation"),
    ("endpoint.rs", "method", "unbound endpoint navigation"),
    ("endpoint.rs", "set", "optional body patch"),
    ("endpoint.rs", "replace", "optional body patch"),
    ("endpoint.rs", "set_if_some", "optional body patch"),
    ("endpoint.rs", "omit", "optional body patch"),
    (
        "endpoint.rs",
        "method",
        "endpoint-sourced credential session (login)",
    ),
    ("owner.rs", "set", "scope/endpoint values"),
    ("owner.rs", "replace", "optional scope/endpoint values"),
    (
        "owner.rs",
        "set_if_some",
        "optional/initialized scope/endpoint values",
    ),
    ("owner.rs", "omit", "optional scope/endpoint values"),
    ("owner.rs", "reset", "initialized scope/endpoint values"),
    ("owner.rs", "method", "bound scope navigation"),
    ("owner.rs", "method", "unbound scope navigation"),
];

/// `(file, name, templated)` of every `pub fn`/`pub const fn`/`pub async fn`
/// in the generator's emission sources.
fn emission_sites() -> Vec<(String, String, bool)> {
    let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/emit");
    let mut files = std::fs::read_dir(&directory)
        .expect("emit sources")
        .map(|entry| entry.expect("emit source").path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "rs"))
        .collect::<Vec<_>>();
    files.sort();
    let mut sites = Vec::new();
    for path in files {
        let file = path
            .file_name()
            .expect("file name")
            .to_string_lossy()
            .into_owned();
        let text = std::fs::read_to_string(&path).expect("emit source");
        for line in text.lines() {
            let line = line.trim_start();
            if line.starts_with("//") {
                continue;
            }
            for marker in ["pub fn ", "pub const fn ", "pub async fn "] {
                let Some(rest) = line.find(marker).map(|at| &line[at + marker.len()..]) else {
                    continue;
                };
                let templated = rest.starts_with('#');
                let name = rest
                    .trim_start_matches('#')
                    .chars()
                    .take_while(|character| character.is_alphanumeric() || *character == '_')
                    .collect::<String>();
                sites.push((file.clone(), name, templated));
            }
        }
    }
    sites
}

#[test]
fn every_emitted_member_site_is_exercised_by_the_fixture() {
    let snapshot =
        std::fs::read_to_string(fixtures().join("v3_members.txt")).expect("member snapshot");
    let names = snapshot
        .lines()
        .filter(|line| !line.starts_with('#') && !line.starts_with("export "))
        .filter_map(|line| line.split(' ').nth(1))
        .collect::<std::collections::BTreeSet<_>>();
    let sites = emission_sites();
    let missing = sites
        .iter()
        .filter(|(_, name, templated)| !templated && !names.contains(name.as_str()))
        .collect::<Vec<_>>();
    assert!(
        missing.is_empty(),
        "emitted members the parity fixture never produces (extend tools/xtask/fixtures/generated_surface/surface_v2_1.rs.txt): {missing:?}"
    );
    let mut templated = sites
        .iter()
        .filter(|(_, _, templated)| *templated)
        .map(|(file, name, _)| (file.as_str(), name.as_str()))
        .collect::<Vec<_>>();
    templated.sort_unstable();
    let mut expected = TEMPLATED_SITES
        .iter()
        .map(|(file, name, _)| (*file, *name))
        .collect::<Vec<_>>();
    expected.sort_unstable();
    assert_eq!(
        templated, expected,
        "the templated member emission sites changed: make the parity fixture exercise each new site and list it in TEMPLATED_SITES"
    );
}
