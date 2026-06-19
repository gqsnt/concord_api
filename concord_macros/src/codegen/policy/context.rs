#[derive(Clone, Copy)]
enum PolicyEmitCtx {
    ClientBase,
    Layer,
    Endpoint,
}

fn emit_policy_apply_fn(policy: &PolicyBlocksResolved, ctx: PolicyEmitCtx) -> TokenStream2 {
    let mut ops = Vec::new();

    ops.push(quote! {
        #[allow(unused_variables)]
        let cx = vars;
        #[allow(unused_variables)]
        let ep = ep;
        #[allow(unused_variables)]
        let auth = auth;
    });

    if policy_uses_auth(policy) {
        // AuthVars is a single RwLock<AuthInner>; lock exactly once per request build.
        ops.push(quote! {
            let auth = ::concord_core::advanced::read_auth_lock(auth, "auth vars lock poisoned")
                .map_err(|source| ::concord_core::prelude::ApiClientError::Auth {
                    ctx: ctx_err.clone(),
                    source,
                })?;
        });
    }
    ops.extend(emit_policy_ops(policy, PolicyKeyKind::Header, ctx));
    ops.extend(emit_policy_ops(policy, PolicyKeyKind::Query, ctx));
    if let Some(t) = &policy.timeout {
        let ex = emit_value_expr(t, ctx);
        ops.push(quote! { policy.set_timeout(#ex); });
    }
    if let Some(cache) = emit_cache_op(&policy.cache) {
        ops.push(cache);
    }
    if let Some(retry) = emit_retry_op(&policy.retry) {
        ops.push(retry);
    }
    if let Some(rate_limit) = emit_rate_limit_op(&policy.rate_limit, ctx) {
        ops.push(rate_limit);
    }
    quote! { #( #ops )* }
}



