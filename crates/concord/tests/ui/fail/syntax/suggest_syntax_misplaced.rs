use concord::api;

api! { client ClientEndpointClause { base "https://example.test/"; paginate via Controller {} } }
api! { client ScopeClientClause { base "https://example.test/"; scope region() { values {} } } }
api! { client ScopeEndpointClause { base "https://example.test/"; scope region() { effect safe; } } }
api! { client EndpointClientClause { base "https://example.test/"; GET read() -> () { credentials {} } } }
api! { client EndpointSubdomain { base "https://example.test/"; GET read() -> () { subdomain ["api"]; } } }
api! { client EndpointRepresentation { base "https://example.test/"; GET read() -> () { response Bytes; } } }
api! { client EndpointScope { base "https://example.test/"; GET read() -> () { scope nested() {} } } }
api! { client SecretPlacement { base "https://example.test/"; secret token; } }

fn main() {}
