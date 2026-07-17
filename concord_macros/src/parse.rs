use std::collections::HashSet;

use proc_macro2::TokenStream;
use syn::parse::{Parse, ParseStream};
use syn::punctuated::Punctuated;
use syn::spanned::Spanned;
use syn::{
    GenericArgument, Ident, LitStr, PathArguments, Token, Type, TypePath, braced, bracketed,
    parenthesized,
};

use crate::model::{
    Api, Client, CodecKind, Endpoint, MethodKind, Parameter, PathAtom, ResponseKind, Scope,
    to_snake_case,
};

const RESERVED_GENERATED_METHODS: &[&str] = &[
    "new",
    "builder",
    "build",
    "http",
    "runtime",
    "credentials",
    "request",
    "response",
    "raw_response",
    "paginate",
    "timeout",
    "disable_timeout",
    "inherit_timeout",
    "debug",
];

mod keyword {
    syn::custom_keyword!(base);
    syn::custom_keyword!(client);
    syn::custom_keyword!(path);
    syn::custom_keyword!(scope);
}

pub(crate) fn parse(input: TokenStream) -> syn::Result<Api> {
    syn::parse2(input)
}

impl Parse for Api {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let client = input.parse()?;
        if !input.is_empty() {
            return Err(input.error(
                "the current generated subset accepts exactly one `client` declaration per `api!` invocation",
            ));
        }
        Ok(Self { client })
    }
}

impl Parse for Client {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        input.parse::<keyword::client>()?;
        let name: Ident = input.parse()?;
        let body;
        braced!(body in input);

        if !body.peek(keyword::base) {
            return Err(body.error("a client must begin with a fixed `base \"https://...\";`"));
        }
        body.parse::<keyword::base>()?;
        let base: LitStr = body.parse()?;
        body.parse::<Token![;]>()?;
        validate_base(&base)?;

        if !body.peek(keyword::scope) {
            return Err(body.error(
                "the current generated subset requires one scope after the fixed `base` declaration",
            ));
        }
        let scope = body.parse()?;
        if !body.is_empty() {
            return Err(body.error(
                "unsupported client declaration; the current generated subset accepts one fixed base and one scope",
            ));
        }

        Ok(Self { name, base, scope })
    }
}

impl Parse for Scope {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        input.parse::<keyword::scope>()?;
        let name: Ident = input.parse()?;
        let parameters_content;
        parenthesized!(parameters_content in input);
        let parameters = parse_required_parameters(&parameters_content, "scope")?;

        let body;
        braced!(body in input);
        if !body.peek(keyword::path) {
            return Err(body.error("a scope must begin with a `path [...]` declaration"));
        }
        let path = parse_path(&body)?;
        validate_path(&path, &parameters, PathContext::Scope)?;

        if body.is_empty() {
            return Err(body.error("a scope must contain at least one endpoint"));
        }
        let mut endpoints = Vec::new();
        let mut endpoint_names = HashSet::new();
        while !body.is_empty() {
            let endpoint = parse_endpoint(&body)?;
            if !endpoint_names.insert(endpoint.name.to_string()) {
                return Err(endpoint
                    .name
                    .span()
                    .error(format!("duplicate endpoint `{}`", endpoint.name)));
            }
            endpoints.push(endpoint);
        }

        validate_generated_names(&name, &endpoints)?;

        Ok(Self {
            name,
            parameters,
            path,
            endpoints,
        })
    }
}

fn validate_generated_names(scope_name: &Ident, endpoints: &[Endpoint]) -> syn::Result<()> {
    let scope_method = to_snake_case(&scope_name.to_string());
    if RESERVED_GENERATED_METHODS.contains(&scope_method.as_str()) {
        return Err(scope_name.span().error(format!(
            "generated scope method `{scope_method}` collides with a reserved Concord method"
        )));
    }

    let mut endpoint_methods = HashSet::new();
    for endpoint in endpoints {
        let method = to_snake_case(&endpoint.name.to_string());
        if RESERVED_GENERATED_METHODS.contains(&method.as_str()) {
            return Err(endpoint.name.span().error(format!(
                "generated endpoint method `{method}` collides with a reserved Concord method"
            )));
        }
        if !endpoint_methods.insert(method.clone()) {
            return Err(endpoint.name.span().error(format!(
                "generated endpoint method `{method}` collides after deterministic Rust name normalization"
            )));
        }
    }
    Ok(())
}

fn parse_endpoint(input: ParseStream<'_>) -> syn::Result<Endpoint> {
    let method: Ident = input
        .parse()
        .map_err(|_| input.error("unsupported endpoint declaration; expected `GET` or `POST`"))?;
    let method = match method.to_string().as_str() {
        "GET" => MethodKind::Get,
        "POST" => MethodKind::Post,
        _ => {
            return Err(method.span().error(
                "unsupported endpoint method; the Phase 3 generated subset accepts `GET` or `POST`",
            ));
        }
    };

    let name: Ident = input.parse()?;
    let parameters_content;
    parenthesized!(parameters_content in input);
    let (parameters, body) = parse_endpoint_parameters(&parameters_content)?;

    input.parse::<Token![->]>()?;
    let response = parse_response(input.parse()?)?;

    let path = if input.peek(Token![;]) {
        input.parse::<Token![;]>()?;
        Vec::new()
    } else {
        let body;
        braced!(body in input);
        if !body.peek(keyword::path) {
            return Err(body.error(
                "an endpoint block in the current generated subset must contain one `path [...]` declaration",
            ));
        }
        let path = parse_path(&body)?;
        if !body.is_empty() {
            return Err(body.error(
                "unsupported endpoint clause; the current generated subset accepts only one `path [...]` clause",
            ));
        }
        path
    };
    validate_path(&path, &parameters, PathContext::Endpoint)?;

    Ok(Endpoint {
        method,
        name,
        parameters,
        body,
        path,
        response,
    })
}

fn parse_endpoint_parameters(
    input: ParseStream<'_>,
) -> syn::Result<(Vec<Parameter>, Option<CodecKind>)> {
    let mut parameters = Vec::new();
    let mut body = None;
    let mut names = HashSet::new();

    while !input.is_empty() {
        let name: Ident = input.parse()?;
        input.parse::<Token![:]>()?;
        let ty: Type = input.parse()?;
        if !names.insert(name.to_string()) {
            return Err(name
                .span()
                .error(format!("duplicate endpoint parameter `{name}`")));
        }
        if input.peek(Token![=]) {
            return Err(input.error(
                "defaulted endpoint parameters are outside the current generated subset; use a required parameter",
            ));
        }

        if name == "body" {
            body = Some(parse_request_codec(ty)?);
        } else {
            if is_option(&ty) {
                return Err(ty.span().error(
                    "optional endpoint parameters are outside the current generated subset; use a required parameter",
                ));
            }
            parameters.push(Parameter { name, ty });
        }

        if input.is_empty() {
            break;
        }
        input.parse::<Token![,]>()?;
        if body.is_some() && !input.is_empty() {
            return Err(input.error("the request body must be the final endpoint parameter"));
        }
    }

    Ok((parameters, body))
}

fn parse_request_codec(ty: Type) -> syn::Result<CodecKind> {
    if is_named_marker(&ty, "NoContent") {
        return Err(ty
            .span()
            .error("`NoContent` is a response marker and cannot be used as a request body"));
    }
    if is_named_marker(&ty, "Bytes") {
        return Err(ty.span().error(
            "`Bytes` is not a request body marker; use `Text`, `Json<T>`, or a custom `EncodeBody` codec",
        ));
    }
    parse_buffered_codec(ty, "request")
}

fn parse_response(ty: Type) -> syn::Result<ResponseKind> {
    if is_named_marker(&ty, "Bytes") {
        let Type::Path(path) = &ty else {
            unreachable!("named markers are type paths")
        };
        ensure_no_arguments(path, "Bytes")?;
        return Ok(ResponseKind::Bytes);
    }
    if is_named_marker(&ty, "NoContent") {
        let Type::Path(path) = &ty else {
            unreachable!("named markers are type paths")
        };
        ensure_no_arguments(path, "NoContent")?;
        return Ok(ResponseKind::NoContent);
    }
    parse_buffered_codec(ty, "response").map(ResponseKind::Buffered)
}

fn parse_buffered_codec(ty: Type, role: &str) -> syn::Result<CodecKind> {
    let Type::Path(path) = &ty else {
        return Ok(CodecKind::Custom(ty));
    };
    if is_unqualified(path, "Text") {
        ensure_no_arguments(path, "Text")?;
        return Ok(CodecKind::Text);
    }
    if is_unqualified(path, "Json") {
        return json_inner(path).map(CodecKind::Json);
    }
    if is_unqualified(path, "Multipart") && role == "response" {
        return Err(ty
            .span()
            .error("`Multipart` is a request-only marker and cannot be used as a response"));
    }
    if is_unqualified(path, "Stream") || is_unqualified(path, "Multipart") {
        return Err(ty.span().error(format!(
            "{role} streaming and multipart markers are deferred to Phase 4"
        )));
    }
    Ok(CodecKind::Custom(ty))
}

fn is_named_marker(ty: &Type, expected: &str) -> bool {
    let Type::Path(path) = ty else {
        return false;
    };
    is_unqualified(path, expected)
}

fn is_unqualified(path: &TypePath, expected: &str) -> bool {
    path.qself.is_none()
        && path.path.leading_colon.is_none()
        && path.path.segments.len() == 1
        && path.path.segments[0].ident == expected
}

fn ensure_no_arguments(path: &TypePath, marker: &str) -> syn::Result<()> {
    let segment = &path.path.segments[0];
    if matches!(segment.arguments, PathArguments::None) {
        Ok(())
    } else {
        Err(segment
            .arguments
            .span()
            .error(format!("`{marker}` does not accept type arguments")))
    }
}

fn json_inner(path: &TypePath) -> syn::Result<Type> {
    let segment = &path.path.segments[0];
    let PathArguments::AngleBracketed(arguments) = &segment.arguments else {
        return Err(segment
            .span()
            .error("`Json` requires exactly one type argument"));
    };
    let mut values = arguments.args.iter();
    let Some(GenericArgument::Type(ty)) = values.next() else {
        return Err(arguments
            .span()
            .error("`Json` requires exactly one type argument"));
    };
    if values.next().is_some() {
        return Err(arguments
            .span()
            .error("`Json` requires exactly one type argument"));
    }
    Ok(ty.clone())
}

fn parse_required_parameters(input: ParseStream<'_>, owner: &str) -> syn::Result<Vec<Parameter>> {
    let mut parameters = Vec::new();
    let mut names = HashSet::new();

    while !input.is_empty() {
        let name: Ident = input.parse()?;
        input.parse::<Token![:]>()?;
        let ty: Type = input.parse()?;
        if is_option(&ty) {
            return Err(ty.span().error(format!(
                "optional {owner} parameters are outside the current generated subset; use a required parameter"
            )));
        }
        if input.peek(Token![=]) {
            return Err(input.error(format!(
                "defaulted {owner} parameters are outside the current generated subset; use a required parameter"
            )));
        }
        if !names.insert(name.to_string()) {
            return Err(name
                .span()
                .error(format!("duplicate {owner} parameter `{name}`")));
        }
        parameters.push(Parameter { name, ty });

        if input.is_empty() {
            break;
        }
        input.parse::<Token![,]>()?;
        if input.is_empty() {
            break;
        }
    }

    Ok(parameters)
}

fn is_option(ty: &Type) -> bool {
    let Type::Path(type_path) = ty else {
        return false;
    };
    type_path
        .path
        .segments
        .last()
        .is_some_and(|segment| segment.ident == "Option")
}

fn parse_path(input: ParseStream<'_>) -> syn::Result<Vec<PathAtom>> {
    input.parse::<keyword::path>()?;
    let atoms_content;
    bracketed!(atoms_content in input);
    let atoms = Punctuated::<PathAtom, Token![,]>::parse_terminated(&atoms_content)?;
    input.parse::<Token![;]>()?;
    if atoms.is_empty() {
        return Err(atoms_content.error("a path must contain at least one atom"));
    }
    Ok(atoms.into_iter().collect())
}

impl Parse for PathAtom {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        if input.peek(LitStr) {
            let literal: LitStr = input.parse()?;
            validate_static_segment(&literal)?;
            return Ok(Self::Static(literal));
        }

        let namespace: Ident = input.parse().map_err(|_| {
            input.error("unsupported path atom; use a string literal, `scope.NAME`, or `ep.NAME`")
        })?;
        input.parse::<Token![.]>()?;
        let name = input.parse()?;
        match namespace.to_string().as_str() {
            "scope" => Ok(Self::Scope(name)),
            "ep" => Ok(Self::Endpoint(name)),
            _ => Err(namespace
                .span()
                .error("unsupported path namespace; use only `scope.NAME` and `ep.NAME`")),
        }
    }
}

#[derive(Clone, Copy)]
enum PathContext {
    Scope,
    Endpoint,
}

fn validate_path(
    atoms: &[PathAtom],
    parameters: &[Parameter],
    context: PathContext,
) -> syn::Result<()> {
    let names: HashSet<_> = parameters
        .iter()
        .map(|parameter| parameter.name.to_string())
        .collect();
    let mut referenced = HashSet::new();

    for atom in atoms {
        let reference = match (context, atom) {
            (PathContext::Scope, PathAtom::Scope(name))
            | (PathContext::Endpoint, PathAtom::Endpoint(name)) => name,
            (PathContext::Scope, PathAtom::Endpoint(name)) => {
                return Err(name
                    .span()
                    .error("`ep.NAME` is not available in a scope path; use `scope.NAME`"));
            }
            (PathContext::Endpoint, PathAtom::Scope(name)) => {
                return Err(name.span().error(
                    "scope values belong in the enclosing scope path in the current generated subset",
                ));
            }
            (_, PathAtom::Static(_)) => continue,
        };

        if !names.contains(&reference.to_string()) {
            let namespace = match context {
                PathContext::Scope => "scope",
                PathContext::Endpoint => "endpoint",
            };
            return Err(reference
                .span()
                .error(format!("unknown {namespace} path value `{reference}`")));
        }
        referenced.insert(reference.to_string());
    }

    for parameter in parameters {
        if !referenced.contains(&parameter.name.to_string()) {
            let owner = match context {
                PathContext::Scope => "scope",
                PathContext::Endpoint => "endpoint",
            };
            return Err(parameter.name.span().error(format!(
                "required {owner} parameter `{}` must be used by its path in the current generated subset",
                parameter.name
            )));
        }
    }

    Ok(())
}

fn validate_base(base: &LitStr) -> syn::Result<()> {
    let value = base.value();
    let authority_and_path = value
        .strip_prefix("https://")
        .or_else(|| value.strip_prefix("http://"))
        .ok_or_else(|| {
            base.span()
                .error("the fixed base must begin with `https://` or `http://`")
        })?;
    let authority = authority_and_path.split('/').next().unwrap_or_default();
    if authority.is_empty() || authority.contains('@') || value.contains('?') || value.contains('#')
    {
        return Err(base
            .span()
            .error("the fixed base must have a host and no user information, query, or fragment"));
    }
    Ok(())
}

fn validate_static_segment(segment: &LitStr) -> syn::Result<()> {
    let value = segment.value();
    if value.is_empty()
        || value == "."
        || value == ".."
        || value.contains('/')
        || value.contains('\\')
    {
        return Err(segment.span().error(
            "a static path atom must be one non-empty, non-dot segment without `/` or `\\`",
        ));
    }
    Ok(())
}

trait SpanError {
    fn error(self, message: impl std::fmt::Display) -> syn::Error;
}

impl SpanError for proc_macro2::Span {
    fn error(self, message: impl std::fmt::Display) -> syn::Error {
        syn::Error::new(self, message)
    }
}

#[cfg(test)]
mod tests {
    use quote::quote;

    use super::parse;

    #[test]
    fn rejects_methods_outside_the_phase3_slice_with_focused_message() {
        let error = parse(quote! {
            client ProofApi {
                base "https://example.test";
                scope resources(scope_value: String) {
                    path ["scopes", scope.scope_value];
                    PUT ById(endpoint_value: String) -> NoContent {
                        path [ep.endpoint_value];
                    }
                }
            }
        })
        .err()
        .expect("PUT is outside the current generated subset");

        assert_eq!(
            error.to_string(),
            "unsupported endpoint method; the Phase 3 generated subset accepts `GET` or `POST`"
        );
    }

    #[test]
    fn accepts_text_json_and_custom_buffered_body_and_response_markers() {
        parse(quote! {
            client CodecApi {
                base "https://example.test";
                scope messages() {
                    path ["messages"];
                    POST TextRoundTrip(body: Text) -> Text;
                    POST JsonRoundTrip(body: Json<crate::Record>) -> Json<crate::Record>;
                    POST Custom(
                        id: String,
                        body: crate::codec::MessagePack<crate::Record>,
                    ) -> crate::codec::MessagePack<crate::Record> {
                        path [ep.id];
                    }
                }
            }
        })
        .expect("Phase 3 buffered grammar must parse");
    }

    #[test]
    fn request_body_must_be_last_and_no_content_is_response_only() {
        let error = parse(quote! {
            client CodecApi {
                base "https://example.test";
                scope messages() {
                    path ["messages"];
                    POST Invalid(body: Text, id: String) -> Text {
                        path [ep.id];
                    }
                }
            }
        })
        .err()
        .expect("body ordering must be checked");
        assert_eq!(
            error.to_string(),
            "the request body must be the final endpoint parameter"
        );

        let error = parse(quote! {
            client CodecApi {
                base "https://example.test";
                scope messages() {
                    path ["messages"];
                    POST Invalid(body: NoContent) -> Text;
                }
            }
        })
        .err()
        .expect("NoContent request bodies must be rejected");
        assert_eq!(
            error.to_string(),
            "`NoContent` is a response marker and cannot be used as a request body"
        );
    }

    #[test]
    fn rejects_trailing_unsupported_client_declaration() {
        let error = parse(quote! {
            client ProofApi {
                base "https://example.test";
                scope resources(scope_value: String) {
                    path ["scopes", scope.scope_value];
                    GET ById(endpoint_value: String) -> NoContent {
                        path [ep.endpoint_value];
                    }
                }
                var ignored: String;
            }
        })
        .err()
        .expect("unsupported input must not be silently ignored");

        assert_eq!(
            error.to_string(),
            "unsupported client declaration; the current generated subset accepts one fixed base and one scope"
        );
    }

    #[test]
    fn accepts_trailing_commas_in_scope_and_endpoint_parameters() {
        parse(quote! {
            client ProofApi {
                base "https://example.test";
                scope resources(
                    account: String,
                    region: String,
                ) {
                    path ["accounts", scope.account, "regions", scope.region];
                    GET ById(
                        endpoint_value: String,
                    ) -> NoContent {
                        path [ep.endpoint_value];
                    }
                }
            }
        })
        .expect("canonical trailing commas must be accepted");
    }

    #[test]
    fn accepts_zero_parameter_scope_multiple_response_modes_and_semicolon_endpoint() {
        parse(quote! {
            client ExampleApi {
                base "http://example.test/v1/";
                scope health() {
                    path ["health"];
                    GET Bytes() -> Bytes;
                    GET Ping() -> NoContent;
                    GET Item(id: String) -> Bytes {
                        path ["items", ep.id];
                    }
                }
            }
        })
        .expect("the retained bodyless grammar slice must parse");
    }

    #[test]
    fn rejects_endpoint_names_that_normalize_to_the_same_public_method() {
        let error = parse(quote! {
            client ExampleApi {
                base "http://example.test/";
                scope health() {
                    path ["health"];
                    GET HTTPStatus() -> NoContent;
                    GET HttpStatus() -> NoContent;
                }
            }
        })
        .err()
        .expect("normalized public endpoint names must not collide");

        assert_eq!(
            error.to_string(),
            "generated endpoint method `http_status` collides after deterministic Rust name normalization"
        );
    }
}
