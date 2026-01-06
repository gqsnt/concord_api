// Path: ".\\concord_macros\\src\\ir.rs"
use crate::parse;
use syn::{Expr, Ident, LitStr, Path, Type};

#[derive(Debug)]
pub struct Ir {
    pub client_name: Ident,
    pub scheme_ident: Ident,
    pub host: LitStr,
    pub client_timeout: Option<Expr>,
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

#[derive(Debug, Clone)]
pub enum IrPolicyNodeKind {
    Prefix,
    Path,
}

/// A single prefix/path node's policy, aligned with the routing tree.
#[derive(Debug, Clone)]
pub struct IrPolicyNode {
    pub kind: IrPolicyNodeKind,
    /// The node template: host template for `prefix`, path template for `path`.
    pub template: parse::RouteExpr,
    pub headers: Vec<parse::HeaderRule>,
    pub query: Vec<parse::QueryEntry>,
    pub timeout: Option<Expr>,
}

#[derive(Debug)]
pub struct IrEndpoint {
    pub name: Ident,
    pub method: Ident,

    pub full_host_prefix: Vec<parse::RouteExpr>,
    pub full_path_prefix: Vec<parse::RouteExpr>,
    pub endpoint_path: parse::RouteExpr,
    pub full_policy_prefix: Vec<IrPolicyNode>,
    pub headers: Vec<parse::HeaderRule>,
    pub query: Vec<parse::QueryEntry>,
    pub timeout: Option<Expr>,
    pub body: Option<IrCodecSpec>,
    pub paginate: Option<IrPaginateSpec>,
    pub resp: IrCodecSpec,
    pub map: Option<IrMapClause>,
}

#[derive(Debug, Clone)]
pub struct IrCodecSpec {
    pub codec: Path,
    pub ty: Type,
}

#[derive(Debug, Clone)]
pub struct IrPaginateSpec {
    pub paginator: Path,
    pub args: Vec<IrPaginateArg>,
}

#[derive(Debug, Clone)]
pub struct IrPaginateArg {
    pub key: Ident,
    pub value: Option<Expr>,
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
    let client_timeout = client.timeout.clone();
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

    let mut endpoints = Vec::new();
    let mut host_stack: Vec<parse::RouteExpr> = Vec::new();
    let mut path_stack: Vec<parse::RouteExpr> = Vec::new();

    let mut policy_stack: Vec<IrPolicyNode> = Vec::new();
    collect_nodes(
        &ast.tree,
        &mut host_stack,
        &mut path_stack,
        &mut policy_stack,
        &mut endpoints,
    )?;

    // Unique endpoint names
    {
        use std::collections::BTreeSet;
        let mut seen: BTreeSet<String> = BTreeSet::new();
        for ep in &endpoints {
            let n = ep.name.to_string();
            if !seen.insert(n.clone()) {
                return Err(syn::Error::new_spanned(
                    &ep.name,
                    format!(
                        "duplicate endpoint name `{}` (endpoint names must be unique)",
                        n
                    ),
                ));
            }
        }
    }

    Ok(Ir {
        client_name: client.name,
        scheme_ident,
        host,
        client_timeout,
        vars,
        client_headers: client.headers,
        endpoints,
    })
}

fn collect_nodes(
    nodes: &[parse::Node],
    host_stack: &mut Vec<parse::RouteExpr>,
    path_stack: &mut Vec<parse::RouteExpr>,
    policy_stack: &mut Vec<IrPolicyNode>,
    out: &mut Vec<IrEndpoint>,
) -> syn::Result<()> {
    for n in nodes {
        match n {
            parse::Node::Prefix(nb) => {
                host_stack.push(nb.template.clone());
                policy_stack.push(IrPolicyNode {
                    kind: IrPolicyNodeKind::Prefix,
                    template: nb.template.clone(),
                    headers: nb.headers.clone(),
                    query: nb.query.clone(),
                    timeout: nb.timeout.clone(),
                });
                collect_nodes(&nb.children, host_stack, path_stack, policy_stack, out)?;
                host_stack.pop();
                policy_stack.pop();
            }
            parse::Node::Path(nb) => {
                path_stack.push(nb.template.clone());
                policy_stack.push(IrPolicyNode {
                    kind: IrPolicyNodeKind::Path,
                    template: nb.template.clone(),
                    headers: nb.headers.clone(),
                    query: nb.query.clone(),
                    timeout: nb.timeout.clone(),
                });
                collect_nodes(&nb.children, host_stack, path_stack, policy_stack, out)?;
                path_stack.pop();
                policy_stack.pop();
            }
            parse::Node::Endpoint(ed) => {
                let body = ed.body.as_ref().map(|b| IrCodecSpec {
                    codec: b.codec.clone(),
                    ty: b.ty.clone(),
                });

                let paginate = ed.paginate.as_ref().map(|p| IrPaginateSpec {
                    paginator: p.paginator.clone(),
                    args: p
                        .args
                        .iter()
                        .map(|a| IrPaginateArg {
                            key: a.key.clone(),
                            value: a.value.clone(),
                        })
                        .collect(),
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
                    full_policy_prefix: policy_stack.clone(),
                    headers: ed.headers.clone(),
                    query: ed.query.clone(),
                    body,
                    paginate,
                    timeout: ed.timeout.clone(),
                    resp,
                    map,
                });
            }
        }
    }
    Ok(())
}
