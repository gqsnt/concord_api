//! Generated rustdoc (§19.2): author text first, then the effective facts a
//! caller needs — inputs, result, statuses, route, timeout, authentication
//! and quota coverage — rendered from the resolved model.

use std::fmt::Write as _;
use std::time::Duration;

use proc_macro2::TokenStream;
use quote::{ToTokens, quote};

use crate::semantic::{
    AuthChallenge, BoundDim, EffectDecl, LocalHost, LocalPath, Meta, Model, OwnerKind, Placement,
    PlacementKind, Positional, SegPart, Status, Value,
};

fn type_text(ty: &impl ToTokens) -> String {
    ty.to_token_stream()
        .to_string()
        .replace(" < ", "<")
        .replace("< ", "<")
        .replace(" <", "<")
        .replace(" >", ">")
        .replace(" ,", ",")
        .replace(" :: ", "::")
        .replace(":: ", "::")
}

pub(crate) fn human_duration(duration: Duration) -> String {
    let seconds = duration.as_secs();
    if duration.subsec_nanos() != 0 {
        return format!("{} ms", duration.as_millis());
    }
    if seconds % 3600 == 0 {
        format!("{} h", seconds / 3600)
    } else if seconds % 60 == 0 {
        format!("{} min", seconds / 60)
    } else {
        format!("{seconds} s")
    }
}

fn author(meta: &Meta) -> String {
    let mut out = String::new();
    for line in &meta.docs {
        out.push_str(line.strip_prefix(' ').unwrap_or(line));
        out.push('\n');
    }
    if !out.is_empty() {
        out.push('\n');
    }
    out
}

pub(crate) fn deprecated_attr(meta: &Meta) -> TokenStream {
    match &meta.deprecated {
        None => TokenStream::new(),
        Some(deprecation) => match (&deprecation.note, &deprecation.since) {
            (None, None) => quote!(#[deprecated]),
            (Some(note), None) => quote!(#[deprecated(note = #note)]),
            (None, Some(since)) => quote!(#[deprecated(since = #since)]),
            (Some(note), Some(since)) => quote!(#[deprecated(since = #since, note = #note)]),
        },
    }
}

fn timeout_text(timeout: Option<Duration>) -> String {
    match timeout {
        Some(duration) => human_duration(duration),
        None => "30 s (root default)".to_owned(),
    }
}

fn placement_text(model: &Model, placement: &Placement) -> String {
    let credential = &model.client.credentials[placement.credential].logical;
    let target = match &placement.kind {
        PlacementKind::Bearer => format!("bearer `{credential}`"),
        PlacementKind::Basic => format!("basic `{credential}`"),
        PlacementKind::Authorization(None) => format!("authorization = `{credential}`"),
        PlacementKind::Authorization(Some(scheme)) => {
            format!("authorization \"{scheme}\" = `{credential}`")
        }
        PlacementKind::Header(name) => format!("header \"{name}\" = `{credential}`"),
        PlacementKind::Query(name) => format!("query \"{name}\" = `{credential}`"),
    };
    let challenge = match placement.challenge {
        AuthChallenge::Never => "",
        AuthChallenge::Unauthorized => " (challenge on 401)",
        AuthChallenge::UnauthorizedOrForbidden => " (challenge on 401/403)",
    };
    format!("{target}{challenge}, from `{}`", placement.origin)
}

/// Effective route template of an endpoint, e.g.
/// `https://{region}.api.catalog.example.test/root/{api_version}/regions/{region}/items`.
pub(crate) fn route_template(model: &Model, endpoint: usize) -> String {
    let entry = &model.endpoints[endpoint];
    let chain = model.chain(entry.owner);
    let from = crate::semantic::Site::Endpoint(endpoint);
    let mut labels: Vec<String> = Vec::new();
    for owner in &chain {
        let local: Vec<String> = match &model.owners[*owner].host {
            None => continue,
            Some(LocalHost::Off) => {
                labels.clear();
                continue;
            }
            Some(LocalHost::Prepend(list) | LocalHost::Replace(list)) => list
                .iter()
                .map(|label| atom_text(model, &label.atom, from))
                .collect(),
        };
        if matches!(model.owners[*owner].host, Some(LocalHost::Replace(_))) {
            labels = local;
        } else {
            let mut next = local;
            next.extend(labels);
            labels = next;
        }
    }
    let mut segments: Vec<String> = Vec::new();
    let paths = chain
        .iter()
        .map(|owner| &model.owners[*owner].path)
        .chain(std::iter::once(&entry.path));
    for path in paths {
        match path {
            None => {}
            Some(LocalPath::Off) => segments.clear(),
            Some(LocalPath::Append(template)) | Some(LocalPath::Replace(template)) => {
                if matches!(path, Some(LocalPath::Replace(_))) {
                    segments.clear();
                }
                for segment in &template.segments {
                    let text = segment
                        .parts
                        .iter()
                        .map(|part| match part {
                            SegPart::Literal(text) => text.clone(),
                            SegPart::Value { value, .. } => {
                                format!("{{{}}}", model.display_ref(*value, from))
                            }
                        })
                        .collect::<String>();
                    segments.push(text);
                }
            }
        }
    }
    let base = model.client.base.value();
    let Ok(url) = url::Url::parse(&base) else {
        return base;
    };
    let host = url.host_str().unwrap_or_default();
    let mut prefix = url.path().to_owned();
    if !prefix.ends_with('/') {
        prefix.push('/');
    }
    let port = url
        .port()
        .map(|port| format!(":{port}"))
        .unwrap_or_default();
    let host = if labels.is_empty() {
        host.to_owned()
    } else {
        format!("{}.{host}", labels.join("."))
    };
    format!(
        "{}://{host}{port}{prefix}{}",
        url.scheme(),
        segments.join("/")
    )
}

fn atom_text(model: &Model, atom: &crate::semantic::RAtom, from: crate::semantic::Site) -> String {
    match atom {
        crate::semantic::RAtom::Literal { text, .. } => text.clone(),
        crate::semantic::RAtom::Value { value, .. } => {
            format!("{{{}}}", model.display_ref(*value, from))
        }
        crate::semantic::RAtom::Concat(parts) => parts
            .iter()
            .map(|part| atom_text(model, part, from))
            .collect(),
    }
}

fn value_line(value: &Value) -> String {
    let presence = if value.optional {
        "optional"
    } else if value.is_initialized() {
        "initialized"
    } else {
        "required"
    };
    let sensitive = if value.sensitive {
        ", sensitive (withheld from automatic observation)"
    } else {
        ""
    };
    let deprecated = if value.is_deprecated() && !value.live {
        ", deprecated no-op value"
    } else {
        ""
    };
    let mut line = format!(
        "- `{}`: `{}` ({presence}{sensitive}{deprecated})",
        value.logical,
        type_text(&value.ty)
    );
    if let Some(first) = value.meta.docs.first() {
        let _ = write!(line, " — {}", first.trim());
    }
    line
}

pub(crate) fn owner_module(model: &Model, owner: usize) -> String {
    let entry = &model.owners[owner];
    match entry.kind {
        OwnerKind::Client => format!(
            "Generated support module for [`{}`](super::{}): its builder, unbound intent tree, credential facade, named requests of root endpoints and one module per scope.",
            entry.display, model.client.name
        ),
        OwnerKind::Scope => format!(
            "Generated support module for the `{}` scope: its handle, unbound handle and named requests.",
            entry.display
        ),
    }
}

pub(crate) fn scope_handle(model: &Model, owner: usize) -> String {
    let entry = &model.owners[owner];
    let mut out = author(&entry.meta);
    let _ = writeln!(
        out,
        "Selected `{}` scope. Cloning shares the client services and this immutable snapshot; value controls consume the handle and return a new snapshot, so requests created earlier keep the old values.",
        entry.display
    );
    if !entry.values.is_empty() {
        out.push_str("\n# Scope values\n\n");
        for value in &entry.values {
            out.push_str(&value_line(value));
            out.push('\n');
        }
    }
    let _ = writeln!(
        out,
        "\nEffective timeout: {}.",
        timeout_text(entry.effective_timeout)
    );
    out
}

pub(crate) fn scope_navigation(model: &Model, owner: usize) -> String {
    let entry = &model.owners[owner];
    let mut out = author(&entry.meta);
    let _ = write!(
        out,
        "Selects the `{}` scope. Its defaults are evaluated once now. No I/O is performed.",
        entry.display
    );
    out
}

pub(crate) fn unbound_owner(model: &Model, owner: usize) -> String {
    let entry = &model.owners[owner];
    format!(
        "Unbound `{}` navigation: captures scope and endpoint snapshots without a client. Bind an endpoint intent with `{}::request` or plan it with `{}::plan`; the selected client supplies its root values and services, and retained defaults are not re-evaluated.",
        entry.display, model.client.name, model.client.name
    )
}

pub(crate) fn control(value: &Value, kind: &str, owner: &str) -> String {
    let name = &value.logical;
    let ty = type_text(&value.ty);
    let mut out = String::new();
    if let Some(first) = value.meta.docs.first() {
        let _ = writeln!(out, "{}\n", first.trim());
    }
    let body = match kind {
        "set" => format!(
            "Selects a present `{name}` payload (`impl IntoValue<{ty}>`: identity; borrowed strings only when the target is `String`)."
        ),
        "replace" => format!(
            "Replaces the outer slot of `{name}` exactly: `Some(v)` is present, `None` is absent. For a nested `Option` payload, `Some(None)` is a present inner `None`."
        ),
        "set_if_some" => format!(
            "Selects `{name}` when `Some`; `None` means no new opinion and leaves the current selection unchanged."
        ),
        "omit" => format!("Selects an absent `{name}`: its keyed emissions are omitted."),
        "reset" => format!(
            "Restores the retained initial selection of `{name}` (the initializer is not re-evaluated)."
        ),
        _ => String::new(),
    };
    let _ = write!(out, "{body} Owner: `{owner}`.");
    if value.sensitive {
        out.push_str(" The value is sensitive: it is sent, but withheld from automatic plans, errors and observers.");
    }
    out
}

fn status_text(status: &Status) -> String {
    match status {
        Status::Success => "200..=299".to_owned(),
        Status::Ranges(ranges) => ranges
            .iter()
            .map(|(start, end)| {
                if start == end {
                    start.to_string()
                } else {
                    format!("{start}..={end}")
                }
            })
            .collect::<Vec<_>>()
            .join(", "),
    }
}

fn effect_text(effect: EffectDecl) -> &'static str {
    match effect {
        EffectDecl::Safe => "safe",
        EffectDecl::Idempotent => "idempotent",
        EffectDecl::NonIdempotent => "non-idempotent",
    }
}

fn endpoint_card(model: &Model, index: usize) -> String {
    let entry = &model.endpoints[index];
    let mut out = String::new();
    let _ = writeln!(out, "# Contract\n");
    let _ = writeln!(
        out,
        "- Operation: `{} {}` (endpoint ID `{}`, {})",
        entry.method.as_str(),
        route_template(model, index),
        entry.id,
        if entry.id_explicit {
            "explicit"
        } else {
            "navigation-derived"
        }
    );
    let result = type_text(&entry.output);
    let _ = writeln!(
        out,
        "- Result: `{result}` via `{}`",
        type_text(&entry.response)
    );
    let _ = writeln!(out, "- Accepted statuses: {}", status_text(&entry.status));
    if let Some(absence) = &entry.absence {
        let codes = absence
            .iter()
            .map(u16::to_string)
            .collect::<Vec<_>>()
            .join(", ");
        let _ = writeln!(
            out,
            "- Expected absence: `None` on {codes} (head-only; that response body is dropped unread)"
        );
    }
    let _ = writeln!(
        out,
        "- Effect: {}{}",
        effect_text(entry.effect),
        if entry.effect_explicit {
            " (declared)"
        } else {
            " (method default)"
        }
    );
    let _ = writeln!(out, "- Timeout: {}", timeout_text(entry.effective_timeout));
    if entry.effective_auth.is_empty() {
        let _ = writeln!(out, "- Authentication: none");
    } else {
        for placement in &entry.effective_auth {
            let _ = writeln!(
                out,
                "- Authentication: {}",
                placement_text(model, placement)
            );
        }
    }
    for rate in &entry.effective_rates {
        let labels = rate
            .dims
            .iter()
            .map(|dim| match dim {
                BoundDim::Origin => "origin".to_owned(),
                BoundDim::Endpoint => "endpoint".to_owned(),
                BoundDim::Custom { label, .. } => format!("\"{label}\""),
            })
            .collect::<Vec<_>>()
            .join(", ");
        let _ = writeln!(
            out,
            "- Rate policy `{}`: key [{labels}], cost {}, from `{}`",
            rate.id, rate.cost, rate.origin
        );
    }
    if entry.pagination.is_some() {
        let _ = writeln!(
            out,
            "- Pagination: `pages`, `collect_prefix` and `collect_all` are available; ordinary `.await` returns one page"
        );
    }
    if entry.expose_accepted_bytes {
        let _ = writeln!(
            out,
            "- `accepted_bytes()` bypasses representation validation and conversion"
        );
    }
    for credential in &entry.storage {
        let credential = &model.client.credentials[*credential];
        let _ = writeln!(
            out,
            "- `{}()` requests local installation into credential `{}`",
            credential.store_terminal, credential.logical
        );
    }
    // §2.3: a real rustdoc link to the expanded effective contract. The card
    // is attached to items of the owner's support module (and to constructor
    // impls written there), so the client type is `depth + 1` levels up.
    let client = &model.client.name;
    let path = "super::".repeat(model.owners[entry.owner].depth + 1);
    let _ = writeln!(
        out,
        "\nThe complete effective contract is available from [`{client}::contract()`]({path}{client}::contract)."
    );
    out
}

pub(crate) fn request(model: &Model, index: usize) -> String {
    let entry = &model.endpoints[index];
    let mut out = author(&entry.meta);
    let _ = writeln!(
        out,
        "Named request for `{}`. Holding it does not contact the server; `.await` executes one managed operation. {}\n",
        entry.display,
        if entry.is_clone() {
            "Cloning creates an independent operation and grants no retry authority."
        } else {
            "It is not `Clone` because the declaration has a body slot."
        }
    );
    out.push_str(&endpoint_card(model, index));
    if !entry.values.is_empty() {
        out.push_str("\n# Endpoint values\n\n");
        for value in &entry.values {
            out.push_str(&value_line(value));
            out.push('\n');
        }
    }
    out
}

pub(crate) fn constructor(model: &Model, index: usize) -> String {
    let entry = &model.endpoints[index];
    let mut out = author(&entry.meta);
    let required: Vec<String> = entry
        .positional
        .iter()
        .map(|positional| match positional {
            Positional::Value(value) => format!("`{}`", entry.values[*value].logical),
            Positional::Body => format!("body `{}`", entry.body.as_ref().expect("body").logical),
        })
        .collect();
    let _ = writeln!(
        out,
        "Creates the inert `{}` request{}. Endpoint defaults are evaluated once now.\n",
        entry.display,
        if required.is_empty() {
            String::new()
        } else {
            format!(" from its required inputs {}", required.join(", "))
        }
    );
    out.push_str(&endpoint_card(model, index));
    out
}

pub(crate) fn intent(model: &Model, index: usize) -> String {
    let entry = &model.endpoints[index];
    let mut out = author(&entry.meta);
    let _ = write!(
        out,
        "Unbound intent for `{}`: endpoint values are captured now; bind it with `{}::request` (or plan it with `{}::plan`). Direct and unbound+bind requests have equivalent plans for the same selected snapshots.",
        entry.display, model.client.name, model.client.name
    );
    out
}

pub(crate) fn client(model: &Model) -> String {
    let root = model.root();
    let mut out = author(&root.meta);
    let _ = writeln!(
        out,
        "Immutable generated client for base `{}`. Build it with [`{}::builder`](Self::builder); transport, runtime policy, client values and credential sources are fixed in the built snapshot. Clones share services and credential stores.",
        model.client.base.value(),
        model.client.name
    );
    if !root.values.is_empty() {
        out.push_str("\n# Client values\n\n");
        for value in &root.values {
            out.push_str(&value_line(value));
            out.push('\n');
        }
    }
    let _ = writeln!(
        out,
        "\nDefault timeout: {}.",
        timeout_text(root.effective_timeout)
    );
    if model.client.requires_limiter {
        out.push_str("\nRate policies are declared: building requires an explicitly selected limiter (`configure_runtime(|runtime| runtime.rate_limiter(...))`); Concord never installs one implicitly.\n");
    }
    out
}

pub(crate) fn builder(model: &Model) -> String {
    let root = model.root();
    let mut out = format!(
        "Builder for [`{}`]. Declaration defaults were evaluated once when this builder was created; `build` does not evaluate them again. Missing required values, secrets and providers are reported together in one `ClientBuildError` naming the setters, never the values. Build performs no network access.\n",
        model.client.name
    );
    let required: Vec<String> = root
        .values
        .iter()
        .filter(|value| value.is_required_input())
        .map(|value| format!("`set_{}`", value.logical))
        .chain(
            model
                .client
                .secrets
                .iter()
                .map(|secret| format!("`{}` (secret)", secret.logical)),
        )
        .collect();
    if !required.is_empty() {
        let _ = writeln!(out, "\nRequired inputs: {}.", required.join(", "));
    }
    out
}

/// V3-O01 / §2.3 / §19.2: the generated rustdoc attached to the actual caller
/// method and request type of the canonical §22 Catalog. The declaration is
/// expanded through the real emitter and the output parsed as Rust, so these
/// assertions read the `#[doc]` attributes rustdoc renders, not an
/// intermediate string.
#[cfg(test)]
mod tests {
    use proc_macro2::TokenStream;
    use quote::quote;

    /// The canonical §22 Catalog declaration
    /// (`crates/concord-examples/src/contracts/catalog_v3.rs`).
    fn catalog() -> TokenStream {
        quote! {
            /// Typed, tenant-scoped Catalog provider client.
            client CatalogApi {
                base "https://catalog.example.test/root/";
                timeout 20s;
                request Json;
                response Json;

                values {
                    /// The tenant is sent, but excluded from automatic observation.
                    #[sensitive] query tenant: TenantId;
                    query locale?: Locale;
                    #[default] api_version: ApiVersion;
                }

                credentials {
                    secret bootstrap_key;
                    credential application_key = api_key(secret.bootstrap_key);
                    credential session = endpoint authenticate;
                }

                rate_limits {
                    policy tenant_reads(tenant) {
                        id "catalog.tenant.read";
                        key [origin, "tenant" = tenant];
                        allow 120 per 1m;
                        allow 5000 per 1h;
                    }
                }

                subdomain ["api"];
                path "{api_version}";
                headers { "X-Client-Version" = "3"; }
                auth { header "X-Bootstrap-Key" = application_key; }

                /// Explicit login. Use store_as_session to request local installation.
                POST authenticate(body input: LoginInput) -> SessionToken {
                    id "catalog.auth.login";
                    path "auth/session";
                    effect non_idempotent;
                }

                scope regional(
                    region: Region,
                    header("Accept-Language") language?: Language,
                ) {
                    subdomain [region];
                    path "regions/{region}";
                    auth replace { bearer session challenge unauthorized; }
                    rate_limit tenant_reads(tenant = client.tenant);

                    /// One decoded page; select an explicit traversal terminal for more.
                    GET item_summaries(
                        query category?: Category,
                        query offset: u64 = 0,
                        query count: NonZeroU32 = DEFAULT_PAGE_SIZE,
                        query("tag") each tags: Vec<Tag>,
                    ) -> Vec<ItemSummary> {
                        id "catalog.items.list";
                        path "items";
                        paginate via OffsetPagination {
                            init OffsetInit { offset: offset, page_size: count };
                            cursor -> offset;
                        }
                    }

                    /// A head-only 404 is None; the body of that response is not polled.
                    GET find_item(id: ItemId) -> Option<Item> {
                        id "catalog.item.find";
                        path "items/{id}";
                        absent on 404;
                    }

                    /// Returns a managed byte stream, not a completed export.
                    #[deprecated(note = "use the paged feed instead")]
                    GET export() -> ByteStream via Binary {
                        id "catalog.item.export";
                        path "export";
                    }
                }
            }
        }
    }

    fn expand(tokens: TokenStream) -> syn::File {
        let expanded =
            crate::expand_with_facade(tokens, &quote!(::concord)).expect("valid declaration");
        syn::parse2(expanded).expect("generated code parses as a Rust file")
    }

    fn doc_text(attributes: &[syn::Attribute]) -> String {
        attributes
            .iter()
            .filter(|attribute| attribute.path().is_ident("doc"))
            .filter_map(|attribute| match &attribute.meta {
                syn::Meta::NameValue(pair) => match &pair.value {
                    syn::Expr::Lit(syn::ExprLit {
                        lit: syn::Lit::Str(text),
                        ..
                    }) => Some(text.value()),
                    _ => None,
                },
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Every generated method named `name` with its attributes, in any module.
    fn methods<'a>(items: &'a [syn::Item], name: &str, found: &mut Vec<&'a [syn::Attribute]>) {
        for item in items {
            match item {
                syn::Item::Impl(block) => {
                    for inner in &block.items {
                        if let syn::ImplItem::Fn(method) = inner {
                            if method.sig.ident == name {
                                found.push(&method.attrs);
                            }
                        }
                    }
                }
                syn::Item::Mod(module) => {
                    if let Some((_, items)) = &module.content {
                        methods(items, name, found);
                    }
                }
                _ => {}
            }
        }
    }

    fn structure<'a>(items: &'a [syn::Item], name: &str) -> Option<&'a syn::ItemStruct> {
        items.iter().find_map(|item| match item {
            syn::Item::Struct(structure) if structure.ident == name => Some(structure),
            syn::Item::Mod(module) => module
                .content
                .as_ref()
                .and_then(|(_, items)| structure(items, name)),
            _ => None,
        })
    }

    /// The scope-handle constructor (not the unbound intent constructor).
    fn constructor_docs(file: &syn::File, name: &str) -> (String, Vec<syn::Attribute>) {
        let mut found = Vec::new();
        methods(&file.items, name, &mut found);
        found
            .into_iter()
            .map(|attributes| (doc_text(attributes), attributes.to_vec()))
            .find(|(docs, _)| docs.contains("Creates the inert"))
            .unwrap_or_else(|| panic!("no generated constructor `{name}`"))
    }

    fn assert_contains(docs: &str, needles: &[&str]) {
        for needle in needles {
            assert!(
                docs.contains(needle),
                "generated docs lack `{needle}`:\n{docs}"
            );
        }
    }

    #[test]
    fn catalog_method_docs_show_the_caller_contract() {
        let file = expand(catalog());
        let (constructor, _) = constructor_docs(&file, "find_item");
        let request = doc_text(
            &structure(&file.items, "FindItemRequest")
                .expect("generated FindItemRequest")
                .attrs,
        );
        for docs in [&constructor, &request] {
            assert_contains(
                docs,
                &[
                    // Author description first.
                    "A head-only 404 is None; the body of that response is not polled.",
                    // Effective route, explicit operational ID.
                    "`GET https://{regional.region}.api.catalog.example.test/root/{client.api_version}/regions/{regional.region}/items/{id}`",
                    "endpoint ID `catalog.item.find`, explicit",
                    // Result and its effective representation.
                    "Result: `Option<Item>` via `Json`",
                    // Expected-absence status, head-only.
                    "Expected absence: `None` on 404 (head-only; that response body is dropped unread)",
                    // Effect and its source, effective timeout.
                    "Effect: safe (method default)",
                    "Timeout: 20 s",
                    // Inherited authentication truth and quota coverage.
                    "Authentication: bearer `session` (challenge on 401)",
                    "Rate policy `catalog.tenant.read`: key [origin, \"tenant\"], cost 1",
                    // The rustdoc link to the complete effective contract,
                    // relative to the `catalog_api::regional` support module.
                    "The complete effective contract is available from [`CatalogApi::contract()`](super::super::CatalogApi::contract).",
                ],
            );
        }
        // Required inputs are named on the method the caller invokes.
        assert_contains(&constructor, &["from its required inputs `id`"]);
        assert!(
            constructor.find("A head-only 404").expect("author text")
                < constructor.find("# Contract").expect("contract card"),
            "author description must precede the generated card:\n{constructor}"
        );
        // An endpoint-backed source never logs in automatically: the docs
        // must not advertise a recovery the source cannot perform (§2.3).
        for docs in [&constructor, &request] {
            let lower = docs.to_ascii_lowercase();
            assert!(
                !lower.contains("refresh") && !lower.contains("recover"),
                "docs advertise a recovery the endpoint-backed source cannot perform:\n{docs}"
            );
        }
    }

    #[test]
    fn catalog_docs_expose_traversal_value_controls_storage_and_deprecation() {
        let file = expand(catalog());

        let (summaries, _) = constructor_docs(&file, "item_summaries");
        assert_contains(
            &summaries,
            &[
                "One decoded page; select an explicit traversal terminal for more.",
                "from its required inputs `tags`",
                "Pagination: `pages`, `collect_prefix` and `collect_all` are available; ordinary `.await` returns one page",
                "Timeout: 20 s",
            ],
        );

        let (login, _) = constructor_docs(&file, "authenticate");
        assert_contains(
            &login,
            &[
                "Explicit login. Use store_as_session to request local installation.",
                "from its required inputs body `input`",
                "Effect: non-idempotent (declared)",
                "`store_as_session()` requests local installation into credential `session`",
                "Authentication: header \"X-Bootstrap-Key\" = `application_key`",
                // A root endpoint's card lives in the `catalog_api` module.
                "The complete effective contract is available from [`CatalogApi::contract()`](super::CatalogApi::contract).",
            ],
        );

        // Value controls carry the author's value docs and a one-line meaning.
        let mut category = Vec::new();
        methods(&file.items, "replace_category", &mut category);
        let category = doc_text(category.first().expect("replace_category control"));
        assert_contains(
            &category,
            &[
                "Replaces the outer slot of `category` exactly: `Some(v)` is present, `None` is absent.",
            ],
        );
        let mut tenant = Vec::new();
        methods(&file.items, "set_tenant", &mut tenant);
        let tenant = doc_text(tenant.first().expect("set_tenant control"));
        assert_contains(
            &tenant,
            &[
                "The tenant is sent, but excluded from automatic observation.",
                "withheld from automatic plans, errors and observers",
            ],
        );

        // Deprecation is emitted where calling triggers the warning.
        let (export, attributes) = constructor_docs(&file, "export");
        assert_contains(
            &export,
            &["Returns a managed byte stream, not a completed export."],
        );
        let deprecated = attributes
            .iter()
            .find(|attribute| attribute.path().is_ident("deprecated"))
            .expect("the deprecated endpoint's constructor carries #[deprecated]");
        let note = quote!(#deprecated).to_string();
        assert!(note.contains("use the paged feed instead"), "{note}");

        // The builder names every required input without values.
        let builder = doc_text(
            &structure(&file.items, "CatalogApiBuilder")
                .expect("generated builder")
                .attrs,
        );
        assert_contains(
            &builder,
            &["Required inputs: `set_tenant`, `bootstrap_key` (secret)."],
        );
    }

    fn allows_deprecated(attributes: &[syn::Attribute]) -> bool {
        attributes.iter().any(|attribute| {
            attribute.path().is_ident("allow")
                && quote!(#attribute).to_string().contains("deprecated")
        })
    }

    fn support_module<'a>(file: &'a syn::File, name: &str) -> &'a syn::ItemMod {
        file.items
            .iter()
            .find_map(|item| match item {
                syn::Item::Mod(module) if module.ident == name => Some(module),
                _ => None,
            })
            .unwrap_or_else(|| panic!("no generated support module `{name}`"))
    }

    /// §3.5 regression (V3-O01, `tests/ui/fail/deprecation`): deprecation
    /// fires at call sites only. The generated support code of a declaration
    /// with a deprecated endpoint or scope allows its own uses of those items
    /// (a `#![deny(deprecated)]` crate could not declare them otherwise), and
    /// the unbound navigation of a deprecated scope warns like the bound one.
    #[test]
    fn deprecation_fires_at_call_sites_not_in_generated_support_code() {
        let file = expand(quote! {
            client LegacyApi {
                base "https://legacy.example.test/";

                #[deprecated(note = "use the paged feed instead")]
                GET export() -> Bytes via Binary;

                #[deprecated(note = "regions are resolved by the provider")]
                scope regional(region: String) {
                    path "regions/{region}";
                    GET status() -> Bytes via Binary;
                }
            }
        });
        assert!(
            allows_deprecated(&support_module(&file, "legacy_api").attrs),
            "the support module must not report its own deprecated items"
        );
        let debug =
            file.items
                .iter()
                .find_map(|item| match item {
                    syn::Item::Impl(block)
                        if block.trait_.as_ref().is_some_and(|(path, _)| {
                            quote!(#path).to_string().contains("Debug")
                        }) =>
                    {
                        Some(block)
                    }
                    _ => None,
                })
                .expect("client Debug impl");
        assert!(allows_deprecated(&debug.attrs));

        // Bound and unbound scope navigation both carry the deprecation.
        let mut navigation = Vec::new();
        methods(&file.items, "regional", &mut navigation);
        assert_eq!(navigation.len(), 2, "bound and unbound `regional`");
        for attributes in navigation {
            let deprecated = attributes
                .iter()
                .find(|attribute| attribute.path().is_ident("deprecated"))
                .expect("deprecated scope navigation carries #[deprecated]");
            let note = quote!(#deprecated).to_string();
            assert!(
                note.contains("regions are resolved by the provider"),
                "{note}"
            );
        }

        // An ordinary declaration (a deprecated value alone included: its
        // controls are never called internally) keeps every deprecation
        // diagnostic for the user types it names.
        let plain = expand(quote! {
            client PlainApi {
                base "https://plain.example.test/";
                values {
                    #[deprecated(note = "no longer sent")]
                    legacy?: String;
                }

                GET export() -> Bytes via Binary;
            }
        });
        assert!(!allows_deprecated(
            &support_module(&plain, "plain_api").attrs
        ));
    }
}

#[cfg(test)]
mod s4_timeout_doc_regression {
    #[test]
    fn generated_default_timeout_documentation_matches_the_contract() {
        assert_eq!(super::timeout_text(None), "30 s (root default)");
        assert_eq!(
            super::timeout_text(Some(std::time::Duration::from_secs(7))),
            "7 s"
        );
        let source =
            "client TimeoutDoc { base \"https://fixture.test/\"; GET read() -> () via Empty; }";
        let model =
            crate::semantic::resolve(crate::syntax::parse(source.parse().unwrap()).unwrap())
                .unwrap();
        let emitted = crate::emit::generate(&model, &quote::quote!(::concord));
        let file = syn::parse2::<syn::File>(emitted).unwrap();
        fn docs(items: &[syn::Item], text: &mut String) {
            for item in items {
                let attrs = match item {
                    syn::Item::Struct(s) => Some(&s.attrs),
                    syn::Item::Mod(m) => Some(&m.attrs),
                    _ => None,
                };
                if let Some(attrs) = attrs {
                    for attr in attrs.iter().filter(|a| a.path().is_ident("doc")) {
                        if let syn::Meta::NameValue(meta) = &attr.meta {
                            if let syn::Expr::Lit(lit) = &meta.value {
                                if let syn::Lit::Str(lit) = &lit.lit {
                                    text.push_str(&lit.value());
                                }
                            }
                        }
                    }
                }
                if let syn::Item::Mod(module) = item {
                    if let Some((_, items)) = &module.content {
                        docs(items, text);
                    }
                }
            }
        }
        let mut text = String::new();
        docs(&file.items, &mut text);
        assert!(text.contains("30 s (root default)"));
        assert!(!text.contains("unless the runtime configuration"));
    }
}
