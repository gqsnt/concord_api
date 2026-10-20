//! Public request binding with page-live cursor inputs erased.

use proc_macro2::TokenStream;
use quote::quote;

use super::head::{emission_readers, host_label, ops_table, path_segment};
use super::{Access, Context, located_at};
use crate::semantic::{Emission, InitValue, LocalHost, LocalPath, RAtom, SegPart, Site, VRef};

fn is_cursor(context: &Context<'_>, index: usize, value: VRef) -> bool {
    value.site == Site::Endpoint(index)
        && context.model.endpoints[index]
            .pagination
            .as_ref()
            .expect("pagination")
            .cursor
            .contains(&value.index)
}

fn excluded(context: &Context<'_>, index: usize, value: VRef) -> bool {
    is_cursor(context, index, value) || context.model.value(value).sensitive
}

fn erase_inputs(context: &Context<'_>, index: usize, atom: &mut RAtom) {
    match atom {
        RAtom::Value { value, .. } if excluded(context, index, *value) => {
            *atom = RAtom::Literal {
                text: "x".to_owned(),
                kind: crate::semantic::LiteralKind::String,
            };
        }
        RAtom::Concat(parts) => {
            for part in parts {
                erase_inputs(context, index, part);
            }
        }
        _ => {}
    }
}

fn view_name(context: &Context<'_>, index: usize) -> syn::Ident {
    let spec = &context.model.endpoints[index].surface.spec_type;
    syn::Ident::new(&format!("{spec}CheckpointView"), spec.span())
}

pub(super) fn items(context: &Context<'_>, index: usize) -> TokenStream {
    let endpoint = &context.model.endpoints[index];
    let Some(pagination) = &endpoint.pagination else {
        return TokenStream::new();
    };
    let f = context.f;
    let p = context.p();
    let local = &endpoint.surface.local_type;
    let view = view_name(context, index);
    let access = Access::Endpoint(index);

    // Flatten the same symbolic program used by ordinary shaping, so
    // suppression and ordering are preserved across the full owner chain.
    // Normalize excluded payloads and presence before running any adapters.
    // This also preserves public parts of atoms that contain a secret.
    let chain = context.model.chain(endpoint.owner);
    let mut program = endpoint.program.clone();
    program.ops = chain
        .iter()
        .flat_map(|owner| context.model.owners[*owner].program.ops.clone())
        .chain(endpoint.program.ops.clone())
        .collect();
    for op in &mut program.ops {
        if op
            .presence
            .is_some_and(|value| excluded(context, index, value))
        {
            op.presence = None;
        }
        match &mut op.emission {
            Some(Emission::Atom(atom)) => erase_inputs(context, index, atom),
            Some(Emission::Each { value, .. }) if excluded(context, index, *value) => {
                op.emission = Some(Emission::Atom(RAtom::Literal {
                    text: "x".to_owned(),
                    kind: crate::semantic::LiteralKind::String,
                }));
            }
            _ => {}
        }
        // Every secret payload has been erased. Keeping sensitivity here
        // would conceal public atoms sharing the same target key.
        op.sensitive = false;
    }
    let ops = ops_table(context, &program);
    let (presence, emit) = emission_readers(context, access, &program);
    let program = quote! {
        #p::apply_head_program(&mut __concord_builder, &[
            #p::ProgramSegment { ops: Self::__OPS, emissions: self },
        ])?;
    };
    let mut path = Vec::new();
    for local_path in chain
        .iter()
        .map(|owner| &context.model.owners[*owner].path)
        .chain([&endpoint.path])
    {
        match local_path {
            None => {}
            Some(LocalPath::Append(template)) => path.extend(template.segments.clone()),
            Some(LocalPath::Replace(template)) => path = template.segments.clone(),
            Some(LocalPath::Off) => path.clear(),
        }
    }
    for segment in &mut path {
        for part in &mut segment.parts {
            if matches!(part, SegPart::Value { value, .. } if excluded(context, index, *value)) {
                *part = SegPart::Literal("x".to_owned());
            }
        }
        segment.sensitive = false;
    }
    let path = path
        .iter()
        .map(|segment| path_segment(context, access, segment));
    let mut host = Vec::new();
    for owner in &chain {
        match &context.model.owners[*owner].host {
            None => {}
            Some(LocalHost::Prepend(labels)) => {
                let mut prepended = labels.clone();
                prepended.extend(host);
                host = prepended;
            }
            Some(LocalHost::Replace(labels)) => host = labels.clone(),
            Some(LocalHost::Off) => host.clear(),
        }
    }
    for label in &mut host {
        erase_inputs(context, index, &mut label.atom);
        label.sensitive = false;
    }
    let host = host.iter().map(|label| host_label(context, access, label));
    located_at(
        quote! {
            struct #view<'a> {
                __owner: &'a __State,
                __local: &'a #local,
            }

            impl #view<'_> {
                const __OPS: &'static [#p::PatchOp<'static>] = #ops;

                fn prepare_head(
                    &self,
                    mut __concord_builder: #p::HeadBuilder,
                ) -> ::std::result::Result<#p::PreparedHead, #f::ApiError> {
                    #(#host)*
                    #(#path)*
                    #program
                    __concord_builder.finish()
                }
            }

            impl #p::HeadEmissions for #view<'_> {
                fn emission_present(&self, __concord_emission: u16) -> bool {
                    #presence
                }

                fn emit(
                    &self,
                    __concord_emission: u16,
                    __concord_sink: &mut #p::OccurrenceSink<'_>,
                ) -> ::std::result::Result<(), #f::AtomError> {
                    #emit
                }
            }
        },
        pagination.span,
    )
}

pub(super) fn methods(context: &Context<'_>, index: usize) -> TokenStream {
    let endpoint = &context.model.endpoints[index];
    let pagination = endpoint.pagination.as_ref().expect("pagination");
    let p = context.p();
    let f = context.f;
    let controller = context.rust_type(&pagination.controller);
    let output = context.rust_type(&endpoint.output);
    let view = view_name(context, index);
    let sensitive = pagination.cursor.iter().any(|index| endpoint.values[*index].sensitive)
        || pagination.fields.iter().any(|(_, value)| {
            matches!(value, InitValue::Value(value) if context.model.value(*value).sensitive)
        })
        || endpoint.program.ops.iter().any(|op| {
            op.sensitive && op.emission.as_ref().is_some_and(|emission| {
                emission.values().iter().any(|value| is_cursor(context, index, *value))
            })
        })
        || endpoint.path.as_ref().is_some_and(|path| {
            let template = match path {
                LocalPath::Append(template) | LocalPath::Replace(template) => template,
                LocalPath::Off => return false,
            };
            template.segments.iter().any(|segment| {
                segment.sensitive && segment.parts.iter().any(|part| {
                    matches!(part, SegPart::Value { value, .. } if is_cursor(context, index, *value))
                })
            })
        });
    quote! {
        fn checkpoint_codec(&self) -> ::std::option::Option<#p::CheckpointCodec<Self::Controller, Self::Output>> {
            // Autoref dispatch selects the opt-in for this concrete
            // controller without requiring every pageable controller to
            // implement the independent durable-state contract.
            struct __ConcordCheckpoint<C, P>(::std::marker::PhantomData<fn() -> (C, P)>);
            trait __ConcordSelectCheckpoint<C, P> {
                fn __concord_codec(self) -> ::std::option::Option<#p::CheckpointCodec<C, P>>;
            }
            impl<C, P> __ConcordSelectCheckpoint<C, P> for &__ConcordCheckpoint<C, P> {
                fn __concord_codec(self) -> ::std::option::Option<#p::CheckpointCodec<C, P>> {
                    ::std::option::Option::None
                }
            }
            impl<C, P> __ConcordSelectCheckpoint<C, P> for &&__ConcordCheckpoint<C, P>
            where C: #f::pagination::ResumableController<P> {
                fn __concord_codec(self) -> ::std::option::Option<#p::CheckpointCodec<C, P>> {
                    ::std::option::Option::Some(#p::CheckpointCodec::new())
                }
            }
            let __concord_marker = __ConcordCheckpoint::<#controller, #output>(::std::marker::PhantomData);
            (&&__concord_marker).__concord_codec()
        }

        fn checkpoint_input(&self) -> ::std::result::Result<::std::vec::Vec<u8>, #f::pagination::CheckpointError> {
            let __concord_view = #view { __owner: &self.__owner, __local: &self.__local };
            #p::checkpoint_input(
                self,
                #p::OwnerProjection::core(__concord_view.__owner),
                |__concord_builder| __concord_view.prepare_head(__concord_builder),
            )
        }

        fn checkpoint_sensitive_cursor(&self) -> bool {
            #sensitive
        }
    }
}
