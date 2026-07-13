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
        let ep = &ep;
    });

    ops.extend(emit_policy_ops(policy, PolicyKeyKind::Header, ctx));
    ops.extend(emit_policy_ops(policy, PolicyKeyKind::Query, ctx));
    if let Some(t) = &policy.timeout {
        let ex = emit_value_expr(t, ctx);
        ops.push(quote! { policy.set_timeout(#ex); });
    }
    if let Some(rate_limit) = emit_rate_limit_op(&policy.rate_limit, ctx) {
        ops.push(rate_limit);
    }
    quote! { #( #ops )* }
}



