//! Status dispatch and lexical environments of readers and arms.
use super::*;

impl Writer<'_> {
    fn reader(
        &self,
        index: usize,
        line: &flat::Line<'_, syn::Expr>,
        ty: TokenStream,
        check: &Ident,
    ) -> TokenStream {
        let bindings = self.bindings_at(
            &line.origin,
            index,
            emit!(__value),
            false,
            line.value.span(),
        );
        let expression = line.value;
        let value = emit_at!(expression.span()=> (|| { #expression })());
        emit_at!(expression.span()=> {
            #bindings ::concord::__private::#check::<#ty, _>(#value)
        })
    }

    pub(super) fn answer(
        &self,
        index: usize,
        endpoint: &flat::Endpoint<'_>,
        read_type: &TokenStream,
    ) -> TokenStream {
        let root = &self.root;
        let output = emit!(Self::Value);
        let read_answer = Ident::new(
            "__read_answer",
            Span::mixed_site().located_at(endpoint.node.name.span()),
        );
        let receiver = emit!(__value: &Self::Data<()>);
        if endpoint.arms.is_empty() {
            let body = self.shared_answer(index, endpoint, read_type);
            let reader = if endpoint.node.output.is_some() {
                endpoint.reader.as_ref()
            } else {
                endpoint.problem_reader.as_ref()
            };
            let span = reader.map_or(endpoint.node.name.span(), |line| line.value.span());
            let future = emit_at!(span=>
                impl ::core::future::Future<Output = #root::Result<::concord::Answer<#output>>> + ::core::marker::Send
            );
            return emit_at!(endpoint.node.name.span()=>
                fn #read_answer(#receiver, __response: ::concord::reqwest::Response, __info: ::concord::transport::TryInfo)
                    -> #future
                { #body }
            );
        }
        let arms = endpoint
            .arms
            .iter()
            .map(|line| self.answer_arm(index, endpoint, line, &output));
        let simple = self.answer_simple(index, endpoint, read_type);
        let refused = self.answer_refused(index, endpoint);
        let body = emit_at!(endpoint.node.name.span()=>
            let (__url, __status, __headers) = ::concord::__private::answer_head(&__response);
            let __value = match __status {
                #(#arms,)* #simple
                _ => return ::core::result::Result::Err(#refused),
            };
            ::concord::__private::frame_answer(__url, __status, __headers, __value)
        );
        emit_at!(endpoint.node.name.span()=>
            async fn #read_answer(#receiver, __response: ::concord::reqwest::Response, __info: ::concord::transport::TryInfo)
                -> #root::Result<::concord::Answer<#output>>
            { #body }
        )
    }

    fn shared_answer(
        &self,
        index: usize,
        endpoint: &flat::Endpoint<'_>,
        read_type: &TokenStream,
    ) -> TokenStream {
        let refusal = match (
            endpoint.problem.as_ref(),
            endpoint.problem_reader.as_ref(),
            self.problem_type(),
        ) {
            (None, _, _) => emit!(::concord::__private::PlainRefusal),
            (Some(_), Some(line), Some(_)) => {
                let reader = self.reader(
                    index,
                    line,
                    self.problem_alias().to_token_stream(),
                    &ident("reader_for_problem"),
                );
                emit_at!(self.problem_span()=> ::concord::__private::ProblemRefusal(move || #reader))
            }
            _ => {
                self.internal(
                    endpoint.node.name.span(),
                    "a problem has no resolved reader",
                );
                return TokenStream::new();
            }
        };
        if endpoint.node.output.is_none() {
            let span = self
                .problem_type()
                .map_or_else(Span::mixed_site, Spanned::span);
            return emit_at!(span=> ::concord::__private::answer_empty(__response, __info, #refusal));
        }
        let Some(line) = endpoint
            .reader
            .as_ref()
            .filter(|_| endpoint.read_type.is_some())
        else {
            self.internal(
                endpoint.node.name.span(),
                "an output has no resolved success reader",
            );
            return TokenStream::new();
        };
        let reader = self.reader(
            index,
            line,
            read_type.clone(),
            &ident("reader_for_response"),
        );
        let wrap = match endpoint.wrapper {
            Some(flat::Wrapper::Option) => emit!(::core::option::Option::Some),
            Some(flat::Wrapper::Result) => emit!(::core::result::Result::Ok),
            None => emit!(::core::convert::identity),
        };
        emit_at!(line.value.span()=>
            ::concord::__private::answer_read(__response, __info, move || #reader, #wrap, #refusal)
        )
    }

    fn answer_arm(
        &self,
        index: usize,
        endpoint: &flat::Endpoint<'_>,
        line: &flat::Line<'_, Arm>,
        output: &TokenStream,
    ) -> TokenStream {
        let Some((code, class)) = flat::status(&line.value.key) else {
            self.internal(line.value.span, "a status arm has an unchecked status key");
            return TokenStream::new();
        };
        let Some(expression) = &line.value.expression else {
            self.internal(line.value.span, "a status arm has no expression");
            return TokenStream::new();
        };
        let test = if class {
            emit!(::concord::http::StatusCode::as_u16(&__status) / 100 == #code / 100)
        } else {
            emit!(::concord::http::StatusCode::as_u16(&__status) == #code)
        };
        let bindings = self.bindings_at(
            &line.origin,
            index,
            emit!(__value),
            false,
            expression.span(),
        );
        let problem = self.problem_alias().to_token_stream();
        let body = flat::body_field(line.value).and(endpoint.reader.as_ref()).map(|reader| {
            let inferred = emit_at!(expression.span()=> _);
            let reader_value = self.reader(index, reader, inferred.clone(), &ident("reader_for_response"));
            emit_at!(reader.value.span()=>
                let __body = ::concord::__private::read_body::<#inferred, #problem, _>(&#reader_value, __response, &__info).await?;
            )
        });
        let mut parameters = Vec::new();
        let mut args = Vec::new();
        if let Some(fields) = &line.value.fields {
            for field in &fields.fields {
                let name = field.binding.as_ref().unwrap_or(&field.name);
                match field.name.to_string().as_str() {
                    "body" => {
                        parameters.push(emit!(#name));
                        args.push(emit!(__body));
                    }
                    "headers" => {
                        parameters.push(emit!(#name: &::concord::Headers));
                        args.push(emit!(&__headers));
                    }
                    "status" => {
                        parameters.push(emit!(#name: ::concord::http::StatusCode));
                        args.push(emit!(__status));
                    }
                    _ => self.internal(field.name.span(), "a status arm has an unchecked field"),
                }
            }
        }
        // As for the owners' bindings: a constant of the author's with a field's name
        // cannot take the closure's parameter.
        let shadows = line
            .value
            .fields
            .iter()
            .flat_map(|fields| &fields.fields)
            .map(|field| {
                let name = field.binding.as_ref().unwrap_or(&field.name);
                emit!(#[allow(unused_imports)] use ::core::mem::drop as #name;)
            });
        let arm = emit_at!(expression.span()=>
            #(#shadows)*
            let __arm = |#(#parameters),*| -> ::core::result::Result<#output, ::concord::Fail<#problem>> {
                ::core::result::Result::Ok({ #expression })
            };
        );
        let call = emit_at!(expression.span()=>
            ::concord::__private::arm(__arm(#(#args),*), __status, &__headers, &__info)?
        );
        emit!(_ if #test => { #body #bindings #arm #call })
    }

    fn answer_simple(
        &self,
        index: usize,
        endpoint: &flat::Endpoint<'_>,
        read_type: &TokenStream,
    ) -> TokenStream {
        if endpoint
            .arms
            .iter()
            .any(|a| flat::status(&a.value.key) == Some((200, true)))
        {
            return TokenStream::new();
        }
        let head = matches!(&endpoint.node.kind, NodeKind::Endpoint(m) if method(m) == "HEAD");
        let success = if endpoint.node.output.is_none() {
            TokenStream::new()
        } else if head {
            emit!(
                return ::core::result::Result::Err(
                    ::concord::__private::no_arm(__response, &__info).await
                )
            )
        } else if let Some(line) = endpoint
            .reader
            .as_ref()
            .filter(|_| endpoint.read_type.is_some())
        {
            let reader = self.reader(
                index,
                line,
                read_type.clone(),
                &ident("reader_for_response"),
            );
            let problem = self.problem_alias().to_token_stream();
            let read = emit_at!(line.value.span()=>
                ::concord::__private::read_body::<#read_type, #problem, _>(&#reader, __response, &__info).await?
            );
            match endpoint.wrapper {
                Some(flat::Wrapper::Option) => emit!(::core::option::Option::Some(#read)),
                Some(flat::Wrapper::Result) => emit!(::core::result::Result::Ok(#read)),
                None => read,
            }
        } else {
            self.internal(
                endpoint.node.name.span(),
                "an output has no resolved success reader",
            );
            return TokenStream::new();
        };
        emit!(_ if ::concord::http::StatusCode::is_success(&__status) => { #success },)
    }

    fn answer_refused(&self, index: usize, endpoint: &flat::Endpoint<'_>) -> TokenStream {
        if endpoint.problem.is_none() {
            return emit!(::concord::__private::refused_plain(__response, &__info).await);
        }
        let Some(line) = &endpoint.problem_reader else {
            self.internal(
                endpoint.node.name.span(),
                "a problem has no resolved reader",
            );
            return TokenStream::new();
        };
        if !self.has_problem() {
            self.internal(
                endpoint.node.name.span(),
                "an endpoint problem has no client problem type",
            );
            return TokenStream::new();
        }
        let problem = self.problem_alias().to_token_stream();
        let reader = self.reader(index, line, problem.clone(), &ident("reader_for_problem"));
        emit_at!(line.value.span()=> ::concord::__private::refused::<#problem, _>(&#reader, __response, &__info).await)
    }
}
