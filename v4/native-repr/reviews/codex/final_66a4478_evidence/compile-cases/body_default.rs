
wire::api! { pub client Api { base "http://127.0.0.1";  POST p(json p: String = String::new()); } }
