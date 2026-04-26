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
        RetryResolved::Patch(patch) => {
            let ops = emit_retry_patch_ops(patch);
            quote! {
                let mut __retry = policy.retry().cloned().unwrap_or_default();
                #( #ops )*
                policy.set_retry(__retry);
            }
        }
    })
}

fn emit_retry_config(config: &RetryConfigResolved) -> TokenStream2 {
    let attempts = config.attempts;
    let methods = config
        .methods
        .iter()
        .map(|method| quote! { ::http::Method::#method });
    let statuses = config.statuses.iter().map(
        |status| quote! { ::http::StatusCode::from_u16(#status).expect("valid retry status") },
    );
    let transport_errors = config.transport_errors.iter().map(|kind| {
        quote! { ::concord_core::transport::TransportErrorKind::#kind }
    });
    let backoff = emit_retry_backoff(&config.backoff);
    let respect_retry_after = config.respect_retry_after;
    let idempotency = emit_retry_idempotency(&config.idempotency);

    quote! {
        ::concord_core::prelude::RetryConfig {
            attempts: #attempts,
            methods: ::std::vec![ #( #methods ),* ],
            statuses: ::std::vec![ #( #statuses ),* ],
            transport_errors: ::std::vec![ #( #transport_errors ),* ],
            backoff: #backoff,
            respect_retry_after: #respect_retry_after,
            idempotency: #idempotency,
        }
    }
}

fn emit_retry_patch_ops(patch: &RetryPatchResolved) -> Vec<TokenStream2> {
    let mut ops = Vec::new();

    if let Some(attempts) = patch.attempts {
        ops.push(quote! { __retry.attempts = #attempts; });
    }
    if let Some(methods) = &patch.methods {
        let methods = methods
            .iter()
            .map(|method| quote! { ::http::Method::#method });
        ops.push(quote! { __retry.methods = ::std::vec![ #( #methods ),* ]; });
    }
    if let Some(statuses) = &patch.statuses {
        let statuses = statuses.iter().map(
            |status| quote! { ::http::StatusCode::from_u16(#status).expect("valid retry status") },
        );
        ops.push(quote! { __retry.statuses = ::std::vec![ #( #statuses ),* ]; });
    }
    if let Some(transport_errors) = &patch.transport_errors {
        let transport_errors = transport_errors.iter().map(|kind| {
            quote! { ::concord_core::transport::TransportErrorKind::#kind }
        });
        ops.push(quote! { __retry.transport_errors = ::std::vec![ #( #transport_errors ),* ]; });
    }
    if let Some(respect_retry_after) = patch.respect_retry_after {
        ops.push(quote! { __retry.respect_retry_after = #respect_retry_after; });
    }
    if let Some(idempotency) = &patch.idempotency {
        let idempotency = emit_retry_idempotency(idempotency);
        ops.push(quote! { __retry.idempotency = #idempotency; });
    }

    ops
}

fn emit_retry_backoff(backoff: &RetryBackoffResolved) -> TokenStream2 {
    match backoff {
        RetryBackoffResolved::None => quote! { ::concord_core::prelude::RetryBackoff::None },
    }
}

fn emit_retry_idempotency(idempotency: &RetryIdempotencyResolved) -> TokenStream2 {
    match idempotency {
        RetryIdempotencyResolved::SafeMethodsOnly => {
            quote! { ::concord_core::prelude::RetryIdempotency::SafeMethodsOnly }
        }
        RetryIdempotencyResolved::Header(header) => {
            let name = emit_helpers::emit_header_name(&header.value(), header.span());
            quote! { ::concord_core::prelude::RetryIdempotency::Header(#name) }
        }
    }
}

