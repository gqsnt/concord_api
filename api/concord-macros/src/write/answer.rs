//! Status dispatch and lexical environments of readers and arms.
use super::*;

impl Writer<'_> {
    fn reader(
        &self,
        index: usize,
        line: &flat::Line<'_, syn::Expr>,
        ty: TokenStream,
    ) -> TokenStream {
        let bindings = self.bindings_at(&line.origin, index, emit!(self), false, line.value.span());
        let expression = line.value;
        let value = emit_at!(expression.span()=> (|| { #expression })());
        let checked = emit_at!(expression.span()=> __reader::<#ty, _>(#value));
        emit_at!(expression.span()=> {
            // The opaque return prevents a second error at read_body/refused.
            fn __reader<__T, __R: ::concord::read::Reader<__T>>(__value: __R) -> impl ::concord::read::Reader<__T> {
                ::concord::__private::reader::<__T, __R>(__value)
            }
            #bindings #checked
        })
    }

    pub(super) fn answer(&self, index: usize, endpoint: &flat::Endpoint<'_>) -> TokenStream {
        let root = &self.root;
        let output = endpoint
            .node
            .output
            .as_ref()
            .map(|o| o.ty.to_token_stream())
            .unwrap_or_else(|| emit!(()));
        let arms = endpoint
            .arms
            .iter()
            .map(|line| self.answer_arm(index, endpoint, line, &output));
        let simple = self.answer_simple(index, endpoint);
        let refused = self.answer_refused(index, endpoint);
        let read_answer = Ident::new(
            "__read_answer",
            Span::mixed_site().located_at(endpoint.node.name.span()),
        );
        emit_at!(endpoint.node.name.span()=>
            async fn #read_answer(&self, __response: ::concord::reqwest::Response, __info: ::concord::transport::TryInfo)
                -> #root::Result<::concord::Answer<#output>>
            {
                let __url = __response.url().clone();
                let __status = __response.status();
                let __headers = ::concord::Headers::from(__response.headers().clone());
                let __value = match __status {
                    #(#arms,)* #simple
                    _ => return ::core::result::Result::Err(#refused),
                };
                ::core::result::Result::Ok(
                    ::concord::Answer::new(__status, __headers, __value).with_response_url(__url)
                )
            }
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
            self.internal(line.value.span, "an answer has an unchecked status key");
            return TokenStream::new();
        };
        let Some(expression) = &line.value.expression else {
            self.internal(line.value.span, "an answer arm has no expression");
            return TokenStream::new();
        };
        let test = if class {
            emit!(__status.as_u16() / 100 == #code / 100)
        } else {
            emit!(__status.as_u16() == #code)
        };
        let bindings = self.bindings_at(&line.origin, index, emit!(self), false, expression.span());
        let problem = self
            .problem_type()
            .map(|p| emit!(#p))
            .unwrap_or_else(|| emit!(()));
        let body = flat::body_field(line.value).and(endpoint.reader.as_ref()).map(|reader| {
            let inferred = emit_at!(expression.span()=> _);
            let reader_value = self.reader(index, reader, inferred.clone());
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
                    _ => self.internal(
                        field.name.span(),
                        "an answer has an unchecked pattern field",
                    ),
                }
            }
        }
        let arm = emit_at!(expression.span()=>
            let __arm = |#(#parameters),*| -> ::core::result::Result<#output, ::concord::Fail<#problem>> {
                ::core::result::Result::Ok({ #expression })
            };
        );
        let call = emit_at!(expression.span()=>
            ::concord::__private::arm(__arm(#(#args),*), __status, &__headers, &__info)?
        );
        emit!(_ if #test => { #body #bindings #arm #call })
    }

    fn answer_simple(&self, index: usize, endpoint: &flat::Endpoint<'_>) -> TokenStream {
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
        } else if let Some((line, ty)) = endpoint.reader.as_ref().zip(endpoint.read_type) {
            let reader = self.reader(index, line, emit!(#ty));
            let problem = self
                .problem_type()
                .map(|p| emit!(#p))
                .unwrap_or_else(|| emit!(()));
            let read = emit_at!(line.value.span()=>
                ::concord::__private::read_body::<#ty, #problem, _>(&#reader, __response, &__info).await?
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
        emit!(_ if __status.is_success() => { #success },)
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
        let Some(problem) = self.problem_type() else {
            self.internal(
                endpoint.node.name.span(),
                "an endpoint problem has no client problem type",
            );
            return TokenStream::new();
        };
        let reader = self.reader(index, line, emit!(#problem));
        emit_at!(line.value.span()=> ::concord::__private::refused::<#problem, _>(&#reader, __response, &__info).await)
    }
}
