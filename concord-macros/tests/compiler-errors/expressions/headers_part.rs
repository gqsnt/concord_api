concord::api! { client Leak { base "https://api.example.com";
part tagged(tag: String) { headers { "tag" = format!("{tag}-{}", route.len()); } }
scope matches(route: String) { headers { "route" = route; }
GET get(..tagged); } } }
fn main() {}
