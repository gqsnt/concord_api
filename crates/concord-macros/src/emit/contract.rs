//! Emission of the effective-contract builder input (§18). Only local,
//! as-written facts are passed; core derives every effective view with the
//! same composition laws and validates them.

use proc_macro2::TokenStream;
use quote::{ToTokens, quote};

use super::{Context, docs, duration_tokens, lit};
use crate::semantic::{
    AuthChallenge, BoundDim, CredentialSource, EffectDecl, Emission, FamilyMode, FormalDim, HeadOp,
    InitValue, Initializer, LiteralKind, LocalAuth, LocalHost, LocalPath, Meta, OpSite, OwnerKind,
    PathTemplate, PlacementFamily, PlacementKind, Positional, RAtom, RateSource, RateUse,
    RepresentationChoice, SegPart, Site, StableValue, StatementKind, Status, VRef, Value, Windows,
};

fn text(tokens: &impl ToTokens) -> syn::LitStr {
    let rendered = tokens.to_token_stream().to_string();
    lit(&compact(&rendered))
}

/// Compacts token spacing for type/path display.
pub(crate) fn compact(rendered: &str) -> String {
    rendered
        .replace(" :: ", "::")
        .replace(":: ", "::")
        .replace(" ::", "::")
        .replace(" < ", "<")
        .replace("< ", "<")
        .replace(" <", "<")
        .replace(" > ", ">")
        .replace(" >", ">")
        .replace(" ,", ",")
        .replace("& ", "&")
        .replace("( ", "(")
        .replace(" )", ")")
        .replace("[ ", "[")
        .replace(" ]", "]")
}

fn doc_lines(meta: &Meta) -> TokenStream {
    let lines = meta
        .docs
        .iter()
        .map(|line| lit(line.strip_prefix(' ').unwrap_or(line)));
    quote!(&[#(#lines),*])
}

fn deprecation(context: &Context<'_>, meta: &Meta) -> TokenStream {
    let f = context.f;
    match &meta.deprecated {
        None => TokenStream::new(),
        Some(deprecation) => {
            let since = deprecation
                .since
                .as_ref()
                .map(|since| quote!(.since(#since)));
            let note = deprecation.note.as_ref().map(|note| quote!(.note(#note)));
            quote!(.deprecated(#f::inspection::Deprecation::new() #since #note))
        }
    }
}

fn value_ref(context: &Context<'_>, key: VRef, from: Site) -> TokenStream {
    let f = context.f;
    let model = context.model;
    let name = lit(&model.value(key).logical);
    if key.site == from {
        return quote!(#f::inspection::ValueRef::local(#name));
    }
    match key.site {
        Site::Owner(0) => quote!(#f::inspection::ValueRef::client(#name)),
        Site::Owner(owner) => {
            let scope = lit(&model.owners[owner].logical);
            quote!(#f::inspection::ValueRef::scope(#scope, #name))
        }
        Site::Endpoint(_) => quote!(#f::inspection::ValueRef::local(#name)),
    }
}

fn value_spec(context: &Context<'_>, value: &Value) -> TokenStream {
    let f = context.f;
    let name = lit(&value.logical);
    let ty = text(&value.ty);
    let optional = value.optional.then(|| quote!(.optional()));
    let init = match &value.init {
        Initializer::None => TokenStream::new(),
        Initializer::Default(_) => quote!(.trait_default()),
        Initializer::Stable(StableValue::Path(path)) => {
            let path = text(path);
            quote!(.stable_constant(#path))
        }
        Initializer::Stable(stable) => {
            let literal = text(&super::owner::stable_tokens(context, stable));
            quote!(.stable_literal(#literal))
        }
    };
    let sensitive = value.sensitive.then(|| quote!(.sensitive()));
    let noop = (!value.live && value.is_deprecated()).then(|| quote!(.deprecated_no_op()));
    let docs = doc_lines(&value.meta);
    let deprecated = deprecation(context, &value.meta);
    quote!(.value(#f::inspection::ValueSpec::new(#name, #ty) #optional #init #sensitive #noop .docs(#docs) #deprecated))
}

fn atom(context: &Context<'_>, atom: &RAtom, from: Site) -> TokenStream {
    let f = context.f;
    match atom {
        RAtom::Literal { text: value, kind } => {
            let value = lit(value);
            let kind = match kind {
                LiteralKind::String => quote!(String),
                LiteralKind::Char => quote!(Char),
                LiteralKind::Bool => quote!(Bool),
                LiteralKind::Integer => quote!(Integer),
            };
            quote!(#f::inspection::AtomExpression::literal(#f::inspection::LiteralKind::#kind, #value))
        }
        RAtom::Value { value, via, .. } => {
            let reference = value_ref(context, *value, from);
            match via {
                Some(adapter) => {
                    let adapter = text(adapter);
                    quote!(#f::inspection::AtomExpression::value_via(#reference, #adapter))
                }
                None => quote!(#f::inspection::AtomExpression::value(#reference)),
            }
        }
        RAtom::Concat(parts) => {
            let parts = parts.iter().map(|part| self::atom(context, part, from));
            quote!(#f::inspection::AtomExpression::concat(::std::vec![#(#parts),*]))
        }
    }
}

fn key_operation(context: &Context<'_>, op: &HeadOp, from: Site) -> Option<TokenStream> {
    let f = context.f;
    let kind = op.kind?;
    let key = lit(&op.key);
    let emission = op.emission.as_ref().map(|emission| match emission {
        Emission::Atom(value) => {
            let atom = atom(context, value, from);
            quote!(#f::inspection::Emission::scalar(#atom))
        }
        Emission::Each { value, via, .. } => {
            let reference = value_ref(context, *value, from);
            match via {
                Some(adapter) => {
                    let adapter = text(adapter);
                    quote!(#f::inspection::Emission::each_via(#reference, #adapter))
                }
                None => quote!(#f::inspection::Emission::each(#reference)),
            }
        }
    });
    let operation = match (kind, op.site) {
        (StatementKind::Introduce, OpSite::Direct) => {
            quote!(#f::inspection::KeyOperation::direct(#key, #emission))
        }
        (StatementKind::Introduce, OpSite::Block) => {
            quote!(#f::inspection::KeyOperation::introduce(#key, #emission))
        }
        (StatementKind::Replace, _) => {
            quote!(#f::inspection::KeyOperation::replace(#key, #emission))
        }
        (StatementKind::WhenPresent, _) => {
            quote!(#f::inspection::KeyOperation::when_present(#key, #emission))
        }
        (StatementKind::Append, _) => quote!(#f::inspection::KeyOperation::append(#key, #emission)),
        (StatementKind::Remove, _) => quote!(#f::inspection::KeyOperation::remove(#key)),
    };
    let method = match op.family {
        PlacementFamily::Query => quote!(query),
        PlacementFamily::Header => quote!(header),
    };
    Some(quote!(.#method(#operation)))
}

fn program(
    context: &Context<'_>,
    program: &crate::semantic::LocalProgram,
    from: Site,
) -> TokenStream {
    let f = context.f;
    let mode = |mode: FamilyMode, method: TokenStream| match mode {
        FamilyMode::Inherit => TokenStream::new(),
        FamilyMode::Replace => quote!(.#method(#f::inspection::FamilyMode::Replace)),
        FamilyMode::Off => quote!(.#method(#f::inspection::FamilyMode::Off)),
    };
    let query_mode = mode(program.query_mode, quote!(query_mode));
    let header_mode = mode(program.header_mode, quote!(header_mode));
    let ops = program
        .ops
        .iter()
        .filter_map(|op| key_operation(context, op, from));
    quote!(#query_mode #header_mode #(#ops)*)
}

fn template(context: &Context<'_>, template: &PathTemplate, from: Site) -> TokenStream {
    let f = context.f;
    let full = lit(&template.text);
    let segments = template.segments.iter().map(|segment| {
        let parts = segment.parts.iter().map(|part| match part {
            SegPart::Literal(value) => {
                let value = lit(value);
                quote!(#f::inspection::TemplatePart::literal(#value))
            }
            SegPart::Value { value, via, .. } => {
                let reference = value_ref(context, *value, from);
                match via {
                    Some(adapter) => {
                        let adapter = text(adapter);
                        quote!(#f::inspection::TemplatePart::hole_via(#reference, #adapter))
                    }
                    None => quote!(#f::inspection::TemplatePart::hole(#reference)),
                }
            }
        });
        quote!(#f::inspection::PathSegment::new(::std::vec![#(#parts),*]))
    });
    quote!(#f::inspection::PathTemplate::new(#full, ::std::vec![#(#segments),*]))
}

fn path(context: &Context<'_>, path: Option<&LocalPath>, from: Site) -> TokenStream {
    let f = context.f;
    match path {
        None => TokenStream::new(),
        Some(LocalPath::Append(value)) => {
            let value = template(context, value, from);
            quote!(.path(#f::inspection::PathOperation::Append(#value)))
        }
        Some(LocalPath::Replace(value)) => {
            let value = template(context, value, from);
            quote!(.path(#f::inspection::PathOperation::Replace(#value)))
        }
        Some(LocalPath::Off) => quote!(.path(#f::inspection::PathOperation::Off)),
    }
}

fn auth(context: &Context<'_>, auth: Option<&LocalAuth>) -> TokenStream {
    let f = context.f;
    let placements = |placements: &[crate::semantic::Placement]| {
        let placements = placements.iter().map(|placement| {
            let credential = lit(&context.model.client.credentials[placement.credential].logical);
            let base = match &placement.kind {
                PlacementKind::Bearer => quote!(#f::inspection::AuthPlacement::bearer(#credential)),
                PlacementKind::Basic => quote!(#f::inspection::AuthPlacement::basic(#credential)),
                PlacementKind::Authorization(scheme) => {
                    let scheme = match scheme {
                        Some(scheme) => {
                            let scheme = lit(scheme);
                            quote!(::std::option::Option::Some(#scheme))
                        }
                        None => quote!(::std::option::Option::None),
                    };
                    quote!(#f::inspection::AuthPlacement::new(#f::inspection::PlacementTarget::Authorization { scheme: #scheme }, #credential))
                }
                PlacementKind::Header(name) => {
                    let name = lit(name);
                    quote!(#f::inspection::AuthPlacement::header(#name, #credential))
                }
                PlacementKind::Query(name) => {
                    let name = lit(name);
                    quote!(#f::inspection::AuthPlacement::query(#name, #credential))
                }
            };
            let challenge = match placement.challenge {
                AuthChallenge::Never => TokenStream::new(),
                AuthChallenge::Unauthorized => quote!(.challenge(#f::inspection::ChallengeRule::Unauthorized)),
                AuthChallenge::UnauthorizedOrForbidden => quote!(.challenge(#f::inspection::ChallengeRule::UnauthorizedOrForbidden)),
            };
            quote!(#base #challenge)
        });
        quote!(::std::vec![#(#placements),*])
    };
    match auth {
        None => TokenStream::new(),
        Some(LocalAuth::Append(list)) => {
            let list = placements(list);
            quote!(.auth(#f::inspection::AuthOperation::append(#list)))
        }
        Some(LocalAuth::Replace(list)) => {
            let list = placements(list);
            quote!(.auth(#f::inspection::AuthOperation::replace(#list)))
        }
        Some(LocalAuth::Off) => quote!(.auth(#f::inspection::AuthOperation::Off)),
    }
}

/// The encoding-contract name of one custom rate-key dimension. DIAG-01: the
/// `RateKeyPart`/`RateKeyEncoder` obligation is reported at `span`, the
/// authored dimension binding.
fn encoding_id(
    context: &Context<'_>,
    key: VRef,
    via: Option<&syn::Type>,
    span: proc_macro2::Span,
) -> TokenStream {
    let p = context.p();
    let ty = context.rust_type(&context.model.value(key).ty);
    let tokens = match via {
        Some(encoder) => {
            let encoder = context.rust_type(encoder);
            quote!(<#encoder as #p::RateKeyEncoder<#ty>>::ENCODING_ID)
        }
        None => quote!(<#ty as #p::RateKeyPart>::ENCODING_ID),
    };
    super::located_at(tokens, span)
}

fn rate(context: &Context<'_>, rate: &RateUse, from: Site) -> TokenStream {
    let f = context.f;
    let model = context.model;
    let cost = rate.attachment_cost.map(|cost| quote!(.cost(#cost)));
    match &rate.source {
        RateSource::Named { policy, bindings } => {
            let entry = &model.client.policies[*policy];
            let name = lit(&entry.logical);
            let binds = entry.formals.iter().zip(bindings).map(|(formal, value)| {
                let formal_name = lit(&crate::semantic::surface::canonical_ident(formal));
                let reference = value_ref(context, *value, from);
                let formal_index = entry
                    .formals
                    .iter()
                    .position(|candidate| candidate == formal)
                    .expect("formal");
                let dim = entry.dims.iter().position(|dim| {
                    matches!(dim, FormalDim::Custom { formal, .. } if *formal == formal_index)
                });
                let via = dim.and_then(|dim| match &entry.dims[dim] {
                    FormalDim::Custom { via, .. } => via.clone(),
                    _ => None,
                });
                let span = dim
                    .and_then(|dim| match rate.dims.get(dim) {
                        Some(BoundDim::Custom { span, .. }) => Some(*span),
                        _ => None,
                    })
                    .unwrap_or(rate.span);
                let id = encoding_id(context, *value, via.as_ref(), span);
                quote!(.bind_encoded(#formal_name, #reference, #id))
            });
            quote!(.rate_limit(#f::inspection::RateAttachment::named(#name) #(#binds)* #cost))
        }
        RateSource::Inline {
            id, dims, windows, ..
        } => {
            let id = lit(id);
            let dims = dims.iter().map(|dim| match dim {
                BoundDim::Origin => quote!(.origin()),
                BoundDim::Endpoint => quote!(.endpoint()),
                BoundDim::Custom {
                    label,
                    value,
                    via,
                    span,
                } => {
                    let label = lit(label);
                    let reference = value_ref(context, *value, from);
                    let encoder = match via {
                        Some(via) => {
                            let via = text(via);
                            quote!(::std::option::Option::Some(#via))
                        }
                        None => quote!(::std::option::Option::None),
                    };
                    let encoding = encoding_id(context, *value, via.as_ref(), *span);
                    quote!(.custom_encoded(#label, #reference, #encoder, ::std::option::Option::Some(#encoding)))
                }
            });
            let windows = match windows {
                Windows::Set(index) => {
                    let set = lit(&model.client.window_sets[*index].logical);
                    quote!(.using(#set))
                }
                Windows::Inline(list) => {
                    let list = list.iter().map(|window| {
                        let capacity = window.capacity;
                        let duration = duration_tokens(window.duration);
                        quote!(.window(#capacity, #duration))
                    });
                    quote!(#(#list)*)
                }
            };
            quote!(.rate_limit(#f::inspection::RateAttachment::inline(#id) #(#dims)* #windows #cost))
        }
    }
}

fn representation(
    context: &Context<'_>,
    choice: Option<&RepresentationChoice>,
    method: TokenStream,
) -> TokenStream {
    let f = context.f;
    match choice {
        None => TokenStream::new(),
        Some(RepresentationChoice::Select(ty)) => {
            let ty = text(ty);
            quote!(.#method(#f::inspection::RepresentationSelection::Type(#ty)))
        }
        Some(RepresentationChoice::Explicit) => {
            quote!(.#method(#f::inspection::RepresentationSelection::ExplicitFence))
        }
    }
}

fn owner_spec(context: &Context<'_>, owner: usize) -> TokenStream {
    let f = context.f;
    let model = context.model;
    let entry = &model.owners[owner];
    let site = Site::Owner(owner);
    let constructor = match entry.kind {
        OwnerKind::Client => {
            let name = lit(&entry.logical);
            quote!(#f::inspection::OwnerSpec::client(#name))
        }
        OwnerKind::Scope => {
            let path = lit(&entry.display);
            quote!(#f::inspection::OwnerSpec::scope(#path))
        }
    };
    let docs = doc_lines(&entry.meta);
    let deprecated = deprecation(context, &entry.meta);
    let values = entry.values.iter().map(|value| value_spec(context, value));
    let host = match &entry.host {
        None => TokenStream::new(),
        Some(LocalHost::Off) => quote!(.subdomain(#f::inspection::SubdomainOperation::Off)),
        Some(LocalHost::Prepend(labels)) => {
            let labels = labels.iter().map(|label| atom(context, &label.atom, site));
            quote!(.subdomain(#f::inspection::SubdomainOperation::prepend(::std::vec![#(#labels),*])))
        }
        Some(LocalHost::Replace(labels)) => {
            let labels = labels.iter().map(|label| atom(context, &label.atom, site));
            quote!(.subdomain(#f::inspection::SubdomainOperation::replace(::std::vec![#(#labels),*])))
        }
    };
    let path = path(context, entry.path.as_ref(), site);
    let program = program(context, &entry.program, site);
    let auth = auth(context, entry.auth.as_ref());
    let timeout = entry.timeout.map(|timeout| {
        let timeout = duration_tokens(timeout);
        quote!(.timeout(#timeout))
    });
    let request = representation(
        context,
        entry.request_choice.as_ref(),
        quote!(request_default),
    );
    let response = representation(
        context,
        entry.response_choice.as_ref(),
        quote!(response_default),
    );
    let rates = entry.rates.iter().map(|value| rate(context, value, site));
    quote! {
        #constructor .docs(#docs) #deprecated #(#values)* #host #path #program #auth #timeout #request #response #(#rates)*
    }
}

fn endpoint_spec(context: &Context<'_>, index: usize) -> TokenStream {
    let f = context.f;
    let p = context.p();
    let model = context.model;
    let entry = &model.endpoints[index];
    let site = Site::Endpoint(index);
    let path_text = lit(&entry.display);
    let method = lit(entry.method.as_str());
    let docs = doc_lines(&entry.meta);
    let deprecated = deprecation(context, &entry.meta);
    // Values and the body in declaration order.
    let mut parameters: Vec<(usize, TokenStream)> = entry
        .values
        .iter()
        .map(|value| (value.position, value_spec(context, value)))
        .collect();
    if let Some(body) = &entry.body {
        let name = lit(&body.logical);
        let ty_text = text(&body.ty);
        let ty = context.rust_type(&body.ty);
        let representation = context.rust_type(&body.representation);
        let optional = body.optional.then(|| quote!(.optional()));
        let via = body.via.as_ref().map(|via| {
            let via = text(via);
            quote!(.via(#via))
        });
        let page = entry
            .pagination
            .as_ref()
            .map(|_| quote!(.page_capable_for::<#representation, #ty>()));
        let body_docs = doc_lines(&body.meta);
        parameters.push((
            body.position,
            quote!(.body(#f::inspection::BodySpec::from_representation::<#representation, #ty>(#name, #ty_text) #optional #via #page .docs(#body_docs))),
        ));
    }
    parameters.sort_by_key(|(position, _)| *position);
    let parameters = parameters.into_iter().map(|(_, tokens)| tokens);
    let path = path(context, entry.path.as_ref(), site);
    let program = program(context, &entry.program, site);
    let auth = auth(context, entry.auth.as_ref());
    let timeout = entry.timeout.map(|timeout| {
        let timeout = duration_tokens(timeout);
        quote!(.timeout(#timeout))
    });
    let rates = entry.rates.iter().map(|value| rate(context, value, site));
    let id = entry.id_explicit.then(|| {
        let id = lit(&entry.id);
        quote!(.id(#id))
    });
    let output_text = text(&entry.output);
    let output = context.rust_type(&entry.output);
    let response = context.rust_type(&entry.response);
    let result = if entry.absence.is_some() {
        quote!(#f::inspection::ResultSpec::from_representation::<#response, <#output as #p::OptionalOutcome>::Present>(#output_text))
    } else {
        quote!(#f::inspection::ResultSpec::from_representation::<#response, #output>(#output_text))
    };
    let via = entry.output_via.as_ref().map(|via| {
        let via = text(via);
        quote!(.via(#via))
    });
    let status = if entry.status_explicit {
        match &entry.status {
            Status::Success => quote!(.status_success()),
            Status::Ranges(ranges) => {
                let ranges = ranges.iter().map(|(start, end)| {
                    if start == end {
                        quote!(#f::inspection::StatusRange::single(#start))
                    } else {
                        quote!(#f::inspection::StatusRange::range(#start, #end))
                    }
                });
                quote!(.status(::std::vec![#(#ranges),*]))
            }
        }
    } else {
        TokenStream::new()
    };
    let absence = entry
        .absence
        .as_ref()
        .map(|codes| quote!(.absent_on(&[#(#codes),*])));
    let effect = entry.effect_explicit.then(|| match entry.effect {
        EffectDecl::Safe => quote!(.effect(#f::inspection::Effect::Safe)),
        EffectDecl::Idempotent => quote!(.effect(#f::inspection::Effect::Idempotent)),
        EffectDecl::NonIdempotent => quote!(.effect(#f::inspection::Effect::NonIdempotent)),
    });
    let pagination = entry.pagination.as_ref().map(|pagination| {
        let controller = text(&pagination.controller);
        let init_type = text(&pagination.init_type);
        let fields = pagination.fields.iter().map(|(field, value)| {
            let field = lit(&crate::semantic::surface::canonical_ident(field));
            match value {
                InitValue::Value(key) => {
                    let reference = value_ref(context, *key, site);
                    quote!(.init_value(#field, #reference))
                }
                InitValue::Stable(StableValue::Path(path)) => {
                    let path = text(path);
                    quote!(.init_constant(#field, #path))
                }
                InitValue::Stable(stable) => {
                    let literal = text(&super::owner::stable_tokens(context, stable));
                    quote!(.init_literal(#field, #literal))
                }
            }
        });
        let cursor = pagination.cursor.iter().map(|index| {
            let reference = value_ref(context, VRef { site, index: *index }, site);
            quote!(.cursor(#reference))
        });
        quote!(.pagination(#f::inspection::PaginationContract::new(#controller, #init_type) #(#fields)* #(#cursor)*))
    });
    let expose = entry
        .expose_accepted_bytes
        .then(|| quote!(.expose_accepted_bytes()));
    let _ = Positional::Body;
    quote! {
        #f::inspection::EndpointSpec::new(#path_text, #method)
            .docs(#docs)
            #deprecated
            #(#parameters)*
            #path
            #program
            #auth
            #timeout
            #(#rates)*
            #id
            .result(#result #via)
            #status
            #absence
            #effect
            #pagination
            #expose
    }
}

/// The complete `ContractDescription::builder(...)...` expression (without
/// the final `.build()`).
pub(crate) fn contract_expr(context: &Context<'_>) -> TokenStream {
    let f = context.f;
    let model = context.model;
    let root = owner_spec(context, 0);
    let base = lit(&model.client.base.value());
    let secrets = model.client.secrets.iter().map(|secret| {
        let name = lit(&secret.logical);
        let docs = doc_lines(&secret.meta);
        quote!(.secret(#f::inspection::SecretDescription::new(#name).docs(#docs)))
    });
    let credentials = model.client.credentials.iter().map(|credential| {
        let name = lit(&credential.logical);
        let docs = doc_lines(&credential.meta);
        let secret = |index: &usize| lit(&model.client.secrets[*index].logical);
        let description = match &credential.source {
            CredentialSource::ApiKey { secret: index } => {
                let secret = secret(index);
                quote!(#f::inspection::CredentialDescription::api_key(#name, #secret))
            }
            CredentialSource::Bearer { secret: index } => {
                let secret = secret(index);
                quote!(#f::inspection::CredentialDescription::bearer(#name, #secret))
            }
            CredentialSource::Basic { username, password } => {
                let username = secret(username);
                let password = secret(password);
                quote!(#f::inspection::CredentialDescription::basic(#name, #username, #password))
            }
            CredentialSource::Endpoint { path_text, .. } => {
                let path = lit(path_text);
                quote!(#f::inspection::CredentialDescription::endpoint(#name, #path))
            }
            CredentialSource::Custom { provider } => {
                let provider = text(provider);
                quote!(#f::inspection::CredentialDescription::custom(#name, #provider))
            }
        };
        quote!(.credential(#description.docs(#docs)))
    });
    let window_sets = model.client.window_sets.iter().map(|set| {
        let name = lit(&set.logical);
        let docs = doc_lines(&set.meta);
        let windows = set.windows.iter().map(|window| {
            let capacity = window.capacity;
            let duration = duration_tokens(window.duration);
            quote!(#f::inspection::WindowDescription::new(#capacity, #duration))
        });
        quote!(.window_set(#f::inspection::WindowSetDescription::new(#name, ::std::vec![#(#windows),*]).docs(#docs)))
    });
    let policies = model.client.policies.iter().map(|policy| {
        let name = lit(&policy.logical);
        let id = lit(&policy.id);
        let docs = doc_lines(&policy.meta);
        let formals = policy.formals.iter().map(|formal| {
            let formal = lit(&crate::semantic::surface::canonical_ident(formal));
            quote!(.formal(#formal))
        });
        let dims = policy.dims.iter().map(|dim| match dim {
            FormalDim::Origin => quote!(.origin()),
            FormalDim::Endpoint => quote!(.endpoint()),
            FormalDim::Custom { label, formal, via } => {
                let label = lit(label);
                let formal = lit(&crate::semantic::surface::canonical_ident(&policy.formals[*formal]));
                match via {
                    Some(via) => {
                        let via = text(via);
                        quote!(.custom_via(#label, #formal, #via))
                    }
                    None => quote!(.custom(#label, #formal)),
                }
            }
        });
        let cost = policy.cost.map(|cost| quote!(.cost(#cost)));
        let windows = match &policy.windows {
            Windows::Set(index) => {
                let set = lit(&model.client.window_sets[*index].logical);
                quote!(.using(#set))
            }
            Windows::Inline(list) => {
                let list = list.iter().map(|window| {
                    let capacity = window.capacity;
                    let duration = duration_tokens(window.duration);
                    quote!(.window(#capacity, #duration))
                });
                quote!(#(#list)*)
            }
        };
        quote!(.policy(#f::inspection::PolicyDefinition::new(#name, #id) #(#formals)* #(#dims)* #cost #windows .docs(#docs)))
    });
    let scopes = (1..model.owners.len()).map(|owner| {
        let spec = owner_spec(context, owner);
        quote!(.scope(#spec))
    });
    let endpoints = (0..model.endpoints.len()).map(|endpoint| {
        let spec = endpoint_spec(context, endpoint);
        quote!(.endpoint(#spec))
    });
    let _ = docs::human_duration;
    quote! {
        #f::inspection::ContractDescription::builder(#root, #base)
            #(#secrets)*
            #(#credentials)*
            #(#window_sets)*
            #(#policies)*
            #(#scopes)*
            #(#endpoints)*
    }
}
