concord::api! { client Leak { base "https://api.example.com";
part placed(route:String) { body concord::json { "route"=route; } }
part tagged(tag:String) { body concord::json { "tag"=format!("{tag}-{}", route.len()); } }
scope matches(route:String) { path "{route}"; POST get(..placed,..tagged); }
} }
fn main() {}
