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
    pieces_with(template, reference)
}

fn pieces_with(template: &Template, binding: impl Fn(&Reference) -> TokenStream) -> TokenStream {
    let pieces = template.pieces.iter().map(|piece| match piece {
        TemplatePiece::Text { text, .. } => emit!(::concord::__private::Piece::Lit(#text)),
        TemplatePiece::Hole { reference: r, .. } => {
            let name = flat::reference_text(r);
            let value = adapted(binding(r), None, r.span);
            emit!(::concord::__private::Piece::Val(#name, &#value))
        }
    });
    emit!(&[::concord::__private::Piece::Lit(""), #(#pieces),*])
}

enum PathPiece<'a> {
    Literal(&'a str),
    Hole(&'a Reference),
    Splat(&'a Reference),
}

fn path_segments(
    template: &Template,
    gate: Option<&str>,
    binding: impl Fn(&Reference) -> TokenStream,
) -> TokenStream {
    use PathPiece::{Hole, Literal, Splat};
    let mut segments = Vec::<Vec<PathPiece<'_>>>::new();
    let mut segment = Vec::new();
    let mut trailing = false;
    for piece in &template.pieces {
        match piece {
            TemplatePiece::Text { text, .. } => {
                let mut split = text.split('/').peekable();
                while let Some(text) = split.next() {
                    if !text.is_empty() {
                        segment.push(Literal(text));
                    }
                    if split.peek().is_some() {
                        if !segment.is_empty() {
                            segments.push(std::mem::take(&mut segment));
                        }
                        trailing = true;
                    } else if !text.is_empty() {
                        trailing = false;
                    }
                }
            }
            TemplatePiece::Hole {
                reference, star, ..
            } => {
                if *star {
                    segments.push(vec![Splat(reference)]);
                } else {
                    segment.push(Hole(reference));
                }
                trailing = false;
            }
        }
    }
    if !segment.is_empty() {
        segments.push(segment);
    }
    let mut output = TokenStream::new();
    let mut literals = Vec::new();
    let flush_literals = |output: &mut TokenStream, literals: &mut Vec<&str>| {
        if !literals.is_empty() {
            output.extend(emit!(__draft.literal_path(&[#(#literals),*]);));
            literals.clear();
        }
    };
    for (i, pieces) in segments.iter().enumerate() {
        let trailing = trailing && i + 1 == segments.len();
        if !trailing {
            if let [Literal(text)] = pieces.as_slice() {
                literals.push(*text);
                continue;
            }
        }
        flush_literals(&mut output, &mut literals);
        if let [Splat(r)] = pieces.as_slice() {
            let value = adapted(binding(r), gate, r.span);
            let name = flat::reference_text(r);
            let tail = trailing.then(|| emit!(__draft.trailing_separator();));
            output.extend(emit!(__draft.segments(#name, &#value); #tail));
        } else if let [Hole(r)] = pieces.as_slice() {
            let value = adapted(binding(r), gate, r.span);
            let name = flat::reference_text(r);
            output.extend(emit!(__draft.segment_value(#name, #value, #trailing);));
        } else {
            let values = pieces.iter().map(|piece| match piece {
                Literal(text) => emit!(::concord::__private::Piece::Lit(#text)),
                Hole(r) | Splat(r) => {
                    let value = adapted(binding(r), gate, r.span);
                    let name = flat::reference_text(r);
                    emit!(::concord::__private::Piece::Val(#name, &#value))
                }
            });
            let tail = trailing.then(|| emit!(, ::concord::__private::Piece::Lit("/")));
            output.extend(emit!(__draft.segment(&[#(#values),* #tail]);));
        }
    }
    flush_literals(&mut output, &mut literals);
    output
}

fn same_origin(a: &flat::Origin<'_>, b: &flat::Origin<'_>) -> bool {
    std::ptr::eq(a.written, b.written) && std::ptr::eq(a.placed, b.placed)
}

/// Keep the evaluated values outside each entry's lexical block so they can be
/// written to the request in placement order.
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
        let method = self.request_method(endpoint);
        let literal = endpoint.base.as_ref().and_then(|line| match line.value {
            Base::Template(t) => match t.pieces.as_slice() {
                [TemplatePiece::Text { text, .. }] => Some(text.as_str()),
                _ => None,
            },
            _ => None,
        });
        let (draft, base) = if let Some(base) = literal {
            (
                emit!(::concord::__private::Draft::from_literal(#method, #base)),
                TokenStream::new(),
            )
        } else {
            (
                emit!(::concord::__private::Draft::new(#method)),
                self.request_base(index, endpoint),
            )
        };
        let paths = self.request_paths(index, endpoint);
        let query = self.entries(index, &endpoint.query, "query");
        let headers = self.entries(index, &endpoint.headers, "header");
        let body = self.request_body(index, endpoint);
        emit! {
            fn __build_draft(__value: &Self::Data<()>) -> #root::Result<::concord::__private::Draft> {
                let mut __draft = #draft;
                #base #paths #query #headers #body
                ::core::result::Result::Ok(__draft)
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
                let binding = |r: &Reference| self.placement_reference(index, &line.origin, r);
                match line.value {
                    Base::Template(template) => {
                        if let [TemplatePiece::Text { text: prefix, .. }, TemplatePiece::Hole { reference, .. }, TemplatePiece::Text { text: suffix, .. }] = template.pieces.as_slice() {
                            let name = flat::reference_text(reference);
                            let value = adapted(binding(reference), None, reference.span);
                            return emit!(__draft.base_value(#prefix, #name, #value, #suffix););
                        }
                        let pieces = pieces_with(template, binding);
                        emit!(__draft.base(#pieces);)
                    }
                    Base::Reference(r) => {
                        let value = binding(r);
                        emit_at!(r.span => __draft.base_from(#value);)
                    }
                }
            })
            .unwrap_or_default()
    }

    fn request_paths(&self, index: usize, endpoint: &flat::Endpoint<'_>) -> TokenStream {
        endpoint
            .paths
            .iter()
            .map(|path| {
                path_segments(
                    path.line.value,
                    path.crosses_base.then_some("inherited_path"),
                    |r| self.placement_reference(index, &path.line.origin, r),
                )
            })
            .collect()
    }

    fn placement_reference(
        &self,
        index: usize,
        origin: &flat::Origin<'_>,
        r: &Reference,
    ) -> TokenStream {
        let input = if let Some(owner) = &r.owner {
            origin
                .owners
                .iter()
                .find(|o| *owner == flat::owner_name(o.node))
                .and_then(|o| o.inputs.iter().find(|i| i.input.name == r.name))
        } else {
            origin.locals.iter().find(|i| i.input.name == r.name)
        };
        // Recovery can leave a reference for rustc to diagnose in its lexical block.
        let Some(input) = input else {
            return reference(r);
        };
        let field = self.input_field(index, input, emit!(__value));
        located(self.borrow_field(index, field, false), r.span)
    }

    fn request_body(&self, index: usize, endpoint: &flat::Endpoint<'_>) -> TokenStream {
        match &endpoint.body {
            Some(flat::BodyLine::Expression(line)) => {
                let bindings = self.bindings(&line.origin, index, emit!(__value), false);
                let expression = line.value;
                let place = emit_at!(expression.span() => __draft.body(__body););
                emit!({
                    #bindings
                    let __body = ::concord::__private::evaluate_body(Self::__ENDPOINT, || ::core::result::Result::Ok({ #expression }))?;
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
        for group in entries.chunk_by(|a, b| same_origin(&a.line.origin, &b.line.origin)) {
            let Some(first) = group.first() else {
                continue;
            };
            let bindings = self.bindings_at(
                &first.line.origin,
                index,
                emit!(__value),
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
                    emit!(::concord::__private::evaluate_body(Self::__ENDPOINT, || ::core::result::Result::Ok({ #value }))?)
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
        for group in entries.chunk_by(|a, b| same_origin(&a.line.origin, &b.line.origin)) {
            let Some(first) = group.first() else {
                continue;
            };
            let bindings = if group
                .iter()
                .any(|entry| !matches!(entry.line.value.kind, EntryKind::Short(_)))
            {
                self.bindings_at(
                    &first.line.origin,
                    index,
                    emit!(__value),
                    false,
                    first.line.origin.span,
                )
            } else {
                TokenStream::new()
            };
            let mut values = Values::default();
            let mut holders = TokenStream::new();
            for entry in group {
                let name = ident(&format!("__Entry{next}"));
                let key = flat::key(entry.line.value, false);
                let write = if let Some(write) =
                    self.scalar_entry(index, entry, next, &mut values, &method)
                {
                    write
                } else {
                    holders.extend(self.entry_value(entry, &name, next, &mut values));
                    emit!(__draft.#method(#key, #name);)
                };
                next += 1;
                names.push((entry.position, write));
            }
            if !values.names.is_empty() || !bindings.is_empty() {
                setup.extend(values.block(bindings));
            }
            setup.extend(holders);
        }
        names.sort_by_key(|(position, _)| *position);
        let writes = names.iter().map(|(_, write)| write);
        emit!({ #setup #(#writes)* })
    }

    fn scalar_entry(
        &self,
        current: usize,
        entry: &flat::EntryLine<'_>,
        index: usize,
        values: &mut Values,
        placement: &Ident,
    ) -> Option<TokenStream> {
        let raw = ident(&format!("__EntryExpression{index}"));
        let key = flat::key(entry.line.value, false);
        let (name, saved, span) = match &entry.line.value.kind {
            EntryKind::Short(r) => (
                flat::reference_text(r),
                self.placement_reference(current, &entry.line.origin, r),
                r.span,
            ),
            EntryKind::Value {
                value: Value::Expression(expr),
                ..
            } => {
                values.bind(raw.clone(), emit!(::concord::__private::evaluate_entry(Self::__ENDPOINT, #key, || ::core::result::Result::Ok({ #expr }))?));
                (key.clone(), emit!(&#raw), expr.span())
            }
            _ => return None,
        };
        let value = adapted(saved, entry.crosses_base.then_some("inherited"), span);
        let place = ident(&format!("{placement}_value"));
        Some(emit!(__draft.#place(#key, #name, #value);))
    }

    fn entry_value(
        &self,
        entry: &flat::EntryLine<'_>,
        name: &Ident,
        index: usize,
        values: &mut Values,
    ) -> TokenStream {
        let gate = entry.crosses_base.then_some("inherited");
        match &entry.line.value.kind {
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
            _ => TokenStream::new(),
        }
    }
}
