use concord::api;

api! { cliet Root {} }
api! { client Base { bsae "https://example.test/"; } }
api! { client Scope { base "https://example.test/"; scpoe region() {} } }
api! { client Method { base "https://example.test/"; get read() -> (); } }
api! { client MethodTranspose { base "https://example.test/"; DELEET remove() -> (); } }
api! { client CustomMethod { base "https://example.test/"; METHDO("PURGE") purge() -> (); } }
api! { client ScopeClause { base "https://example.test/"; scope region() { respnose Bytes; } } }
api! { client EndpointClause { base "https://example.test/"; GET read() -> () { statuz success; } } }
api! { client EndpointEffect { base "https://example.test/"; GET read() -> () { effect nonIdempotent; } } }
api! { client EndpointExpose { base "https://example.test/"; GET read() -> () { expose acceptedBytes; } } }
api! { client EndpointStatus { base "https://example.test/"; GET read() -> () { status sucess; } } }
api! { client EndpointAbsence { base "https://example.test/"; GET read() -> () { absent no 404; } } }
api! { client Unrelated { base "https://example.test/"; GET read() -> () { zzyzx; } } }
// An endpoint cannot own a base, so its typo must not suggest `base`.
api! { client InvalidCandidate { base "https://example.test/"; GET read() -> () { bsae "https://elsewhere.test/"; } } }

api! { client ClientHeaders { base "https://example.test/"; heders {} } }
api! { client ScopeHeaders { base "https://example.test/"; scope region() { heders {} } } }
api! { client EndpointHeaders { base "https://example.test/"; GET read() -> () { heders {} } } }

fn main() {}
