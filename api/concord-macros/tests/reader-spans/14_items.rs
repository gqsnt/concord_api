concord::api! { client C { base "https://span.test"; read concord::Json;
GET get() -> u32 { paginate again; }
} }
fn again(_:c::Get,_:&concord::Answer<u32>)->Option<c::Get>{None}
fn caller(){ let _=C::new().get().pages().items(); }
fn main() {}
