use crate::parse;
use heck::ToUpperCamelCase;
use std::borrow::Cow;
use syn::{Expr, Ident, LitStr, Path, Type};
#[derive(Debug)]
pub struct Ir {
    pub client_name: Ident,
    pub scheme_ident: Ident, // https/http
    pub host: LitStr,

    pub vars: Vec<IrVar>,
    pub client_headers: Vec<parse::HeaderRule>,

    pub endpoints: Vec<IrEndpoint>,
}

#[derive(Debug, Clone)]
pub struct IrVar {
    pub name: Ident,
    pub optional: bool,
    pub ty: Type,
    pub default: Option<Expr>,
}

#[derive(Debug)]
pub struct IrEndpoint {
    pub name: Ident, // generated from method+path
    pub method: Ident,
    pub full_host_prefix: Vec<LitStr>, // list of prefix templates (in nesting order)
    pub full_path_prefix: Vec<LitStr>, // list of path templates (in nesting order)
    pub endpoint_path: LitStr,         // endpoint path template
    pub headers: Vec<parse::HeaderRule>,
    pub query: Vec<parse::QueryItem>,
    pub body: Option<IrCodecSpec>,
    pub resp: IrCodecSpec,
    pub map: Option<IrMapClause>,
}

#[derive(Debug, Clone)]
pub struct IrCodecSpec {
    pub codec: Path, // struct type path
    pub ty: Type,
}

#[derive(Debug, Clone)]
pub struct IrMapClause {
    pub out_ty: Type,
    pub expr: Expr,
}

pub fn lower(ast: parse::ApiFile) -> syn::Result<Ir> {
    let client = ast.client;

    let scheme_ident = client
        .scheme
        .unwrap_or_else(|| Ident::new("https", client.name.span()));
    let host = client.host;

    let mut vars = Vec::new();
    for p in client.params {
        vars.push(IrVar {
            name: p.name,
            optional: p.optional,
            ty: p.ty,
            default: p.default,
        });
    }

    // collect endpoints by traversing tree (prefix/path)
    let mut endpoints = Vec::new();
    let mut host_stack: Vec<LitStr> = Vec::new();
    let mut path_stack: Vec<LitStr> = Vec::new();
    collect_nodes(&ast.tree, &mut host_stack, &mut path_stack, &mut endpoints)?;

    // endpoint naming: simple, deterministic
    for ep in &mut endpoints {
        ep.name = make_endpoint_ident(&ep.method, &ep.full_path_prefix, &ep.endpoint_path)?;
    }

    Ok(Ir {
        client_name: client.name,
        scheme_ident,
        host,
        vars,
        client_headers: client.headers,
        endpoints,
    })
}

fn collect_nodes(
    nodes: &[parse::Node],
    host_stack: &mut Vec<LitStr>,
    path_stack: &mut Vec<LitStr>,
    out: &mut Vec<IrEndpoint>,
) -> syn::Result<()> {
    for n in nodes {
        match n {
            parse::Node::Prefix(nb) => {
                host_stack.push(nb.template.clone());
                collect_nodes(&nb.children, host_stack, path_stack, out)?;
                host_stack.pop();
            }
            parse::Node::Path(nb) => {
                path_stack.push(nb.template.clone());
                collect_nodes(&nb.children, host_stack, path_stack, out)?;
                path_stack.pop();
            }
            parse::Node::Endpoint(ed) => {
                let body = ed.body.as_ref().map(|b| IrCodecSpec {
                    codec: b.codec.clone(),
                    ty: b.ty.clone(),
                });
                let resp = IrCodecSpec {
                    codec: ed.resp.codec.clone(),
                    ty: ed.resp.ty.clone(),
                };
                let map = ed.map.as_ref().map(|m| IrMapClause {
                    out_ty: m.out_ty.clone(),
                    expr: m.expr.clone(),
                });

                out.push(IrEndpoint {
                    name: Ident::new("_", ed.method.span()),
                    method: ed.method.clone(),
                    full_host_prefix: host_stack.clone(),
                    full_path_prefix: path_stack.clone(),
                    endpoint_path: ed.path.clone(),
                    headers: ed.headers.clone(),
                    query: ed.query.clone(),
                    body,
                    resp,
                    map,
                });
            }
        }
    }
    Ok(())
}

fn make_endpoint_ident(
    method: &Ident,
    full_path_prefix: &[LitStr],
    endpoint_path: &LitStr,
) -> syn::Result<Ident> {
    // Heuristique simple (suffisante pour ton exemple) :
    // - base resource = dernier segment statique du path prefix (ex: "posts", "users")
    // - GET ""            => Get + ResourcePlural   (GetPosts)
    // - GET "{id}"        => Get + ResourceSingular (GetPost)
    // - GET "{id}/X"      => Get + ResourceSingular + X (GetPostComments)
    // - POST ""           => Create + ResourceSingular (CreatePost)
    // - GET users "{id}/posts" => GetUserPosts

    fn split_segments(s: &str) -> Vec<&str> {
        s.split('/')
            .map(|x| x.trim())
            .filter(|x| !x.is_empty())
            .collect()
    }
    fn is_placeholder(seg: &str) -> bool {
        seg.starts_with('{') && seg.ends_with('}')
    }
    fn strip_braces(seg: &str) -> Cow<'_, str> {
        if is_placeholder(seg) {
            let inner = seg.trim_matches('{').trim_matches('}').trim();
            let inner = inner.split('=').next().unwrap_or(inner).trim();
            let name = inner.split(':').next().unwrap_or(inner).trim();
            let name = name.trim_end_matches('?').trim();
            Cow::Owned(name.to_string())
        } else {
            Cow::Borrowed(seg)
        }
    }
    fn singularize(s: &str) -> String {
        let s = s.trim();
        if s.ends_with('s') && s.len() > 1 {
            s[..s.len() - 1].to_string()
        } else {
            s.to_string()
        }
    }
    fn camel(seg: &str) -> String {
        seg.to_lowercase().as_str().to_upper_camel_case()
    }

    // base resource = last static segment in full_path_prefix
    let mut prefix_segs: Vec<String> = Vec::new();
    for p in full_path_prefix {
        for s in split_segments(&p.value()) {
            if !is_placeholder(s) && !s.contains('{') {
                prefix_segs.push(s.to_string());
            }
        }
    }
    let resource = prefix_segs.last().map(|s| s.as_str()).unwrap_or("");

    let endpoint_path_value = endpoint_path.value();
    let ep_segs_raw = split_segments(&endpoint_path_value);
    let ep_segs: Vec<String> = ep_segs_raw
        .into_iter()
        .map(|s| strip_braces(s).into_owned())
        .collect();

    let m = method.to_string().to_ascii_uppercase();
    let is_collection = ep_segs.is_empty();
    let is_id = ep_segs
        .first()
        .map(|s| s == "id" || s.ends_with("_id"))
        .unwrap_or(false)
        && !is_collection;

    let name = match (m.as_str(), is_collection, is_id) {
        ("GET", true, _) => format!("Get{}", camel(resource)), // GetPosts / GetUsers
        ("POST", true, _) => format!("Create{}", camel(&singularize(resource))), // CreatePost / CreateUser
        ("GET", false, true) => {
            if ep_segs.len() == 1 {
                format!("Get{}", camel(&singularize(resource))) // GetPost / GetUser
            } else {
                let mut out = format!("Get{}", camel(&singularize(resource)));
                for tail in ep_segs.iter().skip(1) {
                    out.push_str(&camel(tail));
                }
                out
            }
        }
        _ => {
            // fallback unique : Method + full path (prefix + endpoint)
            let mut s = format!("{}_{}", method, resource);
            if !ep_segs.is_empty() {
                s.push('_');
                s.push_str(&ep_segs.join("_"));
            }
            s.replace(['/', '-', '.', ' '], "_")
        }
    };

    Ok(Ident::new(&name, method.span()))
}
