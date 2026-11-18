// What the macro writes does not depend on the names in the author's scope (C10.1):
// a type of the author's named `str` changes neither the endpoint's name nor its call.
#![allow(non_camel_case_types)]
pub type str = u32;

concord::api! {
    pub client Own {
        base "https://a.test";
        GET check();
        scope inner { GET nested(); }
    }
}

fn main() {
    let _: str = 7;
    assert_eq!(<own::Check as own::Endpoint>::NAME, "own.check");
    assert_eq!(<own::inner::Nested as own::Endpoint>::NAME, "own.inner.nested");
    assert!(format!("{:?}", Own::new().check()).starts_with("Check"));
}
