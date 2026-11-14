#![allow(dead_code, non_upper_case_globals)]
mod constant_some {
    const Some: u32 = 7;
    concord::api! { pub client C { base "https://values.test"; HEAD get() -> u32 { status {2xx => Some} } } }
}
mod variant_some {
    pub enum Value { Some }
    use Value::Some;
    concord::api! { pub client C { base "https://values.test"; HEAD get() -> Value { status {2xx => Some} } } }
}
mod constant_ok {
    const Ok: u32 = 8;
    concord::api! { pub client C { base "https://values.test"; HEAD get() -> u32 { status {2xx => Ok} } } }
}
mod variant_ok {
    pub enum Value { Ok }
    use Value::Ok;
    concord::api! { pub client C { base "https://values.test"; HEAD get() -> Value { status {2xx => Ok} } } }
}
mod constant_err {
    const Err: u32 = 9;
    concord::api! { pub client C { base "https://values.test"; HEAD get() -> u32 { status {2xx => Err} } } }
}
mod variant_err {
    pub enum Value { Err }
    use Value::Err;
    concord::api! { pub client C { base "https://values.test"; HEAD get() -> Value { status {2xx => Err} } } }
}
fn main() {
    constant_some::C::new().get().request().unwrap();
    variant_some::C::new().get().request().unwrap();
    constant_ok::C::new().get().request().unwrap();
    variant_ok::C::new().get().request().unwrap();
    constant_err::C::new().get().request().unwrap();
    variant_err::C::new().get().request().unwrap();
}
