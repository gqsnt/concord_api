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
                FmtVarSource::Auth => {
                    if *optional {
                        ops.push(quote! {
                           if let ::core::option::Option::Some(__v) = auth.#field.as_ref() {
                                __fmt_s.push_str(__v.expose());
                            }
                        });
                    } else {
                        ops.push(quote! {
                            __fmt_s.push_str(auth.#field.expose());
                        });
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
                FmtVarSource::Auth => {
                    if *optional {
                        ops.push(quote! {
                           if let ::core::option::Option::Some(__v) = auth.#field.as_ref() {
                                __fmt_s.push_str(__v.expose());
                            }
                        });
                    } else {
                        ops.push(quote! {
                            __fmt_s.push_str(auth.#field.expose());
                        });
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

fn emit_value_expr(v: &ValueKind, ctx: PolicyEmitCtx) -> TokenStream2 {
    match v {
        ValueKind::LitStr(s) => quote! { #s },
        ValueKind::CxField(f) => match ctx {
            PolicyEmitCtx::ClientBase => quote! { &vars.#f },
            _ => quote! { &vars.#f },
        },
        ValueKind::EpField(f) => quote! { &ep.#f },
        ValueKind::AuthField(f) => quote! { auth.#f.expose() },
        ValueKind::OtherExpr(e) => quote! { (#e) },
        ValueKind::Fmt(fmt) => {
            let build = emit_fmt_build_string(fmt);
            quote! { { #build } }
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
                            route.host_mut().push_label(__v.to_string(), ::concord_core::prelude::HostLabelSource::Placeholder { name: #wire_lit });
                        }
                    });
                } else {
                    ops.push(quote! {
                        route.host_mut().push_label(vars.#field.to_string(), ::concord_core::prelude::HostLabelSource::Placeholder { name: #wire_lit });
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
                            route.host_mut().push_label(__v.to_string(), ::concord_core::prelude::HostLabelSource::Placeholder { name: #wire_lit });
                        }
                    });
                } else {
                    ops.push(quote! {
                        route.host_mut().push_label(ep.#field.to_string(), ::concord_core::prelude::HostLabelSource::Placeholder { name: #wire_lit });
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
                                    ::concord_core::prelude::HostLabelSource::Mixed
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
                                ::concord_core::prelude::HostLabelSource::Mixed
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
                if *optional {
                    ops.push(quote! {
                        if let ::core::option::Option::Some(__v) = vars.#field.as_ref() {
                            route.path_mut().push_segment_encoded(&__v.to_string());
                        }
                    });
                } else {
                    ops.push(
                        quote! { route.path_mut().push_segment_encoded(&vars.#field.to_string()); },
                    );
                }
            }
            PathPiece::EpVar { field } => {
                let is_optional = ep_optionals
                    .and_then(|m| m.get(&field.to_string()).copied())
                    .unwrap_or(false);
                if is_optional {
                    ops.push(quote! {
                        if let ::core::option::Option::Some(__v) = ep.#field.as_ref() {
                            route.path_mut().push_segment_encoded(&__v.to_string());
                        }
                    });
                } else {
                    ops.push(
                        quote! { route.path_mut().push_segment_encoded(&ep.#field.to_string()); },
                    );
                }
            }
            PathPiece::Fmt(fmt) => {
                let build = emit_fmt_build_string_with_ep_optionals(fmt, ep_optionals);

                if fmt.require_all {
                    let guard = emit_fmt_require_all_guard_with_ep_optionals(fmt, ep_optionals);
                    ops.push(quote! {
                        {
                            if { #guard } {
                                let __fmt_s: ::std::string::String = { #build };
                                route.path_mut().push_segment_encoded(&__fmt_s);
                            }
                        }
                    });
                } else {
                    ops.push(quote! {
                        {
                            let __fmt_s: ::std::string::String = { #build };
                            route.path_mut().push_segment_encoded(&__fmt_s);
                        }
                    });
                }
            }
        }
    }
    quote! { #( #ops )* }
}

