fn emit_policy_ops(
    policy: &PolicyBlocksResolved,
    kind: PolicyKeyKind,
    ctx: PolicyEmitCtx,
) -> Vec<TokenStream2> {
    let ops = match kind {
        PolicyKeyKind::Header => &policy.headers,
        PolicyKeyKind::Query => &policy.query,
    };

    ops.iter()
        .map(|op| match op {
            PolicyOp::Remove { key } => emit_remove_op(key, kind, ctx),
            PolicyOp::Set { key, value, op } => emit_set_op(key, kind, value, *op, ctx),
        })
        .collect()
}

fn emit_key_string(key: &KeyResolved, kind: PolicyKeyKind) -> (String, Span, TokenStream2) {
    match key {
        KeyResolved::Static(l) => (l.value(), l.span(), quote! { #l }),
        KeyResolved::Ident(id) => {
            let s = match kind {
                PolicyKeyKind::Header => emit_helpers::to_kebab(id),
                PolicyKeyKind::Query => id.to_string(),
            };
            let lit = emit_helpers::lit_str(&s, id.span());
            (s, id.span(), quote! { #lit })
        }
    }
}

fn emit_remove_op(key: &KeyResolved, kind: PolicyKeyKind, _ctx: PolicyEmitCtx) -> TokenStream2 {
    match kind {
        PolicyKeyKind::Header => {
            let (ks, sp, _) = emit_key_string(key, kind);
            let name = emit_helpers::emit_header_name(&ks, sp);
            quote! {
                policy.remove_header(#name);
            }
        }
        PolicyKeyKind::Query => {
            let (ks, sp, _) = emit_key_string(key, kind);
            let lit = emit_helpers::lit_str(&ks, sp);
            quote! {
                policy.remove_query(#lit);
            }
        }
    }
}

fn emit_set_op(
    key: &KeyResolved,
    kind: PolicyKeyKind,
    value: &PolicySetValue,
    op: SetOp,
    ctx: PolicyEmitCtx,
) -> TokenStream2 {
    match kind {
        PolicyKeyKind::Header => emit_header_set_op(key, value, ctx),
        PolicyKeyKind::Query => emit_query_set_op(key, value, op, ctx),
    }
}

fn emit_header_set_op(
    key: &KeyResolved,
    value: &PolicySetValue,
    ctx: PolicyEmitCtx,
) -> TokenStream2 {
    let (ks, sp, _) = emit_key_string(key, PolicyKeyKind::Header);
    let name = emit_helpers::emit_header_name(&ks, sp);

    match value {
        PolicySetValue::OptionalCxField(field) => {
            let as_ref_expr = match ctx {
                PolicyEmitCtx::ClientBase => quote! { vars.#field.as_ref() },
                _ => quote! { vars.#field.as_ref() },
            };
            emit_optional_header_set(name, ks, as_ref_expr)
        }
        PolicySetValue::OptionalEpField(field) => {
            emit_optional_header_set(name, ks, quote! { ep.#field.as_ref() })
        }
        PolicySetValue::Value(PublicValueKind::Fmt(fmt)) => {
            emit_fmt_header_set(fmt, name, ks, sp)
        }
        PolicySetValue::Value(PublicValueKind::LitStr(value)) => {
            let hv = emit_helpers::emit_header_value_from_static(value);
            quote! {
                policy.insert_header(#name, #hv);
            }
        }
        PolicySetValue::Value(value) => {
            let ex = emit_value_expr(value, ctx);
            let err = syn::LitStr::new(&format!("header:{ks}"), sp);
            quote! {
                {
                    let __hv = ::http::HeaderValue::from_str(&(#ex).to_string())
                        .map_err(|_| ::concord_core::prelude::ApiClientError::InvalidParam {
                            ctx: ctx.clone(),
                            param: #err.into(),
                        })?;
                    policy.insert_header(#name, __hv);
                }
            }
        }
    }
}

fn emit_optional_header_set(name: TokenStream2, key: String, as_ref_expr: TokenStream2) -> TokenStream2 {
    quote! {
        if let ::core::option::Option::Some(__v) = #as_ref_expr {
            let __hv = ::http::HeaderValue::from_str(&__v.to_string())
                .map_err(|_| ::concord_core::prelude::ApiClientError::InvalidParam {
                    ctx: ctx.clone(),
                    param: concat!("header:", #key).into(),
                })?;
            policy.insert_header(#name, __hv);
        } else {
            policy.remove_header(#name);
        }
    }
}

fn emit_fmt_header_set(
    fmt: &FmtResolved,
    name: TokenStream2,
    key: String,
    span: Span,
) -> TokenStream2 {
    let err = syn::LitStr::new(&format!("header:{key}"), span);
    let build = emit_fmt_build_string(fmt);
    let insert = quote! {
        let __hv = ::http::HeaderValue::from_str(&__fmt_s)
            .map_err(|_| ::concord_core::prelude::ApiClientError::InvalidParam { ctx: ctx.clone(), param: #err.into() })?;
        policy.insert_header(#name, __hv);
    };

    if fmt.require_all {
        let checks = fmt.pieces.iter().filter_map(|piece| {
            let FmtResolvedPiece::Var {
                source,
                field,
                optional: true,
            } = piece
            else {
                return None;
            };
            match source {
                FmtVarSource::Cx => {
                    Some(quote! { if vars.#field.is_none() { __fmt_ok = false; } })
                }
                FmtVarSource::Ep => Some(quote! { if ep.#field.is_none() { __fmt_ok = false; } }),
            }
        });
        quote! {
            let mut __fmt_ok: bool = true;
            #( #checks )*
            if __fmt_ok {
                let __fmt_s: ::std::string::String = { #build };
                #insert
            } else {
                policy.remove_header(#name);
            }
        }
    } else {
        quote! {
            let __fmt_s: ::std::string::String = { #build };
            #insert
        }
    }
}

fn emit_query_set_op(
    key: &KeyResolved,
    value: &PolicySetValue,
    op: SetOp,
    ctx: PolicyEmitCtx,
) -> TokenStream2 {
    let (ks, sp, _) = emit_key_string(key, PolicyKeyKind::Query);
    let lit = emit_helpers::lit_str(&ks, sp);

    match value {
        PolicySetValue::OptionalCxField(field) => {
            let as_ref_expr = match ctx {
                PolicyEmitCtx::ClientBase => quote! { vars.#field.as_ref() },
                _ => quote! { vars.#field.as_ref() },
            };
            emit_optional_query_set(lit, as_ref_expr, op)
        }
        PolicySetValue::OptionalEpField(field) => {
            emit_optional_query_set(lit, quote! { ep.#field.as_ref() }, op)
        }
        PolicySetValue::Value(PublicValueKind::Fmt(fmt)) => emit_fmt_query_set(fmt, lit, op),
        PolicySetValue::Value(value) => {
            let ex = emit_value_expr(value, ctx);
            match op {
                SetOp::Set => quote! { policy.set_query(#lit, (#ex).to_string()); },
                SetOp::Push => quote! { policy.push_query(#lit, (#ex).to_string()); },
            }
        }
    }
}

fn emit_optional_query_set(lit: syn::LitStr, as_ref_expr: TokenStream2, op: SetOp) -> TokenStream2 {
    let setter = match op {
        SetOp::Set => quote! { policy.set_query(#lit, __v.to_string()); },
        SetOp::Push => quote! { policy.push_query(#lit, __v.to_string()); },
    };
    quote! {
        if let ::core::option::Option::Some(__v) = #as_ref_expr {
            #setter
        } else {
            policy.remove_query(#lit);
        }
    }
}

fn emit_fmt_query_set(fmt: &FmtResolved, lit: syn::LitStr, op: SetOp) -> TokenStream2 {
    let build = emit_fmt_build_string(fmt);
    let setter = match op {
        SetOp::Set => quote! { policy.set_query(#lit, __fmt_s); },
        SetOp::Push => quote! { policy.push_query(#lit, __fmt_s); },
    };

    if fmt.require_all {
        let checks = fmt.pieces.iter().filter_map(|piece| {
            let FmtResolvedPiece::Var {
                source,
                field,
                optional: true,
            } = piece
            else {
                return None;
            };
            match source {
                FmtVarSource::Cx => {
                    Some(quote! { if vars.#field.is_none() { __fmt_ok = false; } })
                }
                FmtVarSource::Ep => Some(quote! { if ep.#field.is_none() { __fmt_ok = false; } }),
            }
        });
        quote! {
            let mut __fmt_ok: bool = true;
            #( #checks )*
            if __fmt_ok {
                let __fmt_s: ::std::string::String = { #build };
                #setter
            } else {
                policy.remove_query(#lit);
            }
        }
    } else {
        quote! {
            let __fmt_s: ::std::string::String = { #build };
            #setter
        }
    }
}
