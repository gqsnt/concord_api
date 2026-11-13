concord::api! { client Leak { base "https://path.test"; part tagged(tag:String) { path "{route}/{tag}"; } scope matches(route:String) { path "{route}"; GET get(..tagged); } } }
fn main() {}
