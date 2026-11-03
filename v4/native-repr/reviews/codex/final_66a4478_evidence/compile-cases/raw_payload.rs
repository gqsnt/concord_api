#[allow(non_camel_case_types)] type r#json=String;
wire::api! { pub client Api { base "http://127.0.0.1"; response text; GET p() -> r#json; } }
