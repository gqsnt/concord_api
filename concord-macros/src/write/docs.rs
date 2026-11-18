//! Caller-facing documentation, derived only from the checked description.
use super::*;

pub(super) fn call_must_use(paged: bool) -> TokenStream {
    let text = if paged {
        "await `.send()` for one page, or use `.pages()` for all pages"
    } else {
        "a call does nothing until sent or awaited"
    };
    emit!(#[must_use = #text])
}

pub(super) fn lines(lines: impl IntoIterator<Item = String>) -> TokenStream {
    let lines = lines.into_iter().map(|line| {
        if line.is_empty() {
            line
        } else {
            format!(" {line}")
        }
    });
    emit!(#(#[doc = #lines])*)
}
pub(super) fn authored(attrs: &[syn::Attribute]) -> Vec<String> {
    attrs
        .iter()
        .filter_map(|attr| {
            if !attr.path().is_ident("doc") {
                return None;
            }
            if let syn::Meta::NameValue(value) = &attr.meta {
                if let syn::Expr::Lit(lit) = &value.value {
                    if let syn::Lit::Str(text) = &lit.lit {
                        let text = text.value();
                        return Some(text.strip_prefix(' ').unwrap_or(&text).to_owned());
                    }
                }
            }
            None
        })
        .collect()
}

fn endpoint_name(endpoint: &flat::Endpoint<'_>) -> String {
    endpoint
        .path
        .split('.')
        .skip(1)
        .collect::<Vec<_>>()
        .join("::")
}

impl Writer<'_> {
    pub(super) fn problem_type(&self) -> Option<&syn::Type> {
        self.model
            .levels
            .iter()
            .find(|level| matches!(level.node.kind, NodeKind::Client))?
            .statements
            .iter()
            .find_map(|statement| match &statement.value.kind {
                StatementKind::Problem { ty, .. } => Some(ty.as_ref()),
                _ => None,
            })
    }
    pub(super) fn has_problem(&self) -> bool {
        self.problem_type().is_some()
    }

    pub(super) fn alias_type_doc(&self, ty: &syn::Type) -> String {
        self.type_doc_at(ty, 1)
    }
    fn type_doc_at(&self, ty: &syn::Type, depth: usize) -> String {
        match ty {
            syn::Type::Path(path) => {
                let spelling = path
                    .path
                    .segments
                    .iter()
                    .map(|s| s.ident.to_string())
                    .collect::<Vec<_>>()
                    .join("::");
                let collision =
                    path.path.segments.len() == 1 && self.generated_names.contains(&spelling);
                let shown = if collision {
                    format!("self::{spelling}")
                } else {
                    spelling.clone()
                };
                let args = path.path.segments.last().map(|s| &s.arguments);
                // Declarations in the generated tree link back to the author's scope.
                let mut text = if depth > 0 {
                    let first = path
                        .path
                        .segments
                        .first()
                        .map(|s| s.ident.to_string())
                        .unwrap_or_default();
                    let target = if path.path.leading_colon.is_some()
                        || matches!(
                            first.as_str(),
                            "crate"
                                | "std"
                                | "core"
                                | "alloc"
                                | "concord"
                                | "String"
                                | "Vec"
                                | "Box"
                                | "Option"
                                | "Result"
                                | "bool"
                                | "char"
                                | "str"
                                | "u8"
                                | "u16"
                                | "u32"
                                | "u64"
                                | "u128"
                                | "usize"
                                | "i8"
                                | "i16"
                                | "i32"
                                | "i64"
                                | "i128"
                                | "isize"
                                | "f32"
                                | "f64"
                        ) {
                        spelling
                    } else if let Some(tail) = spelling.strip_prefix("self::") {
                        format!("{}{tail}", "super::".repeat(depth))
                    } else {
                        format!("{}{spelling}", "super::".repeat(depth))
                    };
                    format!("[`{shown}`]({target})")
                } else {
                    format!("[`{shown}`]")
                };
                if let Some(syn::PathArguments::AngleBracketed(args)) = args {
                    let args = args
                        .args
                        .iter()
                        .map(|arg| match arg {
                            syn::GenericArgument::Type(ty) => self.type_doc_at(ty, depth),
                            other => crate::diag::Written::new(other.to_token_stream()).text(),
                        })
                        .collect::<Vec<_>>()
                        .join(", ");
                    text.push_str(&format!("<{args}>"));
                }
                text
            }
            syn::Type::Tuple(tuple) if tuple.elems.is_empty() => "`()`".into(),
            syn::Type::Tuple(tuple) => format!(
                "({})",
                tuple
                    .elems
                    .iter()
                    .map(|t| self.type_doc_at(t, depth))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            syn::Type::Reference(reference) => {
                format!("&{}", self.type_doc_at(&reference.elem, depth))
            }
            syn::Type::Slice(slice) => format!("[{}]", self.type_doc_at(&slice.elem, depth)),
            syn::Type::Paren(paren) => self.type_doc_at(&paren.elem, depth),
            other => format!(
                "`{}`",
                crate::diag::Written::new(other.to_token_stream()).text()
            ),
        }
    }
    fn result_doc_at(&self, endpoint: &flat::Endpoint<'_>, depth: usize) -> String {
        let value = endpoint
            .node
            .output
            .as_ref()
            .map_or_else(|| "`()`".into(), |o| self.type_doc_at(&o.ty, depth));
        let result = if depth == 0 {
            format!("[`{}::Result`]", self.root)
        } else {
            format!(
                "[`{}::Result`]({}{}::Result)",
                self.root,
                "super::".repeat(depth),
                self.root
            )
        };
        format!("{result}<{value}>")
    }
    pub(super) fn accepts(&self, input: &Input, optional: bool) -> String {
        self.accepts_at(input, optional, 0)
    }
    fn accepts_at(&self, input: &Input, optional: bool, depth: usize) -> String {
        let ty = self.type_doc_at(&input.ty, depth);
        let option = matches!(flat::arrow(&input.ty).0, Some(flat::Wrapper::Option));
        let article = if option { "an" } else { "a" };
        let mut text = if optional && option {
            let inner = flat::arrow(&input.ty).1;
            let inner = crate::diag::Written::new(inner.to_token_stream()).text();
            let spelled = crate::diag::Written::new(input.ty.to_token_stream()).text();
            format!("Accepts {article} {ty}, a &{ty}, or an [`Option`]<{ty}>. `Some(value)` sets a value. `None::<{inner}>` sets an inner `None` (`null` in a JSON body). `None::<{spelled}>` clears it. A bare `None` needs a type annotation.")
        } else if optional {
            format!("Accepts {article} {ty}, a &{ty}, or an [`Option`]<{ty}>. `None` clears it.")
        } else {
            format!("Accepts {article} {ty} or a &{ty}.")
        };
        if matches!(&input.ty, syn::Type::Path(path) if path.path.segments.last().is_some_and(|s| matches!(s.ident.to_string().as_str(), "String" | "Secret" | "Vec")))
        {
            // The trait's own definition is hidden and published by an inline re-export, so an
            // intra-doc link cannot reach it, and an author's `cargo doc --no-deps` resolves no
            // link into the facade: link the published page of its module, for this release.
            text.push_str(concat!(
                " Text and list conversions: [`concord::arg::Arg`](https://docs.rs/concord-api/",
                env!("CARGO_PKG_VERSION"),
                "/concord/arg/index.html)."
            ));
        }
        text
    }
    pub(super) fn setter_doc(&self, input: &Input) -> TokenStream {
        lines(
            authored(&input.attrs)
                .into_iter()
                .chain([self.accepts(input, true)]),
        )
    }
    pub(super) fn endpoint_setter_doc(
        &self,
        endpoint: &flat::Endpoint<'_>,
        input: &Input,
        depth: usize,
    ) -> TokenStream {
        let mut text = vec![
            format!("Option `{}` of `{}`.", input.name, endpoint_name(endpoint)),
            String::new(),
        ];
        if depth > 0 {
            text.extend(["The first argument is the call.".into(), String::new()]);
        }
        let authored = authored(&input.attrs);
        if !authored.is_empty() {
            text.extend(authored);
            text.push(String::new());
        }
        text.push(self.accepts_at(input, true, depth));
        lines(text)
    }
    pub(super) fn client_doc(&self) -> TokenStream {
        let mut text = vec![format!("A client for `{}`.", self.root)];
        if !self.model.middleware.is_empty() {
            text.push(String::new());
            text.push("Every request goes through the client's middleware.".into());
            for level in &self.model.levels {
                for statement in &level.statements {
                    if matches!(statement.value.kind, StatementKind::Middleware(_)) {
                        text.extend(authored(&statement.value.attrs));
                    }
                }
            }
        }
        lines(text)
    }
    pub(super) fn endpoint_doc(&self, index: usize, endpoint: &flat::Endpoint<'_>) -> TokenStream {
        self.endpoint_doc_at(index, endpoint, 0)
    }
    pub(super) fn endpoint_type_doc(
        &self,
        index: usize,
        endpoint: &flat::Endpoint<'_>,
    ) -> TokenStream {
        let Some(node) = self.nodes.get(index) else {
            return TokenStream::new();
        };
        self.endpoint_doc_at(index, endpoint, node.modules.len())
    }

    fn request_line(&self, endpoint: &flat::Endpoint<'_>) -> (String, String) {
        let mut url = endpoint
            .base
            .as_ref()
            .map(|b| match b.value {
                Base::Template(t) => t.literal.value(),
                Base::Reference(r) => format!("{{{}}}", flat::reference_text(r)),
            })
            .unwrap_or_default();
        for path in &endpoint.paths {
            if !url.ends_with('/') {
                url.push('/');
            }
            url.push_str(&path.line.value.literal.value());
        }
        let verb = match &endpoint.node.kind {
            NodeKind::Endpoint(verb) => method(verb),
            _ => String::new(),
        };
        let mut queries = endpoint.query.iter().collect::<Vec<_>>();
        queries.sort_by_key(|entry| entry.position);
        if !queries.is_empty() {
            url.push_str(" ?");
            url.push_str(
                &queries
                    .iter()
                    .map(|entry| entry.key.as_str())
                    .collect::<Vec<_>>()
                    .join("&"),
            );
        }
        (verb, url)
    }

    fn endpoint_doc_at(
        &self,
        index: usize,
        endpoint: &flat::Endpoint<'_>,
        depth: usize,
    ) -> TokenStream {
        let mut text = Vec::new();
        let call_link =
            (depth > 0).then(|| format!("{}{}::Call", "super::".repeat(depth), self.root));
        let mut chain = Vec::new();
        let mut current = Some(index);
        while let Some(i) = current {
            if i == 0 {
                break;
            }
            chain.push(i);
            current = self.nodes.get(i).and_then(|n| n.parent);
        }
        chain.reverse();
        let mut call = self.root.to_string();
        for i in chain {
            if let Some(node) = self.nodes.get(i) {
                let args = node
                    .inputs
                    .iter()
                    .filter(|i| i.input.optional.is_none())
                    .map(|i| i.input.name.to_string())
                    .collect::<Vec<_>>()
                    .join(", ");
                call.push_str(&format!(".{}({args})", node.node.name));
            }
        }
        let tail = if endpoint.pager.is_some() {
            ".send().await?"
        } else {
            ".await?"
        };
        let Some(node) = self.nodes.get(index) else {
            return TokenStream::new();
        };
        let marker = name::type_name(&node.node.name.to_string());
        let path = node
            .modules
            .iter()
            .map(ToString::to_string)
            .chain([marker.clone()])
            .collect::<Vec<_>>()
            .join("::");
        let (method, url) = self.request_line(endpoint);
        if depth > 0 {
            let Some(parent) = node.parent.and_then(|i| self.nodes.get(i)) else {
                return TokenStream::new();
            };
            let owner = parent.node.name.to_string();
            let owner = if matches!(parent.node.kind, NodeKind::Client) {
                owner
            } else {
                name::type_name(&owner)
            };
            let entry = &node.node.name;
            text.extend([
                String::new(),
                format!("Request: `{method} {url}`."),
                String::new(),
                format!(
                    "Call type: `{}::Call<{path}>`, returned by `{owner}::{entry}`.",
                    self.root
                ),
                String::new(),
                format!("`{marker}` names the endpoint and has no value."),
            ]);
        } else {
            text.extend([
                String::new(),
                format!("Call: `{call}{tail}`."),
                String::new(),
                format!("Request: `{method} {url}`."),
            ]);
        }
        if !endpoint.headers.is_empty() {
            let names = endpoint
                .headers
                .iter()
                .map(|e| format!("`{}`", e.key))
                .collect::<Vec<_>>()
                .join(", ");
            text.extend([String::new(), format!("Headers: {names}.")]);
        }
        {
            let required: Vec<_> = node
                .inputs
                .iter()
                .filter(|i| i.input.optional.is_none())
                .collect();
            if !required.is_empty() {
                text.extend([String::new(), "Inputs:".into(), String::new()]);
                for input in required {
                    text.push(String::new());
                    text.push(format!(
                        "`{}`: {}",
                        input.input.name,
                        self.accepts_at(input.input, false, depth)
                    ));
                    text.push(String::new());
                    text.extend(authored(&input.input.attrs));
                }
            }
            let optional: Vec<_> = node
                .inputs
                .iter()
                .filter(|i| i.input.optional.is_some())
                .collect();
            if let Some(first) = optional.first() {
                text.extend([String::new(), "Options:".into(), String::new()]);
                if depth > 0 {
                    let name = &first.input.name;
                    text.extend([
                        format!("Each option is a method of the call (`call.{name}(v)`) and a function of this type (`{marker}::{name}(call, v)`)."),
                        String::new(),
                    ]);
                }
                for input in optional {
                    text.push(String::new());
                    let name = &input.input.name;
                    let label = call_link.as_ref().map_or_else(
                        || format!("`{name}`"),
                        |_| {
                            let ty =
                                crate::diag::Written::new(input.input.ty.to_token_stream()).text();
                            format!("[`{name}(impl Arg<Option<{ty}>>)`](Self::{name})")
                        },
                    );
                    text.push(format!(
                        "{label}: {}",
                        self.accepts_at(input.input, true, depth)
                    ));
                    text.push(String::new());
                    text.extend(authored(&input.input.attrs));
                }
            }
        }
        if !endpoint.arms.is_empty() {
            text.extend([
                String::new(),
                format!(
                    "Status: {}.",
                    endpoint
                        .arms
                        .iter()
                        .map(|a| format!("`{}`", a.value.key))
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            ]);
        }
        let cannot_repeat = match endpoint.idempotent.as_ref().map(|line| line.value) {
            None => !matches!(
                method.as_str(),
                "GET" | "HEAD" | "OPTIONS" | "PUT" | "DELETE"
            ),
            Some(Some(syn::Expr::Lit(l))) => matches!(&l.lit, syn::Lit::Bool(b) if !b.value),
            _ => false,
        };
        if cannot_repeat {
            text.extend([String::new(), "A retry does not repeat this endpoint unless `Retry::not_applied_on` lists the status.".into()]);
        }
        let action = if endpoint.pager.is_some() {
            ".send().await"
        } else {
            ".await"
        };
        text.extend([
            String::new(),
            format!(
                "`{action}` returns {}.",
                self.result_doc_at(endpoint, depth)
            ),
        ]);
        if endpoint.pager.is_some() {
            text.extend([
                String::new(),
                "`.send().await` returns one page; `.pages()` yields them all, and `.pages().items()` each item.".into(),
            ]);
        }
        if let Some(call) = call_link {
            text.extend([
                String::new(),
                format!("Methods shared by every call: see [`Call`]({call})."),
            ]);
        } else {
            text.extend([
                String::new(),
                format!("See [`{marker}`]({path}) for the endpoint documentation."),
            ]);
        }
        lines(text)
    }
}
