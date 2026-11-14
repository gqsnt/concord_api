//! Request construction: expression order first, independent wire order second.
use super::*;

fn reference(reference: &Reference) -> TokenStream {
    let name = &reference.name;
    if let Some(owner) = &reference.owner {
        emit!(#owner.#name)
    } else {
        emit!(#name)
    }
}

fn adapt(value: TokenStream, gate: Option<&str>, span: Span) -> TokenStream {
    let value = emit_at!(span => (#value).adapt());
    if let Some(gate) = gate {
        let gate = ident(gate);
        emit_at!(span => ::concord::__private::#gate(#value))
    } else {
        value
    }
}

fn adapted(value: TokenStream, gate: Option<&str>, span: Span) -> TokenStream {
    adapt(
        emit_at!(span => &::concord::__private::ValueRef(#value)),
        gate,
        span,
    )
}

fn pieces(template: &Template) -> TokenStream {
    let pieces = template.pieces.iter().map(|piece| match piece {
        TemplatePiece::Text { text, .. } => emit!(::concord::__private::Piece::Lit(#text)),
        TemplatePiece::Hole { reference: r, .. } => {
            let name = flat::reference_text(r);
            let value = adapted(reference(r), None, r.span);
            emit!(::concord::__private::Piece::Val(#name, &#value))
        }
    });
    emit!(&[::concord::__private::Piece::Lit(""), #(#pieces),*])
}

fn path_segments(template: &Template, gate: Option<&str>) -> TokenStream {
    let mut segments = Vec::<(bool, Vec<TokenStream>)>::new();
    let mut segment = Vec::new();
    let mut trailing = false;
    for piece in &template.pieces {
        match piece {
            TemplatePiece::Text { text, .. } => {
                let mut split = text.split('/').peekable();
                while let Some(text) = split.next() {
                    if !text.is_empty() {
                        segment.push(emit!(::concord::__private::Piece::Lit(#text)));
                    }
                    if split.peek().is_some() {
                        if !segment.is_empty() {
                            segments.push((false, std::mem::take(&mut segment)));
                        }
                        trailing = true;
                    } else if !text.is_empty() {
                        trailing = false;
                    }
                }
            }
            TemplatePiece::Hole {
                reference: r, star, ..
            } => {
                let value = adapted(reference(r), gate, r.span);
                let name = flat::reference_text(r);
                if *star {
                    segments.push((true, vec![emit!(__draft.segments(#name, &#value);)]));
                } else {
                    segment.push(emit!(::concord::__private::Piece::Val(#name, &#value)));
                }
                trailing = false;
            }
        }
    }
    if !segment.is_empty() {
        segments.push((false, segment));
    }
    let last = segments.len().saturating_sub(1);
    segments
        .iter()
        .enumerate()
        .map(|(i, (splat, segment))| {
            if *splat {
                let tail = (trailing && i == last).then(|| emit!(__draft.trailing_separator();));
                emit!(#(#segment)* #tail)
            } else {
                let tail =
                    (trailing && i == last).then(|| emit!(, ::concord::__private::Piece::Lit("/")));
                emit!(__draft.segment(&[#(#segment),* #tail]);)
            }
        })
        .collect()
}

fn same_writer(a: &flat::Origin<'_>, b: &flat::Origin<'_>) -> bool {
    std::ptr::eq(a.written, b.written) && std::ptr::eq(a.placed, b.placed)
}

/// Export expression values, never an author's lexical bindings. Holders and
/// adapters live outside the writer's block, until the independent wire pass.
#[derive(Default)]
struct Values {
    names: Vec<Ident>,
    evaluate: TokenStream,
}
impl Values {
    fn bind(&mut self, name: Ident, value: TokenStream) {
        self.evaluate.extend(emit!(let #name = #value;));
        self.names.push(name);
    }
    fn block(self, bindings: TokenStream) -> TokenStream {
        let names = self.names;
        let evaluate = self.evaluate;
        if names.is_empty() {
            return emit!({ #bindings #evaluate });
        }
        emit!(let (#(#names,)*) = { #bindings #evaluate (#(#names,)*) };)
    }
}

impl Writer<'_> {
    pub(super) fn request(&self, index: usize, endpoint: &flat::Endpoint<'_>) -> TokenStream {
        let root = &self.root;
        let core = self.core(index);
        let method = self.request_method(endpoint);
        let base = self.request_base(index, endpoint);
        let paths = self.request_paths(index, endpoint);
        let query = self.entries(index, &endpoint.query, "query");
        let headers = self.entries(index, &endpoint.headers, "header");
        let body = self.request_body(index, endpoint);
        let options = self.borrow_field(index, emit!(self.__options), false);
        emit! {
            fn __build_draft(&self) -> #root::Result<::concord::__private::Draft> {
                let mut __draft = ::concord::__private::Draft::new(#method);
                #base #paths #query #headers #body
                ::core::result::Result::Ok(__draft)
            }
            fn __build_request(&self, __take: bool) -> #root::Result<::concord::reqwest::Request> {
                let __draft = self.__build_draft()?;
                if __take { __draft.finish_for_send(&#core, #options, Self::__ENDPOINT) }
                else { __draft.finish(&#core, #options, Self::__ENDPOINT) }
            }
        }
    }

    fn request_method(&self, endpoint: &flat::Endpoint<'_>) -> TokenStream {
        let NodeKind::Endpoint(method) = &endpoint.node.kind else {
            self.internal(
                endpoint.node.name.span(),
                "a request has no endpoint method",
            );
            return TokenStream::new();
        };
        if let Some(custom) = &method.custom {
            emit! {
                ::concord::http::Method::from_bytes(#custom.as_bytes())
                    .map_err(|__error| ::concord::__private::build_error(Self::__ENDPOINT, ::std::boxed::Box::new(__error)))?
            }
        } else {
            let method = &method.keyword;
            emit!(::concord::http::Method::#method)
        }
    }

    fn request_base(&self, index: usize, endpoint: &flat::Endpoint<'_>) -> TokenStream {
        endpoint
            .base
            .as_ref()
            .map(|line| {
                let bindings = self.bindings(&line.origin, index, emit!(self), false);
                let base = match line.value {
                    Base::Template(template) => {
                        let pieces = pieces(template);
                        emit!(__draft.base(#pieces);)
                    }
                    Base::Reference(r) => {
                        let value = reference(r);
                        emit_at!(r.span => __draft.base_from(#value);)
                    }
                };
                emit!({ #bindings #base })
            })
            .unwrap_or_default()
    }

    fn request_paths(&self, index: usize, endpoint: &flat::Endpoint<'_>) -> TokenStream {
        endpoint
            .paths
            .chunk_by(|a, b| same_writer(&a.line.origin, &b.line.origin))
            .map(|paths| {
                let Some(first) = paths.first() else {
                    return TokenStream::new();
                };
                let bindings = self.bindings_at(
                    &first.line.origin,
                    index,
                    emit!(self),
                    false,
                    first.line.origin.span,
                );
                let paths = paths.iter().map(|path| {
                    path_segments(
                        path.line.value,
                        path.crosses_base.then_some("inherited_path"),
                    )
                });
                emit!({ #bindings #(#paths)* })
            })
            .collect()
    }

    fn request_body(&self, index: usize, endpoint: &flat::Endpoint<'_>) -> TokenStream {
        match &endpoint.body {
            Some(flat::BodyLine::Expression(line)) => {
                let bindings = self.bindings(&line.origin, index, emit!(self), false);
                let expression = line.value;
                let place = emit_at!(expression.span() => __draft.body(__body););
                emit!({
                    #bindings
                    let __body = (|| -> ::core::result::Result<_, ::std::boxed::Box<dyn ::std::error::Error + ::core::marker::Send + ::core::marker::Sync>> {
                        ::core::result::Result::Ok({ #expression })
                    })().map_err(|__error| ::concord::__private::build_error(Self::__ENDPOINT, __error))?;
                    #place
                })
            }
            Some(flat::BodyLine::Entries { function, entries }) => {
                self.body_entries(index, function.value, entries)
            }
            None => TokenStream::new(),
        }
    }

    fn body_entries(
        &self,
        index: usize,
        function: &syn::ExprPath,
        entries: &[flat::EntryLine<'_>],
    ) -> TokenStream {
        let mut setup = TokenStream::new();
        let mut writes = TokenStream::new();
        let mut next = 0;
        for group in entries.chunk_by(|a, b| same_writer(&a.line.origin, &b.line.origin)) {
            let Some(first) = group.first() else {
                continue;
            };
            let bindings = self.bindings_at(
                &first.line.origin,
                index,
                emit!(self),
                false,
                first.line.origin.span,
            );
            let mut values = Values::default();
            let mut holders = TokenStream::new();
            for entry in group {
                if matches!(entry.line.value.kind, EntryKind::Remove(_)) {
                    continue;
                }
                let name = ident(&format!("__BodyValue{next}"));
                let raw = ident(&format!("__BodyExpression{next}"));
                next += 1;
                let (value, span, template) = match &entry.line.value.kind {
                    EntryKind::Short(r) => (reference(r), r.span, false),
                    EntryKind::Value {
                        value: Value::Expression(expr),
                        ..
                    } => (emit!(#expr), expr.span(), false),
                    EntryKind::Value {
                        value: Value::Template(template),
                        ..
                    } => {
                        let pieces = pieces(template);
                        (
                            emit!(::concord::__private::Draft::body_template(Self::__ENDPOINT, #pieces)?),
                            template.literal.span(),
                            true,
                        )
                    }
                    EntryKind::Remove(_) => continue,
                };
                let value = if template {
                    value
                } else {
                    emit!((|| -> ::core::result::Result<_, ::std::boxed::Box<dyn ::std::error::Error + ::core::marker::Send + ::core::marker::Sync>> {
                        ::core::result::Result::Ok({ #value })
                    })().map_err(|__error| ::concord::__private::build_error(Self::__ENDPOINT, __error))?)
                };
                values.bind(raw.clone(), value);
                holders.extend(emit!(let #name = ::concord::__private::ValueRef(#raw);));
                let key = flat::key(entry.line.value, false);
                let value = adapt(emit!(&#name), None, span);
                writes.extend(emit!(__entries.__push(#key, #value);));
            }
            setup.extend(values.block(bindings));
            setup.extend(holders);
        }
        emit!({
            fn __body_function(__entries: ::concord::body::Entries<'_>) -> impl ::concord::body::IntoBody { #function(__entries) }
            #setup
            let mut __entries = ::concord::body::Entries::new();
            #writes
            __draft.body(__body_function(__entries));
        })
    }

    fn entries(&self, index: usize, entries: &[flat::EntryLine<'_>], method: &str) -> TokenStream {
        if entries.is_empty() {
            return TokenStream::new();
        }
        let method = ident(method);
        let mut setup = TokenStream::new();
        let mut names = Vec::new();
        let mut next = 0;
        for group in entries.chunk_by(|a, b| same_writer(&a.line.origin, &b.line.origin)) {
            let Some(first) = group.first() else {
                continue;
            };
            let bindings = self.bindings_at(
                &first.line.origin,
                index,
                emit!(self),
                false,
                first.line.origin.span,
            );
            let mut values = Values::default();
            let mut holders = TokenStream::new();
            for entry in group {
                let name = ident(&format!("__Entry{next}"));
                holders.extend(self.entry_value(entry, &name, next, &mut values));
                next += 1;
                names.push((entry, name));
            }
            setup.extend(values.block(bindings));
            setup.extend(holders);
        }
        names.sort_by_key(|(entry, _)| entry.position);
        let writes = names.iter().map(|(entry, name)| {
            let key = flat::key(entry.line.value, false);
            emit!(__draft.#method(#key, #name);)
        });
        emit!({ #setup #(#writes)* })
    }

    fn entry_value(
        &self,
        entry: &flat::EntryLine<'_>,
        name: &Ident,
        index: usize,
        values: &mut Values,
    ) -> TokenStream {
        let gate = entry.crosses_base.then_some("inherited");
        let holder = ident(&format!("__EntryValue{index}"));
        let value = ident(&format!("__EntryExpression{index}"));
        let converted = ident(&format!("__EntryAdapted{index}"));
        let key = flat::key(entry.line.value, false);
        match &entry.line.value.kind {
            EntryKind::Short(r) => {
                let reference = reference(r);
                let error_name = flat::reference_text(r);
                let adapt = adapt(emit!(&#holder), gate, r.span);
                values.bind(value.clone(), reference);
                emit! {
                    let #holder = ::concord::__private::ValueRef(#value);
                    let #converted = #adapt;
                    let #name = &[::concord::__private::Piece::Val(#error_name, &#converted)];
                }
            }
            EntryKind::Value {
                value: Value::Expression(expr),
                ..
            } => {
                let adapt = adapt(emit!(&#holder), gate, expr.span());
                values.bind(value.clone(), emit!((|| -> ::core::result::Result<_, ::std::boxed::Box<dyn ::std::error::Error + ::core::marker::Send + ::core::marker::Sync>> {
                    ::core::result::Result::Ok({ #expr })
                })().map_err(|__error| ::concord::__private::Draft::entry_error(Self::__ENDPOINT, #key, __error))?));
                emit! {
                    let #holder = ::concord::__private::ValueRef(&#value);
                    let #converted = #adapt;
                    let #name = &[::concord::__private::Piece::Val(#key, &#converted)];
                }
            }
            EntryKind::Value {
                value: Value::Template(template),
                ..
            } => {
                let mut setup = TokenStream::new();
                let mut pieces = vec![emit!(::concord::__private::Piece::Lit(""))];
                for (i, piece) in template.pieces.iter().enumerate() {
                    match piece {
                        TemplatePiece::Text { text, .. } => {
                            pieces.push(emit!(::concord::__private::Piece::Lit(#text)))
                        }
                        TemplatePiece::Hole { reference: r, .. } => {
                            let reference = reference(r);
                            let holder = ident(&format!("__Entry{index}Hole{i}"));
                            let converted = ident(&format!("__Entry{index}Adapted{i}"));
                            let raw = ident(&format!("__Entry{index}Expression{i}"));
                            values.bind(raw.clone(), reference);
                            let error_name = flat::reference_text(r);
                            let adapt = adapt(emit!(&#holder), gate, r.span);
                            setup.extend(emit!(let #holder = ::concord::__private::ValueRef(#raw); let #converted = #adapt;));
                            pieces.push(
                                emit!(::concord::__private::Piece::Val(#error_name, &#converted)),
                            );
                        }
                    }
                }
                emit!(#setup let #name = &[#(#pieces),*];)
            }
            EntryKind::Remove(_) => TokenStream::new(),
        }
    }
}
