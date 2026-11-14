concord::api! { client Leak { base "https://api.example.com";
part tagged(tag: String) { query { "tag" = format!("{tag}-{}", route.len()); } }
scope matches(route: String) { query { "route" = route; }
GET get(..tagged); } } }
fn main() {}
