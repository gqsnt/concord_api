//! Emission of routes, DNS labels, patch-program tables, value readers,
//! authentication placements and rate declarations.

use proc_macro2::{Span, TokenStream};
use quote::quote;

use super::{
    Access, Context, duration_tokens, lit, located_at, root_expr, total_ref, value_field_expr,
};
use crate::semantic::{
    AuthChallenge, BoundDim, Emission, HeadOp, HostLabel, LocalProgram, Placement, PlacementFamily,
    PlacementKind, RAtom, RateUse, SegPart, Segment, StatementKind, VRef,
};

/// A generated role call `method(args)` (or `method::<A, T>(args)` with an
/// adapter) whose obligations are reported at `span`, the authored placement
/// (DIAG-01). Only generated tokens are relocated: the author's adapter and
/// value types keep their own spans, so a type that does not resolve is still
/// reported where it is written.
fn placement_call(
    context: &Context<'_>,
    span: Span,
    method: TokenStream,
    adapter: Option<(&syn::Type, &syn::Type)>,
    args: TokenStream,
) -> TokenStream {
    let args = located_at(quote!((#args)), span);
    match adapter {
        Some((adapter, ty)) => {
            let adapter = context.rust_type(adapter);
            let ty = context.rust_type(ty);
            let method = located_at(quote!(#method::<), span);
            let comma = located_at(quote!(,), span);
            let close = located_at(quote!(>), span);
            quote!(#method #adapter #comma #ty #close #args)
        }
        None => {
            let method = located_at(method, span);
            quote!(#method #args)
        }
    }
}

/// Statements writing the parts of one total atom into `writer`.
fn write_atom_parts(context: &Context<'_>, access: Access, atom: &RAtom) -> TokenStream {
    let parts: Vec<RAtom> = atom.clone().flatten();
    let statements = parts.iter().map(|part| match part {
        RAtom::Literal { text, .. } => {
            let text = lit(text);
            quote!(__concord_writer.push_str(#text)?;)
        }
        RAtom::Value { value, via, span } => {
            let reference = total_ref(context.model, access, *value);
            // DIAG-01: the `RequestAtom`/`AtomEncoder` obligation is reported
            // at the authored reference, not across the whole invocation.
            let write = write_call(context, *value, via.as_ref(), *span, reference);
            quote!(#write?;)
        }
        RAtom::Concat(_) => unreachable!("flattened"),
    });
    quote!(#(#statements)*)
}

/// `writer.write(..)` / `writer.write_with::<A, T>(..)` for one value part.
fn write_call(
    context: &Context<'_>,
    value: VRef,
    via: Option<&syn::Type>,
    span: Span,
    reference: TokenStream,
) -> TokenStream {
    match via {
        Some(adapter) => placement_call(
            context,
            span,
            quote!(__concord_writer.write_with),
            Some((adapter, &context.model.value(value).ty)),
            reference,
        ),
        None => placement_call(
            context,
            span,
            quote!(__concord_writer.write),
            None,
            reference,
        ),
    }
}

/// `builder.push_*` statement for one host label.
pub(crate) fn host_label(context: &Context<'_>, access: Access, label: &HostLabel) -> TokenStream {
    if let RAtom::Literal { text, .. } = &label.atom {
        let text = lit(text);
        return quote!(__concord_builder.push_host_static(#text)?;);
    }
    let sensitive = label.sensitive;
    let body = write_atom_parts(context, access, &label.atom);
    quote! {
        __concord_builder.push_host_atoms(#sensitive, |__concord_writer| {
            #body
            ::std::result::Result::Ok(())
        })?;
    }
}

/// `builder.push_*` statement for one generated path segment.
pub(crate) fn path_segment(
    context: &Context<'_>,
    access: Access,
    segment: &Segment,
) -> TokenStream {
    if let Some(text) = segment.static_text() {
        let text = lit(text);
        return quote!(__concord_builder.push_static(#text)?;);
    }
    let sensitive = segment.sensitive;
    let statements = segment.parts.iter().map(|part| match part {
        SegPart::Literal(text) => {
            let text = lit(text);
            quote!(__concord_writer.push_str(#text)?;)
        }
        SegPart::Value { value, via, span } => {
            let reference = total_ref(context.model, access, *value);
            let write = write_call(context, *value, via.as_ref(), *span, reference);
            quote!(#write?;)
        }
    });
    quote! {
        __concord_builder.push_path_atoms(#sensitive, |__concord_writer| {
            #(#statements)*
            ::std::result::Result::Ok(())
        })?;
    }
}

fn family_tokens(context: &Context<'_>, family: PlacementFamily) -> TokenStream {
    let p = context.p();
    match family {
        PlacementFamily::Query => quote!(#p::PatchFamily::Query),
        PlacementFamily::Header => quote!(#p::PatchFamily::Header),
    }
}

/// The static `PatchOp` table of one site's normalized program.
pub(crate) fn ops_table(context: &Context<'_>, program: &LocalProgram) -> TokenStream {
    let p = context.p();
    let mut emission = 0_u16;
    let ops = program.ops.iter().map(|op| {
        let family = family_tokens(context, op.family);
        let key = lit(&op.key);
        match op.kind {
            None => quote!(#p::PatchOp::clear(#family)),
            Some(StatementKind::Remove) => quote!(#p::PatchOp::remove(#family, #key)),
            Some(kind) => {
                let kind = match kind {
                    StatementKind::Introduce => quote!(#p::PatchKind::Introduce),
                    StatementKind::Replace => quote!(#p::PatchKind::Replace),
                    StatementKind::WhenPresent => quote!(#p::PatchKind::WhenPresent),
                    StatementKind::Append => quote!(#p::PatchKind::Append),
                    StatementKind::Remove => unreachable!(),
                };
                let index = emission;
                emission += 1;
                let sensitive = op.sensitive;
                quote!(#p::PatchOp::keyed(#family, #kind, #key, #index, #sensitive))
            }
        }
    });
    quote!(&[#(#ops),*])
}

/// Body of `emission_present` and `emit` for one site.
pub(crate) fn emission_readers(
    context: &Context<'_>,
    access: Access,
    program: &LocalProgram,
) -> (TokenStream, TokenStream) {
    let emitting: Vec<(u16, &HeadOp)> = program
        .ops
        .iter()
        .filter(|op| op.emission.is_some())
        .enumerate()
        .map(|(index, op)| (u16::try_from(index).expect("bounded program"), op))
        .collect();
    let presence_arms = emitting.iter().map(|(index, op)| {
        let present = match op.presence {
            Some(value) => {
                let field = value_field_expr(context.model, access, value);
                quote!(#field.is_some())
            }
            None => quote!(true),
        };
        quote!(#index => #present,)
    });
    let emit_arms = emitting.iter().map(|(index, op)| {
        let body = match op.emission.as_ref().expect("emitting op") {
            Emission::Each { value, via, span } => {
                let optional = context.model.value(*value).optional;
                // DIAG-01: `OrderedValues` and item-atom obligations are
                // reported at the authored `each` source.
                let ty = &context.model.value(*value).ty;
                let call = match via {
                    Some(adapter) => placement_call(
                        context,
                        *span,
                        quote!(__concord_sink.each_with),
                        Some((adapter, ty)),
                        quote!(__concord_values),
                    ),
                    None => placement_call(
                        context,
                        *span,
                        quote!(__concord_sink.each),
                        None,
                        quote!(__concord_values),
                    ),
                };
                if optional {
                    let field = value_field_expr(context.model, access, *value);
                    quote! {
                        match #field.as_deref() {
                            ::std::option::Option::Some(__concord_values) => #call,
                            ::std::option::Option::None => ::std::result::Result::Ok(()),
                        }
                    }
                } else {
                    let reference = total_ref(context.model, access, *value);
                    quote!({ let __concord_values = #reference; #call })
                }
            }
            Emission::Atom(RAtom::Value { value, via, span }) => {
                let optional = context.model.value(*value).optional;
                let ty = &context.model.value(*value).ty;
                let call = match via {
                    Some(adapter) => placement_call(
                        context,
                        *span,
                        quote!(__concord_sink.value_with),
                        Some((adapter, ty)),
                        quote!(__concord_value),
                    ),
                    None => placement_call(
                        context,
                        *span,
                        quote!(__concord_sink.value),
                        None,
                        quote!(__concord_value),
                    ),
                };
                if optional {
                    let field = value_field_expr(context.model, access, *value);
                    quote! {
                        match #field.as_deref() {
                            ::std::option::Option::Some(__concord_value) => #call,
                            ::std::option::Option::None => ::std::result::Result::Ok(()),
                        }
                    }
                } else {
                    let reference = total_ref(context.model, access, *value);
                    quote!({ let __concord_value = #reference; #call })
                }
            }
            Emission::Atom(atom) => {
                let body = write_atom_parts(context, access, atom);
                quote! {
                    __concord_sink.atom(|__concord_writer| {
                        #body
                        ::std::result::Result::Ok(())
                    })
                }
            }
        };
        quote!(#index => #body,)
    });
    (
        quote! {
            match __concord_emission {
                #(#presence_arms)*
                _ => false,
            }
        },
        quote! {
            match __concord_emission {
                #(#emit_arms)*
                _ => ::std::result::Result::Ok(()),
            }
        },
    )
}

/// `builder.push_auth(...)` statement for one placement.
pub(crate) fn auth_placement(
    context: &Context<'_>,
    access: Access,
    placement: &Placement,
) -> TokenStream {
    let f = context.f;
    let p = context.p();
    let root = root_expr(context.model, access);
    let credential = &context.model.client.credentials[placement.credential].field;
    let reference = quote!(#root.#credential.credential());
    let challenge = match placement.challenge {
        AuthChallenge::Never => quote!(#f::auth::ChallengePolicy::Never),
        AuthChallenge::Unauthorized => quote!(#f::auth::ChallengePolicy::Unauthorized),
        AuthChallenge::UnauthorizedOrForbidden => {
            quote!(#f::auth::ChallengePolicy::UnauthorizedOrForbidden)
        }
    };
    // DIAG-01: material-capability obligations (`SecretCredential`) are
    // reported at the authored auth placement. The qualified `<Auth>::`
    // form keeps each call expression within one expansion context, so the
    // relocated span is not widened to the whole invocation.
    let statement = match &placement.kind {
        PlacementKind::Bearer => {
            quote!(__concord_builder.push_auth(<#f::auth::Auth>::bearer(#reference).challenge(#challenge))?;)
        }
        PlacementKind::Basic => {
            quote!(__concord_builder.push_auth(<#f::auth::Auth>::basic(#reference).challenge(#challenge))?;)
        }
        PlacementKind::Authorization(None) => {
            quote!(__concord_builder.push_auth(<#f::auth::Auth>::authorization(#reference).challenge(#challenge))?;)
        }
        PlacementKind::Authorization(Some(scheme)) => {
            let scheme = lit(scheme);
            quote!(__concord_builder.push_auth_result(<#f::auth::Auth>::authorization_with_scheme(#scheme, #reference).map(|__concord_auth| __concord_auth.challenge(#challenge)))?;)
        }
        PlacementKind::Header(name) => {
            let name = lit(&name.to_ascii_lowercase());
            quote!(__concord_builder.push_auth_result(<#f::auth::Auth>::header(#p::HeaderName::from_static(#name), #reference).map(|__concord_auth| __concord_auth.challenge(#challenge)))?;)
        }
        PlacementKind::Query(name) => {
            let name = lit(name);
            quote!(__concord_builder.push_auth_result(<#f::auth::Auth>::query(#name, #reference).map(|__concord_auth| __concord_auth.challenge(#challenge)))?;)
        }
    };
    located_at(statement, placement.span)
}

/// `builder.push_rate_limit(...)` statement for one rate attachment.
pub(crate) fn rate_declaration(
    context: &Context<'_>,
    access: Access,
    rate: &RateUse,
) -> TokenStream {
    let p = context.p();
    let id = lit(&rate.id);
    let dims = rate.dims.iter().map(|dim| match dim {
        BoundDim::Origin => quote!(.origin()),
        BoundDim::Endpoint => quote!(.endpoint()),
        BoundDim::Custom {
            label,
            value,
            via,
            span,
        } => {
            let label = lit(label);
            let reference = total_ref(context.model, access, *value);
            // DIAG-01: `RateKeyPart`/`RateKeyEncoder` obligations are
            // reported at the authored dimension binding.
            let ty = &context.model.value(*value).ty;
            let part = match via {
                Some(encoder) => placement_call(
                    context,
                    *span,
                    quote!(part_with),
                    Some((encoder, ty)),
                    quote!(#label, #reference),
                ),
                None => placement_call(
                    context,
                    *span,
                    quote!(part),
                    None,
                    quote!(#label, #reference),
                ),
            };
            quote!(.#part)
        }
    });
    let windows = rate.windows.iter().map(|window| {
        let capacity = window.capacity;
        let duration = duration_tokens(window.duration);
        quote!(.window(#capacity, #duration))
    });
    let cost = (rate.cost != 1).then(|| {
        let cost = rate.cost;
        quote!(.cost(#cost))
    });
    quote! {
        __concord_builder.push_rate_limit(
            #p::RateLimitDeclaration::new_static(#id) #(#dims)* #(#windows)* #cost
        )?;
    }
}
