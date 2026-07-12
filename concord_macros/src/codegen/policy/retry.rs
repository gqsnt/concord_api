fn emit_retry_op(retry: &Option<RetryResolved>) -> Option<TokenStream2> {
    let retry = retry.as_ref()?;
    Some(match retry {
        RetryResolved::Clear => quote! {
            policy.clear_retry();
        },
        RetryResolved::Set(config) => {
            let config = emit_retry_config(config);
            quote! {
                policy.set_retry(#config);
            }
        }
    })
}

fn emit_retry_config(config: &RetryConfigResolved) -> TokenStream2 {
    let max_attempts = config.max_attempts;
    let methods = config
        .methods
        .iter()
        .map(|method| quote! { ::http::Method::#method });
    let statuses = config.statuses.iter().map(|status| {
        quote! {
            ::http::StatusCode::from_u16(#status).map_err(|_| {
                ::concord_core::prelude::ApiClientError::PolicyViolation {
                    ctx: ctx.clone(),
                    msg: "validated retry status was invalid",
                }
            })?
        }
    });
    let transport_errors = config.transport_errors.iter().map(|kind| {
        quote! { ::concord_core::transport::TransportErrorKind::#kind }
    });
    let respect_retry_after = config.respect_retry_after;
    let idempotency = emit_retry_idempotency(&config.idempotency);

    quote! {
        ::concord_core::advanced::RetryConfig {
            max_attempts: #max_attempts,
            methods: ::std::vec![ #( #methods ),* ],
            statuses: ::std::vec![ #( #statuses ),* ],
            transport_errors: ::std::vec![ #( #transport_errors ),* ],
            respect_retry_after: #respect_retry_after,
            idempotency: #idempotency,
        }
    }
}

fn emit_retry_idempotency(idempotency: &RetryIdempotencyResolved) -> TokenStream2 {
    match idempotency {
        RetryIdempotencyResolved::SafeMethodsOnly => {
            quote! { ::concord_core::advanced::RetryIdempotency::SafeMethodsOnly }
        }
        RetryIdempotencyResolved::Header(header) => {
            let name = emit_helpers::emit_header_name(&header.value(), header.span());
            quote! { ::concord_core::advanced::RetryIdempotency::Header(#name) }
        }
    }
}



