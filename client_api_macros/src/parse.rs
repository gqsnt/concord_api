use syn::parse::{Parse, ParseStream};
use syn::{Expr, Ident, LitStr, Path, Result, Token, Type, braced};

syn::custom_keyword!(client);
syn::custom_keyword!(scheme);
syn::custom_keyword!(host);
syn::custom_keyword!(params);
syn::custom_keyword!(headers);
syn::custom_keyword!(prefix);
syn::custom_keyword!(path);
syn::custom_keyword!(query);
syn::custom_keyword!(body);

#[derive(Debug)]
pub struct ApiFile {
    pub client: ClientDecl,
    pub tree: Vec<Node>,
}

impl Parse for ApiFile {
    fn parse(input: ParseStream) -> Result<Self> {
        let client: ClientDecl = input.parse()?;

        let mut tree = Vec::new();
        while !input.is_empty() {
            tree.push(input.parse()?);
        }

        Ok(Self { client, tree })
    }
}

#[derive(Debug)]
pub struct ClientDecl {
    pub name: Ident,
    pub scheme: Option<Ident>,
    pub host: LitStr,
    pub params: Vec<ParamDecl>,
    pub headers: Vec<HeaderRule>,
}

impl Parse for ClientDecl {
    fn parse(input: ParseStream) -> Result<Self> {
        input.parse::<client>()?;
        let name: Ident = input.parse()?;
        let content;
        braced!(content in input);

        let mut scheme_v: Option<Ident> = None;
        let mut host_v: Option<LitStr> = None;
        let mut params_v: Vec<ParamDecl> = Vec::new();
        let mut headers_v: Vec<HeaderRule> = Vec::new();

        while !content.is_empty() {
            if content.peek(scheme) {
                content.parse::<scheme>()?;
                content.parse::<Token![:]>()?;
                let sc: Ident = content.parse()?;
                scheme_v = Some(sc);
                let _ = content.parse::<Token![,]>();
                continue;
            }
            if content.peek(host) {
                content.parse::<host>()?;
                content.parse::<Token![:]>()?;
                let h: LitStr = content.parse()?;
                host_v = Some(h);
                let _ = content.parse::<Token![,]>();
                continue;
            }
            if content.peek(params) {
                let pb: ParamsBlock = content.parse()?;
                params_v = pb.params;
                continue;
            }
            if content.peek(headers) {
                let hb: HeadersBlock = content.parse()?;
                headers_v = hb.rules;
                continue;
            }

            return Err(content.error("unexpected token in client block"));
        }

        let host =
            host_v.ok_or_else(|| syn::Error::new_spanned(&name, "client.host is required"))?;

        Ok(Self {
            name,
            scheme: scheme_v,
            host,
            params: params_v,
            headers: headers_v,
        })
    }
}

#[derive(Debug)]
pub struct ParamsBlock {
    pub params: Vec<ParamDecl>,
}

impl Parse for ParamsBlock {
    fn parse(input: ParseStream) -> Result<Self> {
        input.parse::<params>()?;
        let content;
        braced!(content in input);

        let mut out = Vec::new();
        while !content.is_empty() {
            out.push(content.parse()?);
            // allow optional trailing ';' or ','
            if content.peek(Token![;]) {
                content.parse::<Token![;]>()?;
            } else {
                let _ = content.parse::<Token![,]>();
            }
        }
        Ok(Self { params: out })
    }
}

#[derive(Debug, Clone)]
pub struct ParamDecl {
    pub name: Ident,
    pub optional: bool, // name?: T
    pub ty: Type,
    pub default: Option<Expr>, // name: T = expr
}

impl Parse for ParamDecl {
    fn parse(input: ParseStream) -> Result<Self> {
        let name: Ident = input.parse()?;
        let optional = if input.peek(Token![?]) {
            input.parse::<Token![?]>()?;
            true
        } else {
            false
        };
        input.parse::<Token![:]>()?;
        let ty: Type = input.parse()?;
        let default = if input.peek(Token![=]) {
            input.parse::<Token![=]>()?;
            Some(input.parse::<Expr>()?)
        } else {
            None
        };
        Ok(Self {
            name,
            optional,
            ty,
            default,
        })
    }
}

#[derive(Debug)]
pub struct HeadersBlock {
    pub rules: Vec<HeaderRule>,
}

impl Parse for HeadersBlock {
    fn parse(input: ParseStream) -> Result<Self> {
        input.parse::<headers>()?;
        let content;
        braced!(content in input);

        let mut rules = Vec::new();
        while !content.is_empty() {
            rules.push(content.parse()?);
            if content.peek(Token![;]) {
                content.parse::<Token![;]>()?;
            } else {
                let _ = content.parse::<Token![,]>();
            }
        }
        Ok(Self { rules })
    }
}

#[derive(Debug, Clone)]
pub enum HeaderRule {
    Remove { name: LitStr },
    Set { name: LitStr, value: LitStr },
}

impl Parse for HeaderRule {
    fn parse(input: ParseStream) -> Result<Self> {
        if input.peek(Token![-]) {
            input.parse::<Token![-]>()?;
            let n: LitStr = input.parse()?;
            return Ok(HeaderRule::Remove { name: n });
        }
        let name: LitStr = input.parse()?;
        input.parse::<Token![:]>()?;
        let value: LitStr = input.parse()?;
        Ok(HeaderRule::Set { name, value })
    }
}

#[derive(Debug)]
pub enum Node {
    Prefix(NodeBlock),
    Path(NodeBlock),
    Endpoint(Box<EndpointDecl>),
}

impl Parse for Node {
    fn parse(input: ParseStream) -> Result<Self> {
        if input.peek(prefix) {
            let nb: NodeBlock = input.parse()?;
            return Ok(Node::Prefix(nb));
        }
        if input.peek(path) {
            let nb: NodeBlock = input.parse()?;
            return Ok(Node::Path(nb));
        }
        // otherwise endpoint
        Ok(Node::Endpoint(input.parse()?))
    }
}

#[derive(Debug)]
pub struct NodeBlock {
    pub template: LitStr,
    pub children: Vec<Node>,
}

impl Parse for NodeBlock {
    fn parse(input: ParseStream) -> Result<Self> {
        if input.peek(prefix) {
            input.parse::<prefix>()?;
        } else if input.peek(path) {
            input.parse::<path>()?;
        } else {
            return Err(input.error("expected prefix or path"));
        };

        let template: LitStr = input.parse()?;
        let content;
        braced!(content in input);

        let mut children = Vec::new();
        while !content.is_empty() {
            children.push(content.parse()?);
        }

        Ok(Self { template, children })
    }
}

#[derive(Debug, Clone)]
pub struct CodecSpec {
    pub codec: Path, // struct type path (ex: JsonEncoding, MyCodec, some::Codec)
    pub ty: Type,    // decoded/body type
}

impl Parse for CodecSpec {
    fn parse(input: ParseStream) -> Result<Self> {
        let codec: Path = input.call(Path::parse_mod_style)?;
        input.parse::<Token![<]>()?;
        let ty: Type = input.parse()?;
        input.parse::<Token![>]>()?;
        Ok(Self { codec, ty })
    }
}

#[derive(Debug)]
pub struct QueryBlock {
    pub items: Vec<QueryItem>,
}

impl Parse for QueryBlock {
    fn parse(input: ParseStream) -> Result<Self> {
        input.parse::<query>()?;
        let content;
        braced!(content in input);

        let mut items = Vec::new();
        while !content.is_empty() {
            items.push(content.parse()?);
            if content.peek(Token![,]) {
                content.parse::<Token![,]>()?;
            }
        }
        Ok(Self { items })
    }
}

#[derive(Debug, Clone)]
pub struct QueryItem {
    pub name: Ident,
    pub optional: bool, // page?: T
    pub ty: Type,
    pub default: Option<Expr>,
}

impl Parse for QueryItem {
    fn parse(input: ParseStream) -> Result<Self> {
        let name: Ident = input.parse()?;
        let optional = if input.peek(Token![?]) {
            input.parse::<Token![?]>()?;
            true
        } else {
            false
        };
        input.parse::<Token![:]>()?;
        let ty: Type = input.parse()?;
        let default = if input.peek(Token![=]) {
            input.parse::<Token![=]>()?;
            Some(input.parse::<Expr>()?)
        } else {
            None
        };
        Ok(Self {
            name,
            optional,
            ty,
            default,
        })
    }
}

#[derive(Debug)]
pub struct EndpointDecl {
    pub method: Ident,
    pub path: LitStr,
    pub headers: Vec<HeaderRule>,
    pub query: Vec<QueryItem>,
    pub body: Option<CodecSpec>,
    pub resp: CodecSpec,
    pub map: Option<MapClause>,
}

#[derive(Debug, Clone)]
pub struct MapClause {
    pub out_ty: Type,
    pub expr: Expr,
}

impl Parse for EndpointDecl {
    fn parse(input: ParseStream) -> Result<Self> {
        let method: Ident = input.parse()?;
        let path: LitStr = input.parse()?;

        let mut headers_v = Vec::new();
        let mut query_v = Vec::new();
        let mut body_v = None;

        // optional blocks in any order before ->
        loop {
            if input.peek(headers) {
                let hb: HeadersBlock = input.parse()?;
                headers_v = hb.rules;
                continue;
            }
            if input.peek(query) {
                let qb: QueryBlock = input.parse()?;
                query_v = qb.items;
                continue;
            }
            if input.peek(body) {
                input.parse::<body>()?;
                body_v = Some(input.parse::<CodecSpec>()?);
                continue;
            }
            break;
        }

        input.parse::<Token![->]>()?;
        let resp: CodecSpec = input.parse()?;

        let map = if input.peek(Token![|]) {
            input.parse::<Token![|]>()?;
            // required: | OutType => expr
            let out_ty: Type = input.parse()?;
            input.parse::<Token![=>]>()?;
            let expr: Expr = input.parse()?;
            Some(MapClause { out_ty, expr })
        } else {
            None
        };

        input.parse::<Token![;]>()?;

        Ok(Self {
            method,
            path,
            headers: headers_v,
            query: query_v,
            body: body_v,
            resp,
            map,
        })
    }
}
