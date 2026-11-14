//! Caller-facing documentation, derived only from the checked description.
use super::*;

pub(super) fn lines(lines: impl IntoIterator<Item = String>) -> TokenStream {
    let lines = lines.into_iter().map(|line| format!(" {line}"));
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

    pub(super) fn type_doc(&self, ty: &syn::Type) -> String {
        self.type_doc_at(ty, false)
    }
    pub(super) fn alias_type_doc(&self, ty: &syn::Type) -> String {
        self.type_doc_at(ty, true)
    }
    fn type_doc_at(&self, ty: &syn::Type, in_child: bool) -> String {
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
                // Alias declarations live one module below the author's scope.
                // Keep the label as authored; only the link target moves.
                let mut text = if in_child {
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
                        format!("super::{tail}")
                    } else {
                        format!("super::{spelling}")
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
                            syn::GenericArgument::Type(ty) => self.type_doc_at(ty, in_child),
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
                    .map(|t| self.type_doc_at(t, in_child))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            syn::Type::Reference(reference) => {
                format!("&{}", self.type_doc_at(&reference.elem, in_child))
            }
            syn::Type::Slice(slice) => format!("[{}]", self.type_doc_at(&slice.elem, in_child)),
            syn::Type::Paren(paren) => self.type_doc_at(&paren.elem, in_child),
            other => format!(
                "`{}`",
                crate::diag::Written::new(other.to_token_stream()).text()
            ),
        }
    }
    pub(super) fn result_doc(&self, endpoint: &flat::Endpoint<'_>) -> String {
        let value = endpoint
            .node
            .output
            .as_ref()
            .map_or_else(|| "`()`".into(), |o| self.type_doc(&o.ty));
        format!("[`{}::Result`]<{value}>", self.root)
    }
    pub(super) fn accepts(&self, input: &Input, optional: bool) -> String {
        let ty = self.type_doc(&input.ty);
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
            text.push_str(" Text and list conversions: [`concord::arg::Arg`].");
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
        let mut text = Vec::new();
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
        text.extend([String::new(), format!("Call: `{call}{tail}`.")]);
        let base = endpoint
            .base
            .as_ref()
            .map(|b| match b.value {
                Base::Template(t) => t.literal.value(),
                Base::Reference(r) => format!("{{{}}}", flat::reference_text(r)),
            })
            .unwrap_or_default();
        let mut url = base;
        for path in &endpoint.paths {
            if !url.ends_with('/') {
                url.push('/');
            }
            url.push_str(&path.line.value.literal.value());
        }
        let method = if let NodeKind::Endpoint(m) = &endpoint.node.kind {
            method(m)
        } else {
            String::new()
        };
        let mut queries = endpoint.query.iter().collect::<Vec<_>>();
        queries.sort_by_key(|e| e.position);
        if !queries.is_empty() {
            url.push_str(" ?");
            url.push_str(
                &queries
                    .iter()
                    .map(|e| e.key.as_str())
                    .collect::<Vec<_>>()
                    .join("&"),
            );
        }
        text.extend([String::new(), format!("Request: `{method} {url}`.")]);
        if !endpoint.headers.is_empty() {
            let names = endpoint
                .headers
                .iter()
                .map(|e| format!("`{}`", e.key))
                .collect::<Vec<_>>()
                .join(", ");
            text.extend([String::new(), format!("Headers: {names}.")]);
        }
        let node = self.nodes.get(index);
        if let Some(node) = node {
            let required: Vec<_> = node
                .inputs
                .iter()
                .filter(|i| i.input.optional.is_none())
                .collect();
            if !required.is_empty() {
                text.extend([String::new(), "Inputs:".into()]);
                for input in required {
                    text.push(format!(
                        "- `{}`: {}",
                        input.input.name,
                        self.accepts(input.input, false)
                    ));
                    text.extend(
                        authored(&input.input.attrs)
                            .into_iter()
                            .map(|l| format!("  {l}")),
                    );
                }
            }
            let optional: Vec<_> = node
                .inputs
                .iter()
                .filter(|i| i.input.optional.is_some())
                .collect();
            if !optional.is_empty() {
                text.extend([String::new(), "Options:".into()]);
                for input in optional {
                    text.push(format!(
                        "- `{}`: {}",
                        input.input.name,
                        self.accepts(input.input, true)
                    ));
                    text.extend(
                        authored(&input.input.attrs)
                            .into_iter()
                            .map(|l| format!("  {l}")),
                    );
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
            text.extend([String::new(), "This endpoint may not be repeated.".into()]);
        }
        let action = if endpoint.pager.is_some() {
            ".send().await"
        } else {
            ".await"
        };
        text.extend([
            String::new(),
            format!("`{action}` gives {}.", self.result_doc(endpoint)),
        ]);
        if endpoint.pager.is_some() {
            text.extend([String::new(), "`.send().await` gives one page; `.pages()` gives them all, and `.pages().items()` each item.".into()]);
        }
        lines(text)
    }
}
