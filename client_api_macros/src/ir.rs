use crate::parse;
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
    pub name: Ident,
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

    // validate user-defined endpoint names are unique
    {
        use std::collections::BTreeSet;
        let mut seen: BTreeSet<String> = BTreeSet::new();
        for ep in &endpoints {
            let n = ep.name.to_string();
            if !seen.insert(n.clone()) {
                return Err(syn::Error::new_spanned(
                    &ep.name,
                    format!("duplicate endpoint name `{}` (endpoint names must be unique)", n),
                ));
            }
        }
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
                    name: ed.name.clone(),
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


