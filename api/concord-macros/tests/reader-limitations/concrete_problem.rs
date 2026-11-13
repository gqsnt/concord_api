concord::api! { client C { base "https://span.test"; read concord::Json;
problem std::rc::Rc<String>;
GET get(); } }
fn main() {}
