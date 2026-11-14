concord::api! { client C(root:String) { base "https://span.test"; read concord::Json;
GET get() -> Vec<u32> {
paginate move |_call, _page| { let _=client.root.len(); None::<c::Get> };
} } }
fn main() {}
