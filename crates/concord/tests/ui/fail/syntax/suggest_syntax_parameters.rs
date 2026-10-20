use concord::api;

api! { client QueryPlacement { base "https://example.test/"; values { QUERY page: u32; } } }
api! { client HeaderPlacement { base "https://example.test/"; values { haeder("X-Trace") trace: String; } } }
api! { client BodyPlacement { base "https://example.test/"; POST send(bodi input: String) -> (); } }
api! { client EachPlacement { base "https://example.test/"; GET read(query eac names: Vec<String>) -> (); } }
api! { client ValueVia { base "https://example.test/"; values { query page: u32 vai Encoder; } } }
api! { client ParameterVia { base "https://example.test/"; GET read(query page: u32 vai Encoder) -> (); } }
api! { client BodyVia { base "https://example.test/"; POST send(body input: String vai Representation) -> (); } }
api! { client OutputVia { base "https://example.test/"; GET read() -> String vai Representation; } }

fn main() {}
