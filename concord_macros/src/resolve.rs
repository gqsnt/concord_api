use std::collections::HashMap;

use proc_macro2::Span;
use syn::spanned::Spanned;
use syn::{GenericArgument, Ident, PathArguments, Type, TypePath};

use crate::ast::{
    HeaderEntry, HeaderOperation, HostOperation, Layer, PathOperation, PublicExpr, QueryEntry,
    QueryOperation, SourceApi, SourceChild, SourceEndpoint, SourceScope, TimeoutOperation,
    ValueDecl,
};
use crate::model::{
    Api, Child, Client, Endpoint, NormalizedConfig, NormalizedHeader, NormalizedQuery,
    ResolvedExpr, ResolvedValue, Scope, Value, ValueReference, ValueShape,
};
use crate::names::{
    RESERVED_GENERATED_METHODS, canonical_ident, prelude_alias, public_ident, request_ext_name,
    snake_ident, to_pascal_case, to_snake_case,
};

#[derive(Clone)]
struct ScopeFrame {
    id: usize,
    values: Vec<Value>,
}

struct Resolver<'a> {
    client_name: &'a Ident,
    client_values: &'a [Value],
    next_scope_id: usize,
    next_endpoint_id: usize,
    prelude_aliases: &'a mut HashMap<String, Span>,
}

#[derive(Clone, Copy)]
enum ExpressionUse {
    Single,
    Query,
    Header,
}

pub(crate) fn resolve(source: SourceApi) -> syn::Result<Api> {
    validate_client_names(&source)?;
    let mut prelude_aliases = HashMap::new();
    let mut clients = Vec::new();

    for source_client in source.clients {
        let values = convert_values(source_client.values)?;
        validate_setter_namespace(&values, "client builder", true, true)?;

        let client_name = source_client.name;
        let client_canonical = canonical_ident(&client_name);
        let builder_name = Ident::new(&format!("{client_canonical}Builder"), client_name.span());
        let support_name = snake_ident(&client_name);
        let endpoint_capability_name =
            Ident::new(&format!("{client_canonical}Endpoint"), client_name.span());
        let values_type_name = Ident::new(
            &format!("__Concord{}Values", to_pascal_case(&client_canonical)),
            Span::mixed_site(),
        );

        let mut resolver = Resolver {
            client_name: &client_name,
            client_values: &values,
            next_scope_id: 0,
            next_endpoint_id: 0,
            prelude_aliases: &mut prelude_aliases,
        };
        let config = resolver.normalize_layer(
            &source_client.layer,
            &NormalizedConfig::default(),
            &[],
            None,
        )?;
        let children =
            resolver.resolve_children(source_client.children, &config, &[], &[], "client")?;
        validate_child_methods(&children, &[], "generated client", true)?;

        clients.push(Client {
            name: client_name,
            builder_name,
            support_name,
            endpoint_capability_name,
            values_type_name,
            base: source_client.base,
            values,
            retry: source_client.retry,
            config,
            children,
        });
    }

    Ok(Api { clients })
}

impl Resolver<'_> {
    fn resolve_children(
        &mut self,
        source: Vec<SourceChild>,
        parent: &NormalizedConfig,
        visible_scopes: &[ScopeFrame],
        scope_path: &[Ident],
        owner: &str,
    ) -> syn::Result<Vec<Child>> {
        let mut children = Vec::new();
        for child in source {
            children.push(match child {
                SourceChild::Scope(scope) => Child::Scope(Box::new(self.resolve_scope(
                    *scope,
                    parent,
                    visible_scopes,
                    scope_path,
                )?)),
                SourceChild::Endpoint(endpoint) => Child::Endpoint(self.resolve_endpoint(
                    endpoint,
                    parent,
                    visible_scopes,
                    scope_path,
                )?),
            });
        }
        validate_module_items(&children, owner)?;
        Ok(children)
    }

    fn resolve_scope(
        &mut self,
        source: SourceScope,
        parent: &NormalizedConfig,
        visible_scopes: &[ScopeFrame],
        scope_path: &[Ident],
    ) -> syn::Result<Scope> {
        let id = self.next_scope_id;
        self.next_scope_id += 1;
        let parameters = convert_values(source.parameters)?;
        validate_setter_namespace(&parameters, "scope facade", false, false)?;

        let mut frames = visible_scopes.to_vec();
        frames.push(ScopeFrame {
            id,
            values: parameters.clone(),
        });
        let config = self.normalize_layer(&source.layer, parent, &frames, None)?;
        let mut path = scope_path.to_vec();
        path.push(source.name.clone());
        let children = self.resolve_children(source.children, &config, &frames, &path, "scope")?;
        validate_child_methods(&children, &parameters, "scope facade", false)?;

        let method_name = snake_ident(&source.name);
        let module_name = method_name.clone();
        let facade_name = Ident::new(
            &format!("{}Facade", to_pascal_case(&canonical_ident(&source.name))),
            source.name.span(),
        );
        Ok(Scope {
            id,
            name: source.name,
            method_name,
            module_name,
            facade_name,
            parameters,
            config,
            children,
        })
    }

    fn resolve_endpoint(
        &mut self,
        source: SourceEndpoint,
        parent: &NormalizedConfig,
        visible_scopes: &[ScopeFrame],
        scope_path: &[Ident],
    ) -> syn::Result<Endpoint> {
        let id = self.next_endpoint_id;
        self.next_endpoint_id += 1;
        let parameters = convert_values(source.parameters)?;
        validate_endpoint_setters(&parameters, visible_scopes)?;

        let public_method_name = source.alias.as_ref().map_or_else(
            || snake_ident(&source.name),
            |alias| public_ident(&canonical_ident(alias), alias.span()),
        );
        let request_ext_name = request_ext_name(&source.name);
        let alias = prelude_alias(
            self.client_name,
            scope_path,
            source.alias.as_ref().unwrap_or(&public_method_name),
        );
        let alias_name = canonical_ident(&alias);
        if let Some(previous) = self
            .prelude_aliases
            .insert(alias_name.clone(), alias.span())
        {
            let mut error = alias.span().error(format!(
                "generated request-extension prelude alias `{alias_name}` collides deterministically"
            ));
            error.combine(previous.error("the first generated alias is here"));
            return Err(error);
        }

        let config =
            self.normalize_layer(&source.layer, parent, visible_scopes, Some(&parameters))?;
        Ok(Endpoint {
            id,
            method: source.method,
            name: source.name,
            alias: source.alias,
            public_method_name,
            request_ext_name,
            prelude_alias: alias,
            parameters,
            body: source.body,
            config,
            response: source.response,
        })
    }

    fn normalize_layer(
        &self,
        layer: &Layer,
        parent: &NormalizedConfig,
        scopes: &[ScopeFrame],
        endpoint: Option<&[Value]>,
    ) -> syn::Result<NormalizedConfig> {
        let mut normalized = parent.clone();

        if let Some(operation) = &layer.host {
            normalized.host = match operation {
                HostOperation::Off => Vec::new(),
                HostOperation::Replace(values) => {
                    self.resolve_expressions(values, scopes, endpoint, ExpressionUse::Single)?
                }
            };
        }
        if let Some(operation) = &layer.path {
            match operation {
                PathOperation::Off => normalized.path.clear(),
                PathOperation::Replace(values) => {
                    normalized.path =
                        self.resolve_expressions(values, scopes, endpoint, ExpressionUse::Single)?;
                }
                PathOperation::Append(values) => {
                    normalized.path.extend(self.resolve_expressions(
                        values,
                        scopes,
                        endpoint,
                        ExpressionUse::Single,
                    )?);
                }
            }
        }
        if let Some(operation) = &layer.query {
            if matches!(operation, QueryOperation::Off | QueryOperation::Replace(_)) {
                normalized.query.clear();
            }
            if let QueryOperation::Modify(entries) | QueryOperation::Replace(entries) = operation {
                for entry in entries {
                    let key = match entry {
                        QueryEntry::Assign { key, .. } | QueryEntry::Remove(key) => key.value(),
                    };
                    normalized.query.retain(|entry| entry.key.value() != key);
                    if let QueryEntry::Assign { key, value } = entry {
                        normalized.query.push(NormalizedQuery {
                            key: key.clone(),
                            value: self.resolve_expr(
                                value,
                                scopes,
                                endpoint,
                                ExpressionUse::Query,
                            )?,
                        });
                    }
                }
            }
        }
        if let Some(operation) = &layer.headers {
            if matches!(
                operation,
                HeaderOperation::Off | HeaderOperation::Replace(_)
            ) {
                normalized.headers.clear();
            }
            if let HeaderOperation::Modify(entries) | HeaderOperation::Replace(entries) = operation
            {
                for entry in entries {
                    let name = match entry {
                        HeaderEntry::Assign { name, .. } | HeaderEntry::Remove(name) => {
                            name.value().to_ascii_lowercase()
                        }
                    };
                    normalized
                        .headers
                        .retain(|entry| entry.canonical_name != name);
                    if let HeaderEntry::Assign {
                        name: literal,
                        value,
                    } = entry
                    {
                        normalized.headers.push(NormalizedHeader {
                            name: literal.clone(),
                            canonical_name: name,
                            value: self.resolve_expr(
                                value,
                                scopes,
                                endpoint,
                                ExpressionUse::Header,
                            )?,
                        });
                    }
                }
            }
        }
        if let Some(operation) = &layer.timeout {
            normalized.timeout = match operation {
                TimeoutOperation::Off => None,
                TimeoutOperation::Set(value) => {
                    let _ = value.span;
                    Some(value.value)
                }
            };
        }
        Ok(normalized)
    }

    fn resolve_expressions(
        &self,
        values: &[PublicExpr],
        scopes: &[ScopeFrame],
        endpoint: Option<&[Value]>,
        usage: ExpressionUse,
    ) -> syn::Result<Vec<ResolvedExpr>> {
        values
            .iter()
            .map(|value| self.resolve_expr(value, scopes, endpoint, usage))
            .collect()
    }

    fn resolve_expr(
        &self,
        value: &PublicExpr,
        scopes: &[ScopeFrame],
        endpoint: Option<&[Value]>,
        usage: ExpressionUse,
    ) -> syn::Result<ResolvedExpr> {
        match value {
            PublicExpr::String(value) => Ok(ResolvedExpr::String(value.clone())),
            PublicExpr::Integer(value) => Ok(ResolvedExpr::Integer(value.clone())),
            PublicExpr::Format { span, values } => {
                let _ = span;
                Ok(ResolvedExpr::Format(self.resolve_expressions(
                    values,
                    scopes,
                    endpoint,
                    ExpressionUse::Single,
                )?))
            }
            PublicExpr::Reference { namespace, name } => {
                let reference = match namespace.to_string().as_str() {
                    "vars" => {
                        let index = find_unique_value(self.client_values, name, "client value")?;
                        ResolvedValue {
                            reference: ValueReference::Client(index),
                            shape: self.client_values[index].shape.clone(),
                        }
                    }
                    "scope" => {
                        let matches = scopes
                            .iter()
                            .filter_map(|scope| {
                                find_value(&scope.values, name).map(|parameter| (scope, parameter))
                            })
                            .collect::<Vec<_>>();
                        if matches.is_empty() {
                            return Err(name.span().error(format!(
                                "unknown visible scope value `{}`",
                                canonical_ident(name)
                            )));
                        }
                        if matches.len() != 1 {
                            return Err(name.span().error(format!(
                                "ambiguous visible scope value `{}`; qualify the DSL model so the name is unique",
                                canonical_ident(name)
                            )));
                        }
                        let (scope, parameter) = matches[0];
                        ResolvedValue {
                            reference: ValueReference::Scope {
                                scope_id: scope.id,
                                parameter,
                            },
                            shape: scope.values[parameter].shape.clone(),
                        }
                    }
                    "ep" => {
                        let Some(endpoint) = endpoint else {
                            return Err(name
                                .span()
                                .error("`ep.NAME` is available only while shaping an endpoint"));
                        };
                        let index = find_unique_value(endpoint, name, "endpoint value")?;
                        ResolvedValue {
                            reference: ValueReference::Endpoint(index),
                            shape: endpoint[index].shape.clone(),
                        }
                    }
                    _ => unreachable!("the parser restricts public-expression namespaces"),
                };
                match usage {
                    ExpressionUse::Single if !matches!(&reference.shape, ValueShape::Scalar) => {
                        return Err(name.span().error(
                            "host, path, and formatted expressions require one scalar value",
                        ));
                    }
                    ExpressionUse::Header
                        if matches!(
                            &reference.shape,
                            ValueShape::Vec { .. } | ValueShape::OptionVec { .. }
                        ) =>
                    {
                        return Err(name
                            .span()
                            .error("public header declarations do not accept vector values"));
                    }
                    ExpressionUse::Single | ExpressionUse::Query | ExpressionUse::Header => {}
                }
                Ok(ResolvedExpr::Value(reference))
            }
        }
    }
}

fn convert_values(values: Vec<ValueDecl>) -> syn::Result<Vec<Value>> {
    values
        .into_iter()
        .map(|value| {
            let shape = value_shape(&value.ty)?;
            Ok(Value {
                name: value.name,
                ty: value.ty,
                kind: value.kind,
                shape,
            })
        })
        .collect()
}

fn value_shape(ty: &Type) -> syn::Result<ValueShape> {
    if let Some(inner) = single_outer_argument(ty, "Option")? {
        if let Some(element) = single_outer_argument(&inner, "Vec")? {
            return Ok(ValueShape::OptionVec { inner, element });
        }
        return Ok(ValueShape::Option { inner });
    }
    if let Some(element) = single_outer_argument(ty, "Vec")? {
        return Ok(ValueShape::Vec { element });
    }
    Ok(ValueShape::Scalar)
}

fn single_outer_argument(ty: &Type, expected: &str) -> syn::Result<Option<Type>> {
    let Type::Path(TypePath { qself: None, path }) = ty else {
        return Ok(None);
    };
    if path.leading_colon.is_some()
        || path.segments.len() != 1
        || path.segments[0].ident != expected
    {
        return Ok(None);
    }
    let PathArguments::AngleBracketed(arguments) = &path.segments[0].arguments else {
        return Err(ty
            .span()
            .error(format!("`{expected}` requires exactly one type argument")));
    };
    let mut arguments = arguments.args.iter();
    let Some(GenericArgument::Type(inner)) = arguments.next() else {
        return Err(ty
            .span()
            .error(format!("`{expected}` requires exactly one type argument")));
    };
    if arguments.next().is_some() {
        return Err(ty
            .span()
            .error(format!("`{expected}` requires exactly one type argument")));
    }
    Ok(Some(inner.clone()))
}

fn find_value(values: &[Value], name: &Ident) -> Option<usize> {
    let name = canonical_ident(name);
    values
        .iter()
        .position(|value| canonical_ident(&value.name) == name)
}

fn find_unique_value(values: &[Value], name: &Ident, owner: &str) -> syn::Result<usize> {
    find_value(values, name).ok_or_else(|| {
        name.span()
            .error(format!("unknown {owner} `{}`", canonical_ident(name)))
    })
}

fn validate_client_names(source: &SourceApi) -> syn::Result<()> {
    let mut invocation_items = HashMap::new();
    for client in &source.clients {
        let client_name = canonical_ident(&client.name);
        insert_name(
            &mut invocation_items,
            client_name.clone(),
            client.name.span(),
            "invocation-scope client, builder, or support-module",
        )?;
        insert_name(
            &mut invocation_items,
            format!("{client_name}Builder"),
            client.name.span(),
            "invocation-scope client, builder, or support-module",
        )?;
        insert_name(
            &mut invocation_items,
            to_snake_case(&client_name),
            client.name.span(),
            "invocation-scope client, builder, or support-module",
        )?;
    }
    Ok(())
}

fn validate_setter_namespace(
    values: &[Value],
    owner: &str,
    include_required: bool,
    reserve_builder_methods: bool,
) -> syn::Result<()> {
    let mut names = HashMap::new();
    if reserve_builder_methods {
        for name in RESERVED_GENERATED_METHODS {
            names.insert((*name).to_owned(), Span::call_site());
        }
    }
    for value in values {
        if include_required || !value.is_required() {
            insert_name(
                &mut names,
                canonical_ident(&value.name),
                value.name.span(),
                owner,
            )?;
        }
        if value.is_optional() {
            insert_name(
                &mut names,
                format!("clear_{}", canonical_ident(&value.name)),
                value.name.span(),
                owner,
            )?;
        } else if value.default().is_some() {
            insert_name(
                &mut names,
                format!("reset_{}", canonical_ident(&value.name)),
                value.name.span(),
                owner,
            )?;
        }
    }
    Ok(())
}

fn validate_endpoint_setters(parameters: &[Value], scopes: &[ScopeFrame]) -> syn::Result<()> {
    let mut values = scopes
        .iter()
        .flat_map(|scope| scope.values.iter())
        .filter(|value| !value.is_required())
        .cloned()
        .collect::<Vec<_>>();
    values.extend(
        parameters
            .iter()
            .filter(|value| !value.is_required())
            .cloned(),
    );
    validate_setter_namespace(&values, "explicit endpoint controls", false, true)?;
    validate_setter_namespace(parameters, "endpoint request controls", false, true)
}

fn validate_child_methods(
    children: &[Child],
    local_values: &[Value],
    owner: &str,
    reserve_all: bool,
) -> syn::Result<()> {
    let mut names = HashMap::new();
    if reserve_all {
        for reserved in RESERVED_GENERATED_METHODS {
            names.insert((*reserved).to_owned(), Span::call_site());
        }
    }
    for value in local_values.iter().filter(|value| !value.is_required()) {
        insert_name(
            &mut names,
            canonical_ident(&value.name),
            value.name.span(),
            owner,
        )?;
        let helper = if value.is_optional() {
            format!("clear_{}", canonical_ident(&value.name))
        } else {
            format!("reset_{}", canonical_ident(&value.name))
        };
        insert_name(&mut names, helper, value.name.span(), owner)?;
    }
    for child in children {
        let (name, span) = match child {
            Child::Scope(scope) => (canonical_ident(&scope.method_name), scope.name.span()),
            Child::Endpoint(endpoint) => (
                canonical_ident(&endpoint.public_method_name),
                endpoint.name.span(),
            ),
        };
        if RESERVED_GENERATED_METHODS.contains(&name.as_str()) {
            return Err(span.error(format!(
                "generated {owner} method `{name}` collides with a reserved Concord method"
            )));
        }
        insert_name(&mut names, name, span, owner)?;
    }
    Ok(())
}

fn validate_module_items(children: &[Child], owner: &str) -> syn::Result<()> {
    let mut items = HashMap::new();
    for child in children {
        match child {
            Child::Scope(scope) => {
                insert_name(
                    &mut items,
                    canonical_ident(&scope.module_name),
                    scope.name.span(),
                    &format!("{owner} generated module"),
                )?;
            }
            Child::Endpoint(endpoint) => {
                insert_name(
                    &mut items,
                    canonical_ident(&endpoint.name),
                    endpoint.name.span(),
                    &format!("{owner} explicit endpoint module"),
                )?;
                if endpoint.parameters.iter().any(|value| !value.is_required()) {
                    insert_name(
                        &mut items,
                        canonical_ident(&endpoint.request_ext_name),
                        endpoint.name.span(),
                        &format!("{owner} endpoint request extension"),
                    )?;
                }
            }
        }
    }
    Ok(())
}

fn insert_name(
    names: &mut HashMap<String, Span>,
    name: String,
    span: Span,
    owner: &str,
) -> syn::Result<()> {
    if let Some(previous) = names.insert(name.clone(), span) {
        let mut error = span.error(format!(
            "generated {owner} name `{name}` collides after deterministic normalization"
        ));
        error.combine(previous.error("the first generated name is here"));
        return Err(error);
    }
    Ok(())
}

trait SpanError {
    fn error(self, message: impl std::fmt::Display) -> syn::Error;
}

impl SpanError for Span {
    fn error(self, message: impl std::fmt::Display) -> syn::Error {
        syn::Error::new(self, message)
    }
}

#[cfg(test)]
mod tests {
    use quote::quote;

    use super::*;
    use crate::parse;

    fn resolve_ok(tokens: proc_macro2::TokenStream) -> Api {
        resolve(parse::parse(tokens).expect("AST")).expect("normalized IR")
    }

    #[test]
    fn normalization_removes_layer_operations_and_preserves_order() {
        let api = resolve_ok(quote! {
            client ShapeApi {
                base "https://example.test/root";
                var tenant: String;
                path ["v1"];
                query { "first" = "one"; "replace" = "old"; }
                headers { "X-First" = "one"; }
                scope users(region: String) {
                    path replace ["users", scope.region];
                    query { remove "replace"; "second" = vars.tenant; }
                    headers replace { "X-Second" = scope.region; }
                    GET ById(id: String) -> Bytes { path [ep.id]; timeout 2s; }
                }
            }
        });
        let Child::Scope(scope) = &api.clients[0].children[0] else {
            panic!("scope")
        };
        let Child::Endpoint(endpoint) = &scope.children[0] else {
            panic!("endpoint")
        };
        assert_eq!(endpoint.config.path.len(), 3);
        assert_eq!(
            endpoint
                .config
                .query
                .iter()
                .map(|entry| entry.key.value())
                .collect::<Vec<_>>(),
            ["first", "second"]
        );
        assert_eq!(endpoint.config.headers.len(), 1);
        assert_eq!(
            endpoint.config.timeout,
            Some(std::time::Duration::from_secs(2))
        );
    }

    #[test]
    fn lexical_resolution_rejects_ambiguous_visible_scope_names() {
        let error = resolve(
            parse::parse(quote! {
                client Api {
                    base "https://example.test";
                    scope outer(id: String) {
                        scope inner(id: String) {
                            GET Read() -> Bytes { path [scope.id]; }
                        }
                    }
                }
            })
            .expect("AST"),
        )
        .err()
        .expect("ambiguous scope name");
        assert!(error.to_string().contains("ambiguous visible scope value"));
    }

    #[test]
    fn prelude_aliases_include_client_scope_and_endpoint_identity() {
        let api = resolve_ok(quote! {
            client ExampleApi {
                base "https://example.test";
                scope regions(region: String) {
                    scope users(tenant: String) {
                        GET ById(verbose: Option<bool>) as fetch -> Bytes;
                    }
                }
            }
        });
        let Child::Scope(regions) = &api.clients[0].children[0] else {
            panic!()
        };
        let Child::Scope(users) = &regions.children[0] else {
            panic!()
        };
        let Child::Endpoint(endpoint) = &users.children[0] else {
            panic!()
        };
        assert_eq!(
            endpoint.prelude_alias,
            "ExampleApiRegionsUsersFetchRequestExt"
        );
    }

    #[test]
    fn normalization_resolves_replace_remove_off_and_type_shapes() {
        let api = resolve_ok(quote! {
            client LayerApi {
                base "https://example.test/fixed";
                host ["client"];
                path ["client"];
                query { "keep" = "client"; "change" = "client"; }
                headers { "X-Keep" = "client"; "X-Change" = "client"; }
                timeout 5s;
                scope shaped() {
                    host off;
                    path replace ["scope"];
                    query {
                        "change" = "scope";
                        "local" = "scope";
                    }
                    headers {
                        "x-change" = "scope";
                        "X-Local" = "scope";
                    }
                    timeout off;
                    GET Modified(
                        scalar: String,
                        optional: Option<String>,
                        vector: Vec<String>,
                        optional_vector: Option<Vec<String>>,
                    ) -> Bytes;
                    GET Cleared() -> Bytes {
                        host ["endpoint"];
                        path off;
                        query off;
                        headers off;
                        timeout 1ms;
                    }
                    GET Replaced() -> Bytes {
                        query replace { "only" = "endpoint"; }
                        headers replace { "X-Only" = "endpoint"; }
                    }
                }
            }
        });
        let Child::Scope(scope) = &api.clients[0].children[0] else {
            panic!("scope")
        };
        let Child::Endpoint(modified) = &scope.children[0] else {
            panic!("modified endpoint")
        };
        assert!(modified.config.host.is_empty());
        assert_eq!(modified.config.path.len(), 1);
        assert_eq!(
            modified
                .config
                .query
                .iter()
                .map(|entry| entry.key.value())
                .collect::<Vec<_>>(),
            ["keep", "change", "local"]
        );
        assert_eq!(
            modified
                .config
                .headers
                .iter()
                .map(|entry| entry.canonical_name.as_str())
                .collect::<Vec<_>>(),
            ["x-keep", "x-change", "x-local"]
        );
        assert_eq!(modified.config.timeout, None);
        assert!(matches!(modified.parameters[0].shape, ValueShape::Scalar));
        assert!(matches!(
            modified.parameters[1].shape,
            ValueShape::Option { .. }
        ));
        assert!(matches!(
            modified.parameters[2].shape,
            ValueShape::Vec { .. }
        ));
        assert!(matches!(
            modified.parameters[3].shape,
            ValueShape::OptionVec { .. }
        ));

        let Child::Endpoint(cleared) = &scope.children[1] else {
            panic!("cleared endpoint")
        };
        assert_eq!(cleared.config.host.len(), 1);
        assert!(cleared.config.path.is_empty());
        assert!(cleared.config.query.is_empty());
        assert!(cleared.config.headers.is_empty());
        assert_eq!(
            cleared.config.timeout,
            Some(std::time::Duration::from_millis(1))
        );

        let Child::Endpoint(replaced) = &scope.children[2] else {
            panic!("replaced endpoint")
        };
        assert_eq!(replaced.config.query[0].key.value(), "only");
        assert_eq!(replaced.config.query.len(), 1);
        assert_eq!(replaced.config.headers[0].canonical_name, "x-only");
        assert_eq!(replaced.config.headers.len(), 1);
    }

    #[test]
    fn semantic_references_and_header_vector_use_fail_with_focused_diagnostics() {
        let unknown = resolve(
            parse::parse(quote! {
                client Api {
                    base "https://example.test";
                    GET Read() -> Bytes { path [ep.missing]; }
                }
            })
            .expect("AST"),
        )
        .err()
        .expect("unknown endpoint value");
        assert!(
            unknown
                .to_string()
                .contains("unknown endpoint value `missing`")
        );

        let vector_header = resolve(
            parse::parse(quote! {
                client Api {
                    base "https://example.test";
                    GET Read(values: Vec<String>) -> Bytes {
                        headers { "X-Values" = ep.values; }
                    }
                }
            })
            .expect("AST"),
        )
        .err()
        .expect("vector header");
        assert!(
            vector_header
                .to_string()
                .contains("do not accept vector values")
        );

        let secret_namespace = parse::parse(quote! {
            client Api {
                base "https://example.test";
                GET Read() -> Bytes { query { "token" = credentials.token; } }
            }
        })
        .err()
        .expect("credential expression namespace");
        assert!(
            secret_namespace
                .to_string()
                .contains("secrets and credentials are not expressible")
        );
    }

    #[test]
    fn generated_name_collisions_cover_aliases_traits_and_generic_request_methods() {
        let support = resolve(
            parse::parse(quote! {
                client lower_api { base "https://example.test"; }
            })
            .expect("AST"),
        )
        .err()
        .expect("client/support module collision");
        assert!(support.to_string().contains("lower_api"));
        assert!(support.to_string().contains("collides"));

        let alias = resolve(
            parse::parse(quote! {
                client Api {
                    base "https://example.test";
                    GET First() as same -> Bytes;
                    GET Second() as same -> Bytes;
                }
            })
            .expect("AST"),
        )
        .err()
        .expect("alias collision");
        let alias = alias.to_string().to_ascii_lowercase();
        assert!(alias.contains("same"));
        assert!(alias.contains("collides"));

        let trait_item = resolve(
            parse::parse(quote! {
                client Api {
                    base "https://example.test";
                    GET Read(value: Option<String>) -> Bytes;
                    GET ReadRequestExt() -> Bytes;
                }
            })
            .expect("AST"),
        )
        .err()
        .expect("request extension trait collision");
        assert!(trait_item.to_string().contains("collides"));

        let request_method = resolve(
            parse::parse(quote! {
                client Api {
                    base "https://example.test";
                    GET Read(timeout: Option<u64>) -> Bytes;
                }
            })
            .expect("AST"),
        )
        .err()
        .expect("generic request method collision");
        assert!(request_method.to_string().contains("timeout"));
    }
}
