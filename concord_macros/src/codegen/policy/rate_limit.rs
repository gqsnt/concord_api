fn emit_rate_limit_op(
    rate_limit: &Option<RateLimitResolved>,
    ctx: PolicyEmitCtx,
) -> Option<TokenStream2> {
    let rate_limit = rate_limit.as_ref()?;
    Some(match rate_limit {
        RateLimitResolved::Clear => quote! {
            policy.clear_rate_limit();
        },
        RateLimitResolved::Add(plan) => {
            let plan = emit_rate_limit_plan(plan, ctx);
            quote! {
                policy.add_rate_limit(#plan);
            }
        }
        RateLimitResolved::Replace(plan) => {
            let plan = emit_rate_limit_plan(plan, ctx);
            quote! {
                policy.replace_rate_limit(#plan);
            }
        }
    })
}

fn emit_rate_limit_plan(plan: &RateLimitPlanResolved, ctx: PolicyEmitCtx) -> TokenStream2 {
    let buckets = plan.buckets.iter().map(|bucket| {
        let kind = LitStr::new(&bucket.kind, Span::call_site());
        let name = LitStr::new(&bucket.name, Span::call_site());
        let key = emit_rate_limit_key(&bucket.key, ctx);
        let cost = bucket.cost;
        let windows = bucket.windows.iter().map(|window| {
            let max = window.max;
            let per_secs = window.per_secs;
            quote! {
                ::concord_core::advanced::RateLimitWindow::new(
                    ::std::num::NonZeroU32::new(#max).ok_or_else(|| {
                        ::concord_core::prelude::ApiClientError::PolicyViolation {
                            ctx: ctx.clone(),
                            msg: "validated rate-limit max was zero",
                        }
                    })?,
                    ::std::time::Duration::from_secs(#per_secs),
                )
            }
        });
        quote! {
            ::concord_core::advanced::RateLimitBucketUse::new(#kind, #name, #key)
                .with_cost(::std::num::NonZeroU32::new(#cost).ok_or_else(|| {
                    ::concord_core::prelude::ApiClientError::PolicyViolation {
                        ctx: ctx.clone(),
                        msg: "validated rate-limit cost was zero",
                    }
                })?)
                .with_windows(::std::vec![ #( #windows ),* ])
        }
    });
    quote! {
        ::concord_core::advanced::RateLimitPlan::from_buckets(::std::vec![ #( #buckets ),* ])
    }
}

fn emit_rate_limit_key(keys: &[RateLimitKeyResolved], ctx: PolicyEmitCtx) -> TokenStream2 {
    let parts = keys.iter().map(|key| match key {
        RateLimitKeyResolved::RouteHost => {
            quote! { ::concord_core::advanced::RateLimitKeyPart::url_host() }
        }
        RateLimitKeyResolved::Endpoint => {
            quote! { ::concord_core::advanced::RateLimitKeyPart::endpoint() }
        }
        RateLimitKeyResolved::Method => {
            quote! { ::concord_core::advanced::RateLimitKeyPart::method() }
        }
        RateLimitKeyResolved::EpField { name, field } => {
            let name = LitStr::new(name, field.span());
            match ctx {
                PolicyEmitCtx::ClientBase => crate::emit_helpers::compile_error_tokens(
                    "internal Concord error: endpoint/scope rate_limit key reached client base codegen",
                    field.span(),
                ),
                PolicyEmitCtx::Layer | PolicyEmitCtx::Endpoint => quote! {
                    ::concord_core::advanced::RateLimitKeyPart::static_value(
                        #name,
                        ::std::string::ToString::to_string(&ep.#field),
                    )
                },
            }
        }
        RateLimitKeyResolved::Static { name, value } => {
            let name = LitStr::new(name, Span::call_site());
            let value = LitStr::new(value, Span::call_site());
            quote! {
                ::concord_core::advanced::RateLimitKeyPart::static_value(#name, #value)
            }
        }
    });
    quote! {
        ::concord_core::advanced::RateLimitKey::new(::std::vec![ #( #parts ),* ])
    }
}

#[cfg(test)]
mod rate_limit_codegen_tests {
    use super::*;
    use quote::quote;
    use syn::parse_quote;

    #[test]
    fn client_base_ep_field_key_emits_compile_error_tokens() {
        let tokens = emit_rate_limit_key(
            &[RateLimitKeyResolved::EpField {
                name: "endpoint_id".to_string(),
                field: parse_quote!(endpoint_id),
            }],
            PolicyEmitCtx::ClientBase,
        );

        let rendered = tokens.to_string();
        assert!(rendered.contains("compile_error"));
        assert!(rendered.contains("internal Concord error"));
        assert!(!rendered.contains("unreachable"));
        let _ = quote!(#tokens);
    }
}




