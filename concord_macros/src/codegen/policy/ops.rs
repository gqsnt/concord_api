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
            PolicyOp::Set {
                key,
                value,
                op,
                conditional_on_optional_ref,
            } => emit_set_op(key, kind, value, *op, *conditional_on_optional_ref, ctx),
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
    value: &ValueKind,
    op: SetOp,
    conditional: Option<OptionalRefKind>,
    ctx: PolicyEmitCtx,
) -> TokenStream2 {
    match kind {
        PolicyKeyKind::Header => {
            let (ks, sp, _) = emit_key_string(key, kind);
            let name = emit_helpers::emit_header_name(&ks, sp);
            if let ValueKind::Fmt(fmt) = value {
                let err = syn::LitStr::new(&format!("header:{ks}"), sp);
                let build = emit_fmt_build_string(fmt);
                let insert = quote! {
                    let __hv = ::http::HeaderValue::from_str(&__fmt_s)
                        .map_err(|_| ::concord_core::prelude::ApiClientError::InvalidParam { ctx: ctx.clone(), param: #err.into() })?;
                    policy.insert_header(#name, __hv);
                };

                if fmt.require_all {
                    let checks = fmt.pieces.iter().filter_map(|p| {
                        let FmtResolvedPiece::Var {
                            source,
                            field,
                            optional: true,
                        } = p
                        else {
                            return None;
                        };
                        match source {
                            FmtVarSource::Cx => {
                                Some(quote! { if vars.#field.is_none() { __fmt_ok = false; } })
                            }
                            FmtVarSource::Ep => {
                                Some(quote! { if ep.#field.is_none() { __fmt_ok = false; } })
                            }
                            FmtVarSource::Auth => {
                                Some(quote! { if auth.#field.is_none() { __fmt_ok = false; } })
                            }
                        }
                    });
                    return quote! {
                        let mut __fmt_ok: bool = true;
                        #( #checks )*
                        if __fmt_ok {
                            let __fmt_s: ::std::string::String = { #build };
                            #insert
                        } else {
                            policy.remove_header(#name);
                        }
                    };
                } else {
                    return quote! {
                        let __fmt_s: ::std::string::String = { #build };
                        #insert
                    };
                }
            }
            // auth direct (non-fmt)
            if let ValueKind::AuthField(fld) = value {
                let err = syn::LitStr::new(&format!("header:{ks}"), sp);
                return if let Some(OptionalRefKind::Auth) = conditional {
                    quote! {
                        {
                            if let ::core::option::Option::Some(__v) = auth.#fld.as_ref() {
                                let __hv = ::http::HeaderValue::from_str(__v.expose())
                                    .map_err(|_| ::concord_core::prelude::ApiClientError::InvalidParam { ctx: ctx.clone(), param: #err.into() })?;
                                policy.insert_header(#name, __hv);
                            } else {
                                policy.remove_header(#name);
                            }
                        }
                    }
                } else {
                    quote! {
                        {
                            let __hv = ::http::HeaderValue::from_str(auth.#fld.expose())
                               .map_err(|_| ::concord_core::prelude::ApiClientError::InvalidParam { ctx: ctx.clone(), param: #err.into() })?;
                            policy.insert_header(#name, __hv);
                        }
                    }
                };
            }
            // conditional optional ref => if Some set else remove
            if let Some(_ref_kind) = conditional {
                let as_ref_expr = match value {
                    ValueKind::CxField(f) => match ctx {
                        PolicyEmitCtx::ClientBase => quote! { vars.#f.as_ref() },
                        _ => quote! { vars.#f.as_ref() },
                    },
                    ValueKind::EpField(f) => quote! { ep.#f.as_ref() },
                    _ => unreachable!(),
                };
                return quote! {
                    if let ::core::option::Option::Some(__v) = #as_ref_expr {
                        let __hv = ::http::HeaderValue::from_str(&__v.to_string())
                            .map_err(|_| ::concord_core::prelude::ApiClientError::InvalidParam {
                                ctx: ctx.clone(),
                                param: concat!("header:", #ks).into(),
                            })?;
                        policy.insert_header(#name, __hv);
                    } else {
                        policy.remove_header(#name);
                    }
                };
            }

            let hv = match value {
                ValueKind::LitStr(s) => emit_helpers::emit_header_value_from_static(s),
                _ => {
                    let ex = emit_value_expr(value, ctx);
                    emit_helpers::emit_header_value_from_expr(&syn::parse2(ex).unwrap(), &ks, sp)
                }
            };

            quote! {
                policy.insert_header(#name, #hv);
            }
        }
        PolicyKeyKind::Query => {
            let (ks, sp, _) = emit_key_string(key, kind);
            let lit = emit_helpers::lit_str(&ks, sp);
            if let ValueKind::Fmt(fmt) = value {
                let build = emit_fmt_build_string(fmt);
                let setter = match op {
                    SetOp::Set => quote! { policy.set_query(#lit, __fmt_s); },
                    SetOp::Push => quote! { policy.push_query(#lit, __fmt_s); },
                };
                if fmt.require_all {
                    let checks = fmt.pieces.iter().filter_map(|p| {
                        let FmtResolvedPiece::Var {
                            source,
                            field,
                            optional: true,
                        } = p
                        else {
                            return None;
                        };
                        match source {
                            FmtVarSource::Cx => {
                                Some(quote! { if vars.#field.is_none() { __fmt_ok = false; } })
                            }
                            FmtVarSource::Ep => {
                                Some(quote! { if ep.#field.is_none() { __fmt_ok = false; } })
                            }
                            FmtVarSource::Auth => {
                                Some(quote! { if auth.#field.is_none() { __fmt_ok = false; } })
                            }
                        }
                    });
                    return quote! {
                        let mut __fmt_ok: bool = true;
                        #( #checks )*
                        if __fmt_ok {
                            let __fmt_s: ::std::string::String = { #build };
                            #setter
                        } else {
                            policy.remove_query(#lit);
                        }
                    };
                } else {
                    return quote! {
                        let __fmt_s: ::std::string::String = { #build };
                        #setter
                    };
                }
            }
            if let ValueKind::AuthField(fld) = value {
                let setter = match op {
                    SetOp::Set => quote! { policy.set_query(#lit, __s); },
                    SetOp::Push => quote! { policy.push_query(#lit, __s); },
                };
                return if let Some(OptionalRefKind::Auth) = conditional {
                    quote! {
                        {
                           if let ::core::option::Option::Some(__v) = auth.#fld.as_ref() {
                                let __s = __v.expose();
                                #setter
                            } else {
                                policy.remove_query(#lit);
                            }
                        }
                    }
                } else {
                    quote! {
                    {
                        let __s = auth.#fld.expose();
                        #setter
                    }
                    }
                };
            }
            if let Some(_ref_kind) = conditional {
                let as_ref_expr = match value {
                    ValueKind::CxField(f) => match ctx {
                        PolicyEmitCtx::ClientBase => quote! { vars.#f.as_ref() },
                        _ => quote! { vars.#f.as_ref() },
                    },
                    ValueKind::EpField(f) => quote! { ep.#f.as_ref() },
                    _ => unreachable!(),
                };
                let setter = match op {
                    SetOp::Set => quote! { policy.set_query(#lit, __v.to_string()); },
                    SetOp::Push => quote! { policy.push_query(#lit, __v.to_string()); },
                };
                return quote! {
                    if let ::core::option::Option::Some(__v) = #as_ref_expr {
                        #setter
                    } else {
                        policy.remove_query(#lit);
                    }
                };
            }

            let ex = emit_value_expr(value, ctx);
            match op {
                SetOp::Set => quote! { policy.set_query(#lit, (#ex).to_string()); },
                SetOp::Push => quote! { policy.push_query(#lit, (#ex).to_string()); },
            }
        }
    }
}

