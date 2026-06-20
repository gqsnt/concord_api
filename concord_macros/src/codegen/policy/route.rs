fn emit_fmt_build_string(fmt: &FmtResolved) -> proc_macro2::TokenStream {
    let mut ops: Vec<proc_macro2::TokenStream> = Vec::new();

    for p in &fmt.pieces {
        match p {
            FmtResolvedPiece::Lit(s) => {
                ops.push(quote! { __fmt_s.push_str(#s); });
            }
            FmtResolvedPiece::Var {
                source,
                field,
                optional,
            } => match source {
                FmtVarSource::Cx => {
                    if *optional {
                        ops.push(quote! {
                            if let ::core::option::Option::Some(__v) = vars.#field.as_ref() {
                                __fmt_s.push_str(&__v.to_string());
                            }
                        });
                    } else {
                        ops.push(quote! { __fmt_s.push_str(&vars.#field.to_string()); });
                    }
                }
                FmtVarSource::Ep => {
                    if *optional {
                        ops.push(quote! {
                            if let ::core::option::Option::Some(__v) = ep.#field.as_ref() {
                                __fmt_s.push_str(&__v.to_string());
                            }
                        });
                    } else {
                        ops.push(quote! { __fmt_s.push_str(&ep.#field.to_string()); });
                    }
                }
            },
        }
    }

    quote! {
        let mut __fmt_s = ::std::string::String::new();
        #( #ops )*
        __fmt_s
    }
}

fn emit_fmt_build_string_with_ep_optionals(
    fmt: &FmtResolved,
    ep_optionals: Option<&std::collections::BTreeMap<String, bool>>,
) -> proc_macro2::TokenStream {
    let mut ops: Vec<proc_macro2::TokenStream> = Vec::new();
    for p in &fmt.pieces {
        match p {
            FmtResolvedPiece::Lit(s) => {
                ops.push(quote! { __fmt_s.push_str(#s); });
            }
            FmtResolvedPiece::Var {
                source,
                field,
                optional,
            } => match source {
                FmtVarSource::Cx => {
                    if *optional {
                        ops.push(quote! {
                            if let ::core::option::Option::Some(__v) = vars.#field.as_ref() {
                                __fmt_s.push_str(&__v.to_string());
                            }
                        });
                    } else {
                        ops.push(quote! { __fmt_s.push_str(&vars.#field.to_string()); });
                    }
                }
                FmtVarSource::Ep => {
                    let is_optional = ep_optionals
                        .and_then(|m| m.get(&field.to_string()).copied())
                        .unwrap_or(*optional);
                    if is_optional {
                        ops.push(quote! {
                            if let ::core::option::Option::Some(__v) = ep.#field.as_ref() {
                                __fmt_s.push_str(&__v.to_string());
                            }
                        });
                    } else {
                        ops.push(quote! { __fmt_s.push_str(&ep.#field.to_string()); });
                    }
                }
            },
        }
    }
    quote! {
        let mut __fmt_s = ::std::string::String::new();
        #( #ops )*
        __fmt_s
    }
}

fn emit_value_expr(v: &PublicValueKind, ctx: PolicyEmitCtx) -> TokenStream2 {
    match v {
        PublicValueKind::LitStr(s) => quote! { #s },
        PublicValueKind::CxField(f) => match ctx {
            PolicyEmitCtx::ClientBase => quote! { &vars.#f },
            _ => quote! { &vars.#f },
        },
        PublicValueKind::EpField(f) => quote! { &ep.#f },
        PublicValueKind::OtherExpr(e) => quote! { (#e) },
        PublicValueKind::Fmt(fmt) => {
            let build = emit_fmt_build_string(fmt);
            quote! { { #build } }
        }
    }
}

fn emit_dynamic_path_segment_push(value: TokenStream2, label: LitStr) -> TokenStream2 {
    quote! {
        {
            let __segment = (#value).to_string();
            if __segment.contains('/') || __segment.contains('\\') {
                return ::core::result::Result::Err(
                    ::concord_core::prelude::ApiClientError::invalid_param(ctx.clone(), #label)
                );
            }
            route.path_mut().push_segment_encoded(&__segment);
        }
    }
}

fn emit_prefix_route_apply(
    pieces: &[PrefixPiece],
    ep_optionals: Option<&std::collections::BTreeMap<String, bool>>,
) -> TokenStream2 {
    // HostParts joins labels in natural insertion order.
    let mut ops = Vec::new();
    for p in pieces {
        match p {
            PrefixPiece::Static(s) => {
                let lit = LitStr::new(s, Span::call_site());
                ops.push(quote! { route.host_mut().push_label_static(#lit); });
            }
            PrefixPiece::CxVar { field, optional } => {
                let wire_lit = LitStr::new(&format!("cx.{}", field), Span::call_site());
                if *optional {
                    ops.push(quote! {
                        if let ::core::option::Option::Some(__v) = vars.#field.as_ref() {
                            route.host_mut().push_label(__v.to_string(), ::concord_core::advanced::HostLabelSource::Placeholder { name: #wire_lit });
                        }
                    });
                } else {
                    ops.push(quote! {
                        route.host_mut().push_label(vars.#field.to_string(), ::concord_core::advanced::HostLabelSource::Placeholder { name: #wire_lit });
                    });
                }
            }
            PrefixPiece::EpVar { field } => {
                let is_optional = ep_optionals
                    .and_then(|m| m.get(&field.to_string()).copied())
                    .unwrap_or(false);
                let wire_lit = LitStr::new(&format!("ep.{}", field), Span::call_site());
                if is_optional {
                    ops.push(quote! {
                        if let ::core::option::Option::Some(__v) = ep.#field.as_ref() {
                            route.host_mut().push_label(__v.to_string(), ::concord_core::advanced::HostLabelSource::Placeholder { name: #wire_lit });
                        }
                    });
                } else {
                    ops.push(quote! {
                        route.host_mut().push_label(ep.#field.to_string(), ::concord_core::advanced::HostLabelSource::Placeholder { name: #wire_lit });
                    });
                }
            }
            PrefixPiece::Fmt(fmt) => {
                let build = emit_fmt_build_string_with_ep_optionals(fmt, ep_optionals);

                if fmt.require_all {
                    let guard = emit_fmt_require_all_guard_with_ep_optionals(fmt, ep_optionals);
                    ops.push(quote! {
                        {
                            if { #guard } {
                                let __fmt_s: ::std::string::String = { #build };
                                route.host_mut().push_label(
                                    __fmt_s,
                                    ::concord_core::advanced::HostLabelSource::Mixed
                                );
                            }
                        }
                    });
                } else {
                    ops.push(quote! {
                        {
                            let __fmt_s: ::std::string::String = { #build };
                            route.host_mut().push_label(
                                __fmt_s,
                                ::concord_core::advanced::HostLabelSource::Mixed
                            );
                        }
                    });
                }
            }
        }
    }
    quote! { #( #ops )* }
}

fn emit_path_route_apply(
    pieces: &[PathPiece],
    ep_optionals: Option<&std::collections::BTreeMap<String, bool>>,
) -> TokenStream2 {
    let mut ops = Vec::new();
    for p in pieces {
        match p {
            PathPiece::Static(s) => {
                let lit = LitStr::new(s, Span::call_site());
                ops.push(quote! { route.path_mut().push_raw(#lit); });
            }
            PathPiece::CxVar { field, optional } => {
                let label = LitStr::new(&format!("vars.{field}"), Span::call_site());
                if *optional {
                    let push =
                        emit_dynamic_path_segment_push(quote! { __v }, label.clone());
                    ops.push(quote! {
                        if let ::core::option::Option::Some(__v) = vars.#field.as_ref() {
                            #push
                        }
                    });
                } else {
                    ops.push(emit_dynamic_path_segment_push(
                        quote! { &vars.#field },
                        label,
                    ));
                }
            }
            PathPiece::EpVar { field } => {
                let is_optional = ep_optionals
                    .and_then(|m| m.get(&field.to_string()).copied())
                    .unwrap_or(false);
                let label = LitStr::new(&format!("ep.{field}"), Span::call_site());
                if is_optional {
                    let push =
                        emit_dynamic_path_segment_push(quote! { __v }, label.clone());
                    ops.push(quote! {
                        if let ::core::option::Option::Some(__v) = ep.#field.as_ref() {
                            #push
                        }
                    });
                } else {
                    ops.push(emit_dynamic_path_segment_push(quote! { &ep.#field }, label));
                }
            }
            PathPiece::Fmt(fmt) => {
                let build = emit_fmt_build_string_with_ep_optionals(fmt, ep_optionals);
                let label = LitStr::new("fmt", Span::call_site());

                if fmt.require_all {
                    let guard = emit_fmt_require_all_guard_with_ep_optionals(fmt, ep_optionals);
                    let push = emit_dynamic_path_segment_push(quote! { __fmt_s }, label.clone());
                    ops.push(quote! {
                        {
                            if { #guard } {
                                let __fmt_s: ::std::string::String = { #build };
                                #push
                            }
                        }
                    });
                } else {
                    let push = emit_dynamic_path_segment_push(quote! { __fmt_s }, label);
                    ops.push(quote! {
                        {
                            let __fmt_s: ::std::string::String = { #build };
                            #push
                        }
                    });
                }
            }
        }
    }
    quote! { #( #ops )* }
}



