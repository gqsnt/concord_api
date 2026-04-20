fn emit_fmt_require_all_guard_with_ep_optionals(
    fmt: &FmtResolved,
    ep_optionals: Option<&std::collections::BTreeMap<String, bool>>,
) -> TokenStream2 {
    let checks = fmt.pieces.iter().filter_map(|p| {
        let FmtResolvedPiece::Var {
            source,
            field,
            optional,
        } = p
        else {
            return None;
        };
        let effective_optional = match source {
            FmtVarSource::Ep => ep_optionals
                .and_then(|m| m.get(&field.to_string()).copied())
                .unwrap_or(*optional),
            _ => *optional,
        };
        if !effective_optional {
            return None;
        }
        match source {
            FmtVarSource::Cx => Some(quote! { if vars.#field.is_none() { __fmt_ok = false; } }),
            FmtVarSource::Ep => Some(quote! { if ep.#field.is_none() { __fmt_ok = false; } }),
            FmtVarSource::Auth => Some(quote! { if auth.#field.is_none() { __fmt_ok = false; } }),
        }
    });

    quote! {
        let mut __fmt_ok: bool = true;
        #( #checks )*
        __fmt_ok
    }
}
